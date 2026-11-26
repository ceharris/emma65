//! The CPU-neutral half of building an emulator from configuration:
//! [`build_machine`] instantiates the configured devices onto a bus and wraps
//! it in a [`MachineState`]. Each CPU family's configuration (for the 6502,
//! `emma65::mos6502::config::Config`) builds its CPU around the result.
use super::{DeviceModuleError, DeviceRegistry, DeviceSpec, InstantiationContext};
use crate::bus::DeviceIdAllocator;
use crate::bus::vector::IdentityVectorResolver;
use crate::device::device_event_channel;
use crate::{
    BusConfig, ClockSpeed, CpuBuildError, ErrorReceiver, LogSender, MachineState, UnmappedPolicy,
};
use serde::{Deserialize, Serialize};
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
}

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
    for device in spec.devices {
        bus_config = registry
            .instantiate(
                device.module_name(),
                bus_config,
                device.address(),
                device.attributes(),
                &context,
                id_allocator.clone(),
            )
            .await
            .map_err(|e| BuildError::Device {
                module_name: device.module_name().to_string(),
                address: device.address(),
                source: e,
            })?;
    }

    if let Some(policy) = spec.unmapped_policy {
        bus_config = bus_config.unmapped_policy(policy.to_unmapped_policy());
    }
    let vector_resolver = bus_config
        .take_vector_resolver()
        .unwrap_or_else(|| Box::new(IdentityVectorResolver));
    let bus = bus_config.build();
    let machine = MachineState::new(
        bus,
        spec.clock_speed_hz
            .map_or(ClockSpeed::unlimited(), ClockSpeed::hz),
        vector_resolver,
        LogSender::default(),
    );
    let id_allocator = *id_allocator.lock().unwrap();
    Ok(BuiltMachine {
        machine,
        error_receiver,
        id_allocator,
    })
}
