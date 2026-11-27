//! The 6502's emulator configuration: the top-level TOML/CLI schema
//! ([`Config`]), including the 65C02 variant selector ([`CpuVariantSpec`]), and
//! building a 6502 [`EmulatorSession`] from it.
//!
//! The CPU-neutral work (instantiating devices onto the bus) is
//! [`build_machine`]; this module wraps the resulting machine state in a
//! [`Cpu`].
use self::CpuVariantSpec::{Cmos6502, Wdc6502};
use crate::mos6502::EmulatorSession;
use crate::mos6502::cpu::Cpu;
use crate::mos6502::cpu::variant::CpuVariant;
use clap::Parser;
use emma6k_core::config::{
    BuildError, DeviceRegistry, DeviceSpec, InstantiationContext, MachineSpec, UnmappedPolicySpec,
    build_machine,
};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// The 65C02 variant to emulate, as set via the top-level `cpu-variant`
/// configuration attribute.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum CpuVariantSpec {
    Cmos6502,
    Wdc6502,
}

impl CpuVariantSpec {
    fn to_cpu_variant(&self) -> CpuVariant {
        match self {
            Cmos6502 => CpuVariant::Cmos65C02,
            Wdc6502 => CpuVariant::Wdc65C02,
        }
    }
}

impl Display for CpuVariantSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Cmos6502 => write!(f, "65C02"),
            Wdc6502 => write!(f, "WDC65C02"),
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
        let upper_s = s.to_ascii_uppercase();
        let us = upper_s.as_str();
        match us {
            "65C02" | "C02" => Ok(Cmos6502),
            "WDC65C02" | "WDC02" => Ok(Wdc6502),
            _ => Err(format!("Invalid CPU variant '{s}'")),
        }
    }
}

#[derive(Debug, Clone, Parser, Serialize, Deserialize)]
#[clap(name = "emulator")]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
/// Configuration attributes for the emulator.
pub struct Config {
    /// Selected CPU variant (e.g. 65C02, WDC65C02).
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
    pub async fn build(&self, registry: &DeviceRegistry) -> Result<EmulatorSession, BuildError> {
        let context = InstantiationContext {
            clock_hz: self.clock_speed_hz,
            ..Default::default()
        };
        self.build_session(registry, context).await
    }

    /// Builds an [`EmulatorSession`] using the provided [`InstantiationContext`].
    ///
    /// Use this when the caller needs to inject a pre-created console transport or
    /// supply a custom error sender before building. If `context.error_sender` is
    /// `None`, a new error channel is created and its receiver is stored in the session.
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
            },
            context,
        )
        .await?;

        let variant = self
            .cpu_variant_spec
            .as_ref()
            .map_or(CpuVariant::Cmos65C02, CpuVariantSpec::to_cpu_variant);
        let cpu = Cpu::builder(variant)
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
mod tests {
    use super::*;
    use emma6k_core::BusConfig;
    use emma6k_core::bus::DeviceIdAllocator;
    use emma6k_core::config::DeviceModuleError;
    use emma6k_core::{BusConfigError, DeviceModule, IdentityVectorResolver};
    use figment::value::Value;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// A test-only device module that installs an `IdentityVectorResolver` on the
    /// `BusConfig` it's handed, used to exercise the "at most one PIC" rule.
    #[derive(Clone)]
    struct ResolverInstallerModule;

    impl DeviceModule for ResolverInstallerModule {
        fn name(&self) -> &'static str {
            "resolver-installer"
        }

        async fn instantiate(
            &self,
            bus_config: BusConfig,
            _address: u16,
            _attributes: &HashMap<String, Value>,
            _context: &InstantiationContext,
            _id_allocator: Arc<Mutex<DeviceIdAllocator>>,
        ) -> Result<BusConfig, DeviceModuleError> {
            bus_config
                .vector_resolver(Box::new(IdentityVectorResolver))
                .map_err(DeviceModuleError::BusConfig)
        }
    }

    #[tokio::test]
    async fn build_fails_when_two_device_specs_install_a_vector_resolver() {
        let mut registry = DeviceRegistry::new();
        registry.register(ResolverInstallerModule);
        let config = Config {
            cpu_variant_spec: None,
            clock_speed_hz: None,
            devices: Some(vec![
                "resolver-installer@0x1000".parse().unwrap(),
                "resolver-installer@0x2000".parse().unwrap(),
            ]),
            unmapped_policy: None,
        };
        let err = config.build(&registry).await.err().unwrap();
        assert!(matches!(
            err,
            BuildError::Device {
                source: DeviceModuleError::BusConfig(BusConfigError::DuplicateVectorResolver),
                ..
            }
        ));
    }

    #[test]
    fn config_rejects_unrecognized_top_level_attribute() {
        use figment::Figment;
        use figment::providers::{Format, Toml};

        let result: Result<Config, _> = Figment::new()
            .merge(Toml::string(
                r#"
                cpu-variant = "65C02"
                bogus-attribute = true
                "#,
            ))
            .extract();

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn build_installs_resolver_from_a_single_device_spec() {
        let mut registry = DeviceRegistry::new();
        registry.register(ResolverInstallerModule);
        let config = Config {
            cpu_variant_spec: None,
            clock_speed_hz: None,
            devices: Some(vec!["resolver-installer@0x1000".parse().unwrap()]),
            unmapped_policy: None,
        };
        assert!(config.build(&registry).await.is_ok());
    }

    #[tokio::test]
    async fn build_applies_unmapped_policy_error() {
        let registry = DeviceRegistry::with_builtins();
        let config = Config {
            cpu_variant_spec: None,
            clock_speed_hz: None,
            devices: Some(vec!["ram@0x0000,size=256,fill=0".parse().unwrap()]),
            unmapped_policy: Some(UnmappedPolicySpec::Error),
        };
        let mut session = config.build(&registry).await.unwrap();
        assert!(matches!(
            session.cpu.bus_mut().read(0x1000),
            Err(emma6k_core::BusError::Unmapped { addr: 0x1000 })
        ));
    }

    #[test]
    fn unmapped_policy_toml_values_parse() {
        use figment::Figment;
        use figment::providers::{Format, Toml};

        let config: Config = Figment::new()
            .merge(Toml::string(r#"unmapped-policy = "error""#))
            .extract()
            .unwrap();
        assert!(matches!(
            config.unmapped_policy,
            Some(UnmappedPolicySpec::Error)
        ));

        let config: Config = Figment::new()
            .merge(Toml::string(r#"unmapped-policy = "ignore""#))
            .extract()
            .unwrap();
        assert!(matches!(
            config.unmapped_policy,
            Some(UnmappedPolicySpec::Ignore)
        ));
    }
}
