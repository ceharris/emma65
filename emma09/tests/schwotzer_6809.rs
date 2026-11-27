//! Runs W. Schwotzer's MC6809 CPU test suite (`tests/roms/cputest.asm`; see
//! `tests/roms/README.md`) on the emulated 6809.
//!
//! The suite checks every addressing mode, including all the indexed and
//! indirect forms, and the arithmetic, logic, shift, load/store, LEA, TFR and
//! EXG instructions with their condition codes. It reports through FLEX
//! monitor entry points, which `tests/roms/flex_shim.asm` implements on one
//! output register, and ends by jumping to FLEX's warm start. It prints
//! `Failed Test: <mnemonic>` for each failure and `All Tests succeded` (sic)
//! if there were none.

use emma6k_core::{AddressRange, Bus, ClockSpeed, DeviceId, IoDevice, Processor};
use emma09::cpu::StepResult;
use emma09::{CpuBuilder, InvalidOpcodePolicy};
use std::sync::{Arc, Mutex};

/// Where the suite is assembled (`ORG $8100`).
const SUITE_START: u16 = 0x8100;
/// Where the FLEX shim is assembled.
const SHIM_START: u16 = 0xCD00;
/// FLEX's warm start, where the suite jumps when it's done.
const WARMS: u16 = 0xCD03;
/// The shim's output register (`OUTPUT` in `flex_shim.asm`).
const OUTPUT: u16 = 0xC000;
/// The suite never sets S, so the harness does, clear of its scratch area
/// (`$7F80–$807F`) and its code.
const STACK_TOP: u16 = 0x7000;
/// Far more steps than the suite needs (about 40,000).
const MAX_STEPS: u64 = 10_000_000;

/// A write-only register that collects every byte written to it.
struct Capture(Arc<Mutex<Vec<u8>>>);

impl IoDevice for Capture {
    fn read(&mut self, _address: u16) -> u8 {
        0
    }

    fn write(&mut self, _address: u16, value: u8) {
        self.0.lock().unwrap().push(value);
    }

    fn peek(&self, _address: u16) -> u8 {
        0
    }

    fn identity_address(&self) -> u16 {
        OUTPUT
    }
}

#[test]
fn schwotzer_cpu_test_suite_passes() {
    let suite = include_bytes!("roms/cputest.bin");
    let shim = include_bytes!("roms/flex_shim.bin");
    let mut image = vec![0u8; 0x1_0000];
    let at = |addr: u16| addr as usize;
    image[at(SUITE_START)..at(SUITE_START) + suite.len()].copy_from_slice(suite);
    image[at(SHIM_START)..at(SHIM_START) + shim.len()].copy_from_slice(shim);

    let output = Arc::new(Mutex::new(Vec::new()));
    let bus = Bus::config()
        .ram_with_data(
            AddressRange::new(0x0000, OUTPUT - 1),
            image[..at(OUTPUT)].to_vec(),
        )
        .unwrap()
        .device(
            AddressRange::new(OUTPUT, OUTPUT),
            DeviceId(1),
            Box::new(Capture(output.clone())),
        )
        .unwrap()
        .ram_with_data(
            AddressRange::new(OUTPUT + 1, 0xFFFF),
            image[at(OUTPUT) + 1..].to_vec(),
        )
        .unwrap()
        .build();
    let mut cpu = CpuBuilder::new()
        .clock_speed(ClockSpeed::unlimited())
        .invalid_opcode_policy(InvalidOpcodePolicy::Error)
        .bus(bus)
        .build()
        .unwrap();
    cpu.registers_mut().pc = SUITE_START;
    cpu.registers_mut().s = STACK_TOP;

    let text = || String::from_utf8_lossy(&output.lock().unwrap()).into_owned();
    let mut steps = 0;
    while cpu.pc() != WARMS {
        assert!(
            steps < MAX_STEPS,
            "the suite didn't finish in {MAX_STEPS} steps; PC=${:04X}; output: {}",
            cpu.pc(),
            text()
        );
        match cpu.step(None, false) {
            StepResult::Executed(_) => {}
            StepResult::Error(e) => panic!("{e} (PC=${:04X}); output so far: {}", cpu.pc(), text()),
            _ => panic!(
                "the CPU stopped executing at PC=${:04X}; output so far: {}",
                cpu.pc(),
                text()
            ),
        }
        steps += 1;
    }

    let text = text();
    assert!(!text.contains("Failed Test:"), "the suite failed:{text}");
    assert!(
        text.contains("All Tests succeded"),
        "no success message: {text:?}"
    );
}
