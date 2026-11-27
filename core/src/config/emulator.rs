//! The CPU-neutral half of building an emulator from configuration:
//! [`build_machine`] instantiates the configured devices onto a bus and wraps
//! it in a [`MachineState`]. Each CPU family's configuration (for the 6502,
//! `emma65::mos6502::config::Config`) builds its CPU around the result.
use super::{DeviceModuleError, DeviceRegistry, DeviceSpec, InstantiationContext};
use crate::bus::vector::IdentityVectorResolver;
use crate::bus::{DeviceIdAllocator, InterruptLine, MAX_IRQ_SOURCES};
use crate::device::{DeviceId, device_event_channel};
use crate::{
    BusConfig, ClockSpeed, CpuBuildError, ErrorReceiver, LogSender, MachineState, UnmappedPolicy,
};
use figment::value::Value;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

/// Policy for accesses to unmapped bus addresses, as set via the top-level
/// `unmapped-policy` configuration attribute.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum UnmappedPolicySpec {
    /// Reads return a default value and writes are silently discarded.
    Ignore,
    /// Reads and writes to unmapped addresses return a `BusError::Unmapped`.
    Error,
}

impl UnmappedPolicySpec {
    fn to_unmapped_policy(&self) -> UnmappedPolicy {
        match self {
            UnmappedPolicySpec::Ignore => UnmappedPolicy::DefaultValue,
            UnmappedPolicySpec::Error => UnmappedPolicy::Error,
        }
    }
}

impl Display for UnmappedPolicySpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            UnmappedPolicySpec::Ignore => write!(f, "ignore"),
            UnmappedPolicySpec::Error => write!(f, "error"),
        }
    }
}

impl From<UnmappedPolicySpec> for String {
    fn from(v: UnmappedPolicySpec) -> Self {
        v.to_string()
    }
}

impl TryFrom<String> for UnmappedPolicySpec {
    type Error = String;

    fn try_from(s: String) -> Result<Self, <Self as TryFrom<String>>::Error> {
        s.parse()
    }
}

impl FromStr for UnmappedPolicySpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "ignore" => Ok(UnmappedPolicySpec::Ignore),
            "error" => Ok(UnmappedPolicySpec::Error),
            _ => Err(format!("Invalid unmapped policy '{s}'")),
        }
    }
}

/// An error that occurs during emulator configuration or startup.
#[derive(Debug)]
pub enum BuildError {
    /// An error that occurred while creating and configuring the CPU.
    Cpu(CpuBuildError),
    /// An error that occurred while instantiating a device module.
    Device {
        module_name: String,
        address: u16,
        source: DeviceModuleError,
    },
}

impl Display for BuildError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::Cpu(e) => write!(f, "CPU configuration error: {e}"),
            BuildError::Device {
                module_name,
                address,
                source,
            } => write!(
                f,
                "failed to configure device '{module_name}' at address {address:#06x}: {source}"
            ),
        }
    }
}

/// The CPU-neutral settings [`build_machine`] needs.
pub struct MachineSpec<'a> {
    /// Device config specifications, instantiated in order.
    pub devices: &'a [DeviceSpec],
    /// Clock speed to simulate via throttling, or `None` for unlimited.
    pub clock_speed_hz: Option<u64>,
    /// Policy for accesses to unmapped addresses, or `None` for the bus default.
    pub unmapped_policy: Option<&'a UnmappedPolicySpec>,
    /// The interrupt inputs the CPU has. A device can be routed (with its `interrupt`
    /// attribute) only to a maskable line in this list.
    pub interrupt_lines: &'a [InterruptLine],
}

/// The device attribute that routes a device's interrupt output to a CPU input line.
/// [`build_machine`] handles it, so device modules never see it.
pub const INTERRUPT_ATTRIBUTE: &str = "interrupt";

/// The result of [`build_machine`]: machine state ready to be wrapped in a CPU.
pub struct BuiltMachine {
    /// Bus, interrupt controller, clock speed, and vector resolver for the CPU.
    pub machine: MachineState,
    /// Receiver for errors emitted by machine components.
    pub error_receiver: ErrorReceiver,
    /// Device ID allocator, left in its post-configuration state.
    pub id_allocator: DeviceIdAllocator,
}

