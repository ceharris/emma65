use super::*;
use crate::cpu::{FIRQ_VECTOR, IRQ_VECTOR, RESET_VECTOR, StepResult};
use emma6k_core::Processor;
use emma6k_core::config::DeviceModuleError;
use figment::Figment;
use figment::providers::{Format, Toml};

fn config(toml: &str) -> Config {
    Figment::new().merge(Toml::string(toml)).extract().unwrap()
}

#[test]
fn cpu_variant_parses_6809_in_any_case() {
    assert_eq!("6809".parse(), Ok(CpuVariantSpec::Mc6809));
    assert_eq!(
        config(r#"cpu-variant = "6809""#).cpu_variant_spec,
        Some(CpuVariantSpec::Mc6809)
    );
    assert_eq!(CpuVariantSpec::Mc6809.to_string(), "6809");
}

#[test]
fn cpu_variant_rejects_the_6309_and_other_cpus() {
    assert_eq!(
        "6309".parse::<CpuVariantSpec>(),
        Err("the 6309 isn't supported yet".to_string())
    );
    assert_eq!(
        "65C02".parse::<CpuVariantSpec>(),
        Err("Invalid CPU variant '65C02'".to_string())
    );
}

#[test]
fn config_rejects_an_unrecognized_top_level_attribute() {
    let result: Result<Config, _> = Figment::new()
        .merge(Toml::string("bogus-attribute = true"))
        .extract();
    assert!(result.is_err());
}

#[tokio::test]
async fn the_6809_registry_leaves_out_the_6502_pic() {
    let err = config(
        r#"
        [[devices]]
        type = "pic/finch"
        address = 0xFFFF
        "#,
    )
    .build(&device_registry())
    .await
    .err()
    .expect("pic/finch is 6502-only");
    assert!(
        matches!(&err, BuildError::Device { source: DeviceModuleError::Config(s), .. }
            if s == "unknown device type: pic/finch"),
        "{err}"
    );
}

#[tokio::test]
async fn the_6809_accepts_irq_and_firq_routing_but_not_nmi() {
    for (line, ok) in [("irq", true), ("firq", true), ("nmi", false)] {
        let result = config(&format!(
            r#"
            [[devices]]
            type = "acia/6850"
            address = 0x8000
            interrupt = "{line}"
            "#
        ))
        .build(&device_registry())
        .await;
        assert_eq!(result.is_ok(), ok, "interrupt = {line}");
    }
}

/// Builds a 64K-RAM machine with an MC6850 at `$8000`, routed by
/// `interrupt` (or by default when `None`), loads a program that enables the
/// ACIA's transmit interrupt and unmasks IRQ and FIRQ, and returns the PC the
/// CPU first jumps to.
async fn first_handler(interrupt: Option<&str>) -> u16 {
    const START: u16 = 0x1000;
    const IRQ_HANDLER: u16 = 0x2000;
    const FIRQ_HANDLER: u16 = 0x2100;
    // The emulated MC6850 enables its transmit interrupt with control bits
    // CR6:CR5 = 10; CR1:CR0 = 01 selects ÷16.
    const ACIA_TX_IRQ_ENABLE: u8 = 0x41;
    let routing = interrupt.map_or(String::new(), |line| format!("interrupt = \"{line}\""));
    let mut session = config(&format!(
        r#"
        [[devices]]
        type = "ram"
        address = 0x0000
        size = 0x10000
        [[devices]]
        type = "acia/6850"
        address = 0x8000
        {routing}
        "#
    ))
    .build(&device_registry())
    .await
    .unwrap();
    let cpu = &mut session.cpu;
    let program = [
        0x10,
        0xCE,
        0x70,
        0x00, // LDS #$7000
        0x86,
        0x03, //             LDA #$03 (master reset)
        0xB7,
        0x80,
        0x00, //       STA $8000
        0x86,
        ACIA_TX_IRQ_ENABLE, // LDA #$41
        0xB7,
        0x80,
        0x00, //       STA $8000
        0x1C,
        0xAF, //             ANDCC #$AF (unmask IRQ and FIRQ)
        0x20,
        0xFE, //             BRA *
    ];
    let mut write = |addr: u16, bytes: &[u8]| {
        for (i, &b) in bytes.iter().enumerate() {
            cpu.bus_mut().write(addr + i as u16, b).unwrap();
        }
    };
    write(START, &program);
    write(RESET_VECTOR, &START.to_be_bytes());
    write(IRQ_VECTOR, &IRQ_HANDLER.to_be_bytes());
    write(FIRQ_VECTOR, &FIRQ_HANDLER.to_be_bytes());
    cpu.reset().unwrap();
    for _ in 0..20 {
        assert!(matches!(cpu.step(None, true), StepResult::Executed(_)));
        if cpu.pc() == IRQ_HANDLER || cpu.pc() == FIRQ_HANDLER {
            return cpu.pc();
        }
    }
    panic!("no interrupt reached the CPU; PC=${:04X}", cpu.pc());
}

#[tokio::test]
async fn an_mc6850_routed_to_firq_interrupts_through_the_firq_vector() {
    assert_eq!(first_handler(Some("firq")).await, 0x2100);
}

#[tokio::test]
async fn an_mc6850_interrupts_through_the_irq_vector_by_default() {
    assert_eq!(first_handler(None).await, 0x2000);
    assert_eq!(first_handler(Some("irq")).await, 0x2000);
}

#[test]
fn a_cpu_variant_converts_to_its_config_string() {
    assert_eq!(String::from(CpuVariantSpec::Mc6809), "6809");
}

/// A device module that records the clock rate devices are built with.
#[derive(Clone)]
struct ClockProbe(std::sync::Arc<std::sync::Mutex<Option<u64>>>);

impl DeviceModule for ClockProbe {
    fn name(&self) -> &'static str {
        "probe"
    }

    async fn instantiate(
        &self,
        bus_config: emma6k_core::BusConfig,
        _address: u16,
        _attributes: &std::collections::HashMap<String, figment::value::Value>,
        context: &InstantiationContext,
        _id_allocator: std::sync::Arc<std::sync::Mutex<emma6k_core::bus::DeviceIdAllocator>>,
    ) -> Result<emma6k_core::BusConfig, DeviceModuleError> {
        *self.0.lock().unwrap() = context.clock_hz;
        Ok(bus_config)
    }
}

#[tokio::test]
async fn build_gives_devices_the_configured_clock_rate() {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let mut registry = device_registry();
    registry.register(ClockProbe(seen.clone()));
    config(
        r#"
        clock-speed-hz = 2000000
        [[devices]]
        type = "ram"
        address = 0x0000
        size = 65536
        [[devices]]
        type = "probe"
        address = 0x0000
        "#,
    )
    .build(&registry)
    .await
    .unwrap();
    assert_eq!(*seen.lock().unwrap(), Some(2_000_000));
}
