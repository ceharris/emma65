//! The 6809's emulator configuration: the top-level TOML/CLI schema
//! ([`Config`]), its CPU variant selector ([`CpuVariantSpec`]), the 6809's
//! device registry, and building a 6809 [`EmulatorSession`] from them.
//!
//! The CPU-neutral work (instantiating devices onto the bus) is
//! [`build_machine`]; this module wraps the resulting machine state in a
//! [`Cpu`].

use crate::EmulatorSession;
use crate::cpu::Cpu;
use clap::Parser;
use emma6k_core::InterruptLine;
use emma6k_core::config::{
    BuildError, DeviceModule, DeviceRegistry, DeviceSpec, InstantiationContext, MachineSpec,
    PicFinchModule, UnmappedPolicySpec, build_machine,
};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// The 6809's interrupt inputs. A device may be routed to IRQ or FIRQ with
/// its `interrupt` attribute.
pub const INTERRUPT_LINES: &[InterruptLine] =
    &[InterruptLine::Irq, InterruptLine::Firq, InterruptLine::Nmi];

/// The 6809 variant to emulate, as set by the top-level `cpu-variant`
/// configuration attribute. Values are case-insensitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum CpuVariantSpec {
    /// The Motorola MC6809 (`"6809"`).
    Mc6809,
}

impl Display for CpuVariantSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CpuVariantSpec::Mc6809 => write!(f, "6809"),
        }
    }
}

impl From<CpuVariantSpec> for String {
    fn from(v: CpuVariantSpec) -> Self {
        v.to_string()
    }
}

impl TryFrom<String> for CpuVariantSpec {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl FromStr for CpuVariantSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "6809" => Ok(CpuVariantSpec::Mc6809),
            "6309" => Err("the 6309 isn't supported yet".to_string()),
            _ => Err(format!("Invalid CPU variant '{s}'")),
        }
    }
}

/// Returns the device registry for a 6809 machine: every built-in device
/// except those that assume the 6502. That's `pic/finch`, which resolves the
/// 6502's IRQ vector; configuring it is an unknown-device-type error.
pub fn device_registry() -> DeviceRegistry {
    let mut registry = DeviceRegistry::with_builtins();
    registry.unregister(PicFinchModule.name());
    registry
}

#[derive(Debug, Clone, Parser, Serialize, Deserialize)]
#[clap(name = "emma09")]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
/// Configuration attributes for a 6809 emulator.
pub struct Config {
    /// Selected CPU variant (`6809`).
    #[serde(rename = "cpu-variant", skip_serializing_if = "Option::is_none")]
    #[clap(long = "cpu-variant")]
    pub cpu_variant_spec: Option<CpuVariantSpec>,

    /// Clock speed to simulate via throttling.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[clap(long = "clock-speed-hz")]
    pub clock_speed_hz: Option<u64>,

    /// Device config specifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[clap(long = "device", num_args = 1..)]
    pub devices: Option<Vec<DeviceSpec>>,

    /// Policy for accesses to unmapped addresses: "ignore" (read as a default
    /// value, discard writes) or "error" (fail the access). Defaults to
    /// "ignore".
    #[serde(rename = "unmapped-policy", skip_serializing_if = "Option::is_none")]
    #[clap(long = "unmapped-policy")]
    pub unmapped_policy: Option<UnmappedPolicySpec>,
}

impl Config {
    /// Builds an [`EmulatorSession`] using a default [`InstantiationContext`].
    /// Pass [`device_registry`] for the 6809's built-in devices.
    pub async fn build(&self, registry: &DeviceRegistry) -> Result<EmulatorSession, BuildError> {
        let context = InstantiationContext {
            clock_hz: self.clock_speed_hz,
            ..Default::default()
        };
        self.build_session(registry, context).await
    }

    /// Builds an [`EmulatorSession`] using the provided [`InstantiationContext`].
    ///
    /// If `context.error_sender` is `None`, a new error channel is created and
    /// its receiver is stored in the session.
    pub async fn build_with_context(
        &self,
        registry: &DeviceRegistry,
        context: InstantiationContext,
    ) -> Result<EmulatorSession, BuildError> {
        self.build_session(registry, context).await
    }

    async fn build_session(
        &self,
        registry: &DeviceRegistry,
        context: InstantiationContext,
    ) -> Result<EmulatorSession, BuildError> {
        let devices: &[DeviceSpec] = self.devices.as_deref().unwrap_or(&[]);
        let built = build_machine(
            registry,
            MachineSpec {
                devices,
                clock_speed_hz: self.clock_speed_hz,
                unmapped_policy: self.unmapped_policy.as_ref(),
                interrupt_lines: INTERRUPT_LINES,
            },
            context,
        )
        .await?;

        // The 6809 is the only variant, so `cpu_variant_spec` needs no
        // mapping yet.
        let cpu = Cpu::builder()
            .machine(built.machine)
            .build()
            .map_err(BuildError::Cpu)?;

        Ok(EmulatorSession {
            cpu,
            error_receiver: built.error_receiver,
            id_allocator: built.id_allocator,
        })
    }
}

#[cfg(test)]
mod tests;