/// Instantiates `spec`'s devices through `registry` and returns the machine
/// state a CPU is built around.
///
/// A new error channel is always created, and `error_receiver` is its
/// receiving end. The devices report through that channel unless
/// `context.error_sender` already supplies a sender, in which case they use
/// the caller's and `error_receiver` never receives anything. The vector
/// resolver is the one a device module installed on the bus, or
/// [`IdentityVectorResolver`] if none did.
///
/// Each device's [`INTERRUPT_ATTRIBUTE`] (`irq`, the default, or `firq`) is removed from
/// its attributes before its module sees them. The interrupt-capable devices a
/// FIRQ-routed spec creates go in the interrupt controller's FIRQ mask. Routing to a line
/// that isn't in `spec.interrupt_lines`, or routing a device with no interrupt output to
/// FIRQ, is an error.
pub async fn build_machine(
    registry: &DeviceRegistry,
    spec: MachineSpec<'_>,
    context: InstantiationContext,
) -> Result<BuiltMachine, BuildError> {
    let (error_sender, error_receiver) = device_event_channel();
    let context = InstantiationContext {
        error_sender: context.error_sender.or(Some(error_sender)),
        ..context
    };

    let mut bus_config = BusConfig::new();
    let id_allocator = Arc::new(Mutex::new(DeviceIdAllocator::new()));
    let mut firq_mask = 0u64;
    for device in spec.devices {
        let device_error = |source| BuildError::Device {
            module_name: device.module_name().to_string(),
            address: device.address(),
            source,
        };
        let (line, attributes) =
            take_interrupt_line(device.attributes(), spec.interrupt_lines).map_err(device_error)?;
        let first_new_device = bus_config.device_ids().len();
        bus_config = registry
            .instantiate(
                device.module_name(),
                bus_config,
                device.address(),
                &attributes,
                &context,
                id_allocator.clone(),
            )
            .await
            .map_err(device_error)?;
        if line == InterruptLine::Firq {
            let sources = irq_source_bits(bus_config.device_ids().skip(first_new_device));
            if sources == 0 {
                return Err(device_error(DeviceModuleError::Config(format!(
                    "this device has no interrupt output to route to {line}"
                ))));
            }
            firq_mask |= sources;
        }
    }

    if let Some(policy) = spec.unmapped_policy {
        bus_config = bus_config.unmapped_policy(policy.to_unmapped_policy());
    }
    let vector_resolver = bus_config
        .take_vector_resolver()
        .unwrap_or_else(|| Box::new(IdentityVectorResolver));
    let bus = bus_config.build();
    let mut machine = MachineState::new(
        bus,
        spec.clock_speed_hz
            .map_or(ClockSpeed::unlimited(), ClockSpeed::hz),
        vector_resolver,
        LogSender::default(),
    );
    machine.interrupts_mut().set_firq_mask(firq_mask);
    let id_allocator = *id_allocator.lock().unwrap();
    Ok(BuiltMachine {
        machine,
        error_receiver,
        id_allocator,
    })
}

/// Parses and removes a device's [`INTERRUPT_ATTRIBUTE`], returning the line it routes
/// the device to (IRQ if the attribute is absent) and the attributes its module sees.
/// Borrows the attributes unchanged when the attribute is absent.
fn take_interrupt_line<'a>(
    attributes: &'a HashMap<String, Value>,
    supported: &[InterruptLine],
) -> Result<(InterruptLine, Cow<'a, HashMap<String, Value>>), DeviceModuleError> {
    let Some(value) = attributes.get(INTERRUPT_ATTRIBUTE) else {
        return Ok((InterruptLine::Irq, Cow::Borrowed(attributes)));
    };
    let routable: Vec<&str> = supported
        .iter()
        .filter(|line| matches!(line, InterruptLine::Irq | InterruptLine::Firq))
        .map(|line| line.name())
        .collect();
    let expected = routable.join(" or ");
    let name = value.as_str().ok_or_else(|| {
        DeviceModuleError::Config(format!(
            "{INTERRUPT_ATTRIBUTE} must be a line name ({expected})"
        ))
    })?;
    let line = match name.to_ascii_lowercase().as_str() {
        "irq" => InterruptLine::Irq,
        "firq" => InterruptLine::Firq,
        _ => {
            return Err(DeviceModuleError::Config(format!(
                "invalid {INTERRUPT_ATTRIBUTE} line '{name}' (expected {expected})"
            )));
        }
    };
    if !supported.contains(&line) {
        return Err(DeviceModuleError::Config(format!(
            "this CPU has no {line} input ({INTERRUPT_ATTRIBUTE} must be {expected})"
        )));
    }
    let mut attributes = attributes.clone();
    attributes.remove(INTERRUPT_ATTRIBUTE);
    Ok((line, Cow::Owned(attributes)))
}

