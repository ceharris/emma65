//! `emma6k_core::exec` driving the 6809: free running, breakpoints, step-over
//! of each subroutine call, and step-out.

use crate::cpu::{Cpu, RESET_VECTOR, StepResult};
use emma6k_core::exec::{RunStopper, run, step_into, step_over_subroutine, step_return};
use emma6k_core::{AddressRange, Bus, Processor};
use std::sync::Arc;
use std::sync::atomic::AtomicU16;
use std::time::Duration;

const START: u16 = 0x1000;
/// Close enough for BSR's 8-bit offset.
const SUB: u16 = 0x1040;
const STACK: u16 = 0x8000;

/// A CPU with `program` at `START`, `subroutine` at `SUB`, and S at `STACK`.
fn cpu(program: &[u8], subroutine: &[u8]) -> Cpu {
    let bus = Bus::config()
        .ram_with_fill(AddressRange::new(0x0000, 0xFFFF), 0)
        .unwrap()
        .build();
    let mut cpu = Cpu::builder().bus(bus).build().unwrap();
    let mut write = |addr: u16, bytes: &[u8]| {
        for (i, &b) in bytes.iter().enumerate() {
            cpu.bus_mut().write(addr + i as u16, b).unwrap();
        }
    };
    write(RESET_VECTOR, &START.to_be_bytes());
    write(START, program);
    write(SUB, subroutine);
    cpu.reset().unwrap();
    cpu.registers_mut().s = STACK;
    cpu
}

/// `NOP; BRA *-3`: an endless loop.
fn nop_loop() -> Cpu {
    cpu(&[0x12, 0x20, 0xFD], &[])
}

/// The subroutine every call below targets: `NOP; NOP; RTS`.
const SUBROUTINE: [u8; 3] = [0x12, 0x12, 0x39];

fn step_over(cpu: &mut Cpu) -> Option<StepResult> {
    let (_stopper, stop_rx, mut cmd_rx) = RunStopper::channel();
    step_over_subroutine(
        cpu,
        &stop_rx,
        &mut cmd_rx,
        None,
        &Arc::new(AtomicU16::new(0)),
    )
}

#[tokio::test]
async fn free_run_halts_at_a_breakpoint() {
    let mut cpu = nop_loop();
    cpu.add_breakpoint(START + 1);
    let result = run(cpu).wait().await;
    assert!(matches!(result, StepResult::Breakpoint(addr) if addr == START + 1));
}

#[tokio::test]
async fn a_stopped_run_reports_stop_requested() {
    let handle = run(nop_loop());
    let stopper = handle.stopper();
    tokio::time::sleep(Duration::from_millis(5)).await;
    stopper.stop();
    let (result, cpu) = handle.take_cpu_with_result().await;
    assert!(matches!(result, StepResult::StopRequested));
    assert!(cpu.cycles() > 0);
}

#[test]
fn step_over_runs_each_kind_of_call_as_one_step() {
    let offset = |len: u16| SUB.wrapping_sub(START + len);
    let bsr = [0x8D, offset(2) as u8];
    let [lbsr_hi, lbsr_lo] = offset(3).to_be_bytes();
    let [sub_hi, sub_lo] = SUB.to_be_bytes();
    let calls: [(&str, Vec<u8>); 5] = [
        ("BSR", bsr.to_vec()),
        ("LBSR", vec![0x17, lbsr_hi, lbsr_lo]),
        ("JSR >", vec![0xBD, sub_hi, sub_lo]),
        // JSR <$40 with DP = $10.
        ("JSR <", vec![0x9D, sub_lo]),
        // JSR [n16]: through a pointer at $2000.
        ("JSR [n16]", vec![0xAD, 0x9F, 0x20, 0x00]),
    ];
    for (name, program) in calls {
        let len = program.len() as u16;
        let mut cpu = cpu(&program, &SUBROUTINE);
        cpu.registers_mut().dp = sub_hi;
        cpu.bus_mut().write(0x2000, sub_hi).unwrap();
        cpu.bus_mut().write(0x2001, sub_lo).unwrap();
        let result = step_over(&mut cpu);
        assert!(
            matches!(result, Some(StepResult::Executed(op)) if op.opcode == 0x39),
            "{name}: the last instruction run should be RTS"
        );
        assert_eq!(cpu.pc(), START + len, "{name}");
        assert_eq!(cpu.registers().s, STACK, "{name}");
        assert!(cpu.breakpoints().is_empty(), "{name}");
    }
}

#[test]
fn step_over_a_non_call_steps_once() {
    let mut cpu = nop_loop();
    let result = step_over(&mut cpu);
    assert!(matches!(result, Some(StepResult::Executed(op)) if op.opcode == 0x12));
    assert_eq!(cpu.pc(), START + 1);
}

#[test]
fn step_return_halts_after_rts() {
    let [sub_hi, sub_lo] = SUB.to_be_bytes();
    let mut cpu = cpu(&[0xBD, sub_hi, sub_lo, 0x12], &SUBROUTINE);
    assert!(matches!(step_into(&mut cpu), StepResult::Executed(_)));
    assert_eq!(cpu.pc(), SUB);
    let (_stopper, stop_rx, mut cmd_rx) = RunStopper::channel();
    let result = step_return(
        &mut cpu,
        &stop_rx,
        &mut cmd_rx,
        None,
        &Arc::new(AtomicU16::new(0)),
    );
    assert!(matches!(result, Some(StepResult::Executed(op)) if op.opcode == 0x39));
    assert_eq!(cpu.pc(), START + 3);
    assert_eq!(cpu.registers().s, STACK);
}

#[tokio::test]
async fn live_snapshot_carries_the_6809_stack_window() {
    let handle = run(nop_loop());
    tokio::time::sleep(Duration::from_millis(5)).await;
    let snapshot = handle.live_snapshot().expect("a published snapshot");
    assert_eq!(snapshot.stack_base, STACK - 32);
    assert_eq!(snapshot.stack.len(), 128);
    assert_eq!(snapshot.registers.s, STACK);
    handle.take_cpu().await;
}