/// Returns the interrupt-controller source bits of the interrupt-capable devices in
/// `ids` (those whose IDs are below [`MAX_IRQ_SOURCES`]).
fn irq_source_bits(ids: impl Iterator<Item = DeviceId>) -> u64 {
    ids.filter(|id| id.0 < MAX_IRQ_SOURCES)
        .fold(0, |bits, id| bits | (1u64 << id.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::AddressRange;
    use crate::config::DeviceModule;
    use crate::config::{Mc6850Module, RamModule};
    use crate::device::IoDevice;

    const IRQ_NMI: &[InterruptLine] = &[InterruptLine::Irq, InterruptLine::Nmi];
    const IRQ_FIRQ_NMI: &[InterruptLine] =
        &[InterruptLine::Irq, InterruptLine::Firq, InterruptLine::Nmi];

    /// A device that asserts its interrupt output constantly.
    struct AssertingDevice;

    impl IoDevice for AssertingDevice {
        fn read(&mut self, _address: u16) -> u8 {
            0
        }
        fn write(&mut self, _address: u16, _value: u8) {}
        fn peek(&self, _address: u16) -> u8 {
            0
        }
        fn irq_active(&self) -> bool {
            true
        }
        fn identity_address(&self) -> u16 {
            0x8000
        }
    }

    /// Maps an [`AssertingDevice`] at its address, as interrupt source `irq` (an attribute,
    /// required). Fails on any other attribute, as a module with `deny_unknown_fields` would.
    #[derive(Clone)]
    struct AssertingModule;

    impl DeviceModule for AssertingModule {
        fn name(&self) -> &'static str {
            "asserting"
        }

        async fn instantiate(
            &self,
            bus_config: BusConfig,
            address: u16,
            attributes: &HashMap<String, Value>,
            _context: &InstantiationContext,
            id_allocator: Arc<Mutex<DeviceIdAllocator>>,
        ) -> Result<BusConfig, DeviceModuleError> {
            if let Some(key) = attributes.keys().find(|key| *key != "irq") {
                return Err(DeviceModuleError::Config(format!(
                    "unknown attribute {key}"
                )));
            }
            let irq = attributes
                .get("irq")
                .and_then(|value| value.deserialize::<u32>().ok())
                .expect("irq attribute");
            let id = id_allocator
                .lock()
                .unwrap()
                .for_irq(irq)
                .map_err(DeviceModuleError::BusConfig)?;
            bus_config
                .device(
                    AddressRange::new(address, address),
                    id,
                    Box::new(AssertingDevice),
                )
                .map_err(DeviceModuleError::BusConfig)
        }
    }

    fn registry() -> DeviceRegistry {
        let mut registry = DeviceRegistry::new();
        registry.register(AssertingModule);
        registry.register(Mc6850Module);
        registry.register(RamModule);
        registry
    }

    fn specs(specs: &[&str]) -> Vec<DeviceSpec> {
        specs.iter().map(|spec| spec.parse().unwrap()).collect()
    }

    async fn build(
        devices: &[DeviceSpec],
        interrupt_lines: &[InterruptLine],
    ) -> Result<BuiltMachine, BuildError> {
        build_machine(
            &registry(),
            MachineSpec {
                devices,
                clock_speed_hz: None,
                unmapped_policy: None,
                interrupt_lines,
            },
            InstantiationContext::default(),
        )
        .await
    }

    /// Builds `devices`, which must succeed, and polls the devices once.
    async fn build_and_poll(devices: &[&str], interrupt_lines: &[InterruptLine]) -> MachineState {
        let mut machine = build(&specs(devices), interrupt_lines)
            .await
            .unwrap_or_else(|e| panic!("build failed: {e}"))
            .machine;
        machine.tick_idle();
        machine
    }

    fn config_error(result: Result<BuiltMachine, BuildError>) -> String {
        match result {
            Err(BuildError::Device {
                source: DeviceModuleError::Config(message),
                ..
            }) => message,
            Err(e) => panic!("expected a device configuration error, got {e}"),
            Ok(_) => panic!("expected a device configuration error, got a machine"),
        }
    }

    #[tokio::test]
    async fn devices_route_to_irq_by_default() {
        let machine = build_and_poll(&["asserting@0x8000,irq=3"], IRQ_FIRQ_NMI).await;
        assert_eq!(machine.interrupts().firq_mask(), 0);
        assert!(machine.interrupts().irq_active());
        assert!(!machine.interrupts().firq_active());
        assert!(machine.irq_recognized());
    }

    #[tokio::test]
    async fn explicit_irq_routing_is_accepted_and_stripped() {
        let machine = build_and_poll(&["asserting@0x8000,irq=3,interrupt=irq"], IRQ_NMI).await;
        assert_eq!(machine.interrupts().firq_mask(), 0);
        assert!(machine.interrupts().irq_active());
    }

    #[tokio::test]
    async fn firq_routed_device_raises_firq_not_irq() {
        let machine =
            build_and_poll(&["asserting@0x8000,irq=3,interrupt=firq"], IRQ_FIRQ_NMI).await;
        assert_eq!(machine.interrupts().firq_mask(), 1 << 3);
        assert!(machine.interrupts().firq_active());
        assert!(!machine.interrupts().irq_active());
    }

    #[tokio::test]
    async fn firq_routed_device_is_invisible_to_irq_recognition() {
        let machine =
            build_and_poll(&["asserting@0x8000,irq=3,interrupt=FIRQ"], IRQ_FIRQ_NMI).await;
        assert!(!machine.irq_recognized());
        assert_eq!(machine.interrupts().active_sources_mask(), 0);
    }

    #[tokio::test]
    async fn irq_and_firq_routed_devices_drive_their_own_lines() {
        let machine = build_and_poll(
            &[
                "asserting@0x8000,irq=2",
                "asserting@0x8001,irq=5,interrupt=firq",
            ],
            IRQ_FIRQ_NMI,
        )
        .await;
        assert_eq!(machine.interrupts().firq_mask(), 1 << 5);
        assert_eq!(machine.interrupts().active_sources_mask(), 1 << 2);
        assert!(machine.interrupts().irq_active());
        assert!(machine.interrupts().firq_active());
        assert!(machine.irq_recognized());
    }

    #[tokio::test]
    async fn interrupt_attribute_is_hidden_from_modules_that_deny_unknown_fields() {
        let built = build(&specs(&["acia/6850@0x8000,interrupt=firq"]), IRQ_FIRQ_NMI)
            .await
            .unwrap_or_else(|e| panic!("build failed: {e}"));
        assert_ne!(built.machine.interrupts().firq_mask(), 0);
    }

    #[tokio::test]
    async fn firq_is_rejected_by_a_cpu_without_firq() {
        let message =
            config_error(build(&specs(&["asserting@0x8000,irq=3,interrupt=firq"]), IRQ_NMI).await);
        assert!(message.contains("no firq input"), "{message}");
        assert!(message.contains("must be irq"), "{message}");
    }

    #[tokio::test]
    async fn unknown_interrupt_line_is_rejected() {
        for line in ["nmi", "bogus"] {
            let message = config_error(
                build(
                    &specs(&[&format!("asserting@0x8000,irq=3,interrupt={line}")]),
                    IRQ_FIRQ_NMI,
                )
                .await,
            );
            assert!(
                message.contains(&format!("invalid interrupt line '{line}'")),
                "{message}"
            );
            assert!(message.contains("expected irq or firq"), "{message}");
        }
    }

    #[tokio::test]
    async fn firq_routing_a_device_with_no_interrupt_output_is_rejected() {
        let message =
            config_error(build(&specs(&["ram@0x0000,size=1K,interrupt=firq"]), IRQ_FIRQ_NMI).await);
        assert!(message.contains("no interrupt output"), "{message}");
    }
}
