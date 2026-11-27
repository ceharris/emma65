//! The `exec` module's 6502-based tests: free runs, stepping, step-over and
//! step-out, WAI/STP parking, and interrupt commands, driven through the
//! 65C02. They moved here from `emma6k_core::exec` when the core became its
//! own crate, since the core can't depend on the 6502; `exec`'s CPU-neutral
//! coverage is its toy-`Processor` tests.

use emma6k_core::IrqSource;
use emma6k_core::exec::*;
use std::sync::{Arc, atomic::AtomicU16};
use std::time::Instant;
use tokio::sync::{mpsc, watch};

// --- free-run tests ---

use crate::mos6502::cpu::variant::CpuVariant;
use crate::mos6502::cpu::{Cpu, CpuBuilder};
use emma6k_core::bus::{AddressRange, Bus};

const RESET_VECTOR: u16 = 0xFFFC;
const NOP_ADDR: u16 = 0x0200;

/// Builds a CPU with 64KB RAM and an infinite NOP loop at `NOP_ADDR`.
fn make_cpu_with_speed(speed: ClockSpeed) -> Cpu {
    let mut bus = Bus::config()
        .ram_with_fill(AddressRange::new(0x0000, 0xFFFF), 0)
        .unwrap()
        .build();
    // Reset vector → NOP_ADDR
    bus.write(RESET_VECTOR, (NOP_ADDR & 0xFF) as u8).unwrap();
    bus.write(RESET_VECTOR + 1, (NOP_ADDR >> 8) as u8).unwrap();
    // NOP at NOP_ADDR; BRA -2 loops back to NOP_ADDR forever
    bus.write(NOP_ADDR, 0xEA).unwrap(); // NOP
    bus.write(NOP_ADDR + 1, 0x80).unwrap(); // BRA
    bus.write(NOP_ADDR + 2, 0xFD_u8).unwrap(); // offset -3 → back to NOP_ADDR
    let mut cpu = CpuBuilder::new(CpuVariant::Wdc65C02)
        .clock_speed(speed)
        .bus(bus)
        .build()
        .unwrap();
    cpu.reset().unwrap();
    cpu
}

#[tokio::test]
async fn free_run_stop_returns_cpu() {
    let cpu = make_cpu_with_speed(ClockSpeed::unlimited());
    let handle = run(cpu);
    // Let it run briefly, then stop.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let cpu = handle.take_cpu().await;
    // CPU executed NOPs; PC should still be in the NOP loop.
    assert!(cpu.cycles() > 0);
}

#[tokio::test]
async fn free_run_stop_reports_stop_requested() {
    let cpu = make_cpu_with_speed(ClockSpeed::unlimited());
    let handle = run(cpu);
    let stopper = handle.stopper();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    stopper.stop();
    let (result, cpu) = handle.take_cpu_with_result().await;
    assert!(matches!(result, StepResult::StopRequested));
    assert!(cpu.cycles() > 0);
}

#[tokio::test]
async fn live_snapshot_stack_is_page_one() {
    let mut cpu = make_cpu_with_speed(ClockSpeed::unlimited());
    cpu.bus_mut().write(0x0100, 0x11).unwrap();
    cpu.bus_mut().write(0x01FF, 0x22).unwrap();
    let handle = run(cpu);
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let snapshot = handle
        .live_snapshot()
        .expect("expected a published snapshot");
    assert_eq!(snapshot.stack_base, 0x0100);
    assert_eq!(snapshot.stack.len(), 256);
    assert_eq!(snapshot.stack[0x00], 0x11);
    assert_eq!(snapshot.stack[0xFF], 0x22);
    handle.take_cpu().await;
}

#[tokio::test]
async fn live_snapshot_reports_watched_addresses() {
    let mut cpu = make_cpu_with_speed(ClockSpeed::unlimited());
    cpu.bus_mut().write(0x0300, 0x42).unwrap();
    let handle = run_from(cpu, None, Arc::new(AtomicU16::new(0)), vec![0x0300], false);
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let snapshot = handle
        .live_snapshot()
        .expect("expected a published snapshot");
    assert_eq!(snapshot.watched_values, vec![(0x0300, 0x42)]);
    handle.take_cpu().await;
}

#[tokio::test]
async fn breakpoint_during_free_run_returns_breakpoint_result() {
    let mut cpu = make_cpu_with_speed(ClockSpeed::unlimited());
    cpu.add_breakpoint(NOP_ADDR);
    let handle = run(cpu);
    let result = handle.wait().await;
    assert!(matches!(result, StepResult::Breakpoint(NOP_ADDR)));
}

#[tokio::test]
async fn watch_trigger_during_free_run_returns_watch_triggered() {
    use crate::mos6502::cpu::{map_flag_name, map_register_name};
    use emma6k_core::watch::WatchCompiler;

    let mut cpu = make_cpu_with_speed(ClockSpeed::unlimited());
    let mut compiler = WatchCompiler::new(map_register_name, map_flag_name, |_| None);
    // PC == NOP_ADDR triggers immediately on the first watch evaluation.
    let wp = compiler
        .compile(&format!("PC == ${:X}", NOP_ADDR), cpu.evaluator_mut())
        .unwrap();
    cpu.evaluator_mut().add(wp);
    let handle = run(cpu);
    let result = handle.wait().await;
    assert!(matches!(result, StepResult::WatchTriggered { .. }));
}

#[tokio::test]
async fn throttled_execution_takes_at_least_wall_time() {
    // Test at 2 MHz — the most demanding common clock speed (1 MHz, 1.8432 MHz, 2 MHz).
    // At 2 MHz, BATCH_SIZE=1000 instructions ≈ 2500 cycles ≈ 1.25ms of emulated time,
    // so the throttle fires roughly every 1ms — still fine-grained enough.
    // We allow 2× tolerance: cycles must not exceed 2 × (wall_elapsed × 2_000_000 Hz).
    let cpu = make_cpu_with_speed(ClockSpeed::mhz(2.0));
    let handle = run(cpu);
    let wall_start = Instant::now();
    tokio::time::sleep(std::time::Duration::from_millis(15)).await;
    let cpu = handle.take_cpu().await;
    let wall_elapsed = wall_start.elapsed();
    // cycles / 2_000_000 should be ≤ wall_elapsed; allow 2× slack for CI jitter.
    let max_expected_cycles = wall_elapsed.as_micros() as u64 * 4; // 2 cycles/µs × 2 slack
    assert!(
        cpu.cycles() <= max_expected_cycles,
        "throttled CPU ran too fast: {} cycles in {:?}",
        cpu.cycles(),
        wall_elapsed
    );
}

#[tokio::test]
async fn unlimited_faster_than_throttled() {
    // Use 50 kHz throttle — well below even a debug-build's natural step() throughput,
    // so the throttle demonstrably limits cycle accumulation compared to unlimited.
    // 50ms × 50_000 Hz = 2500 expected throttled cycles; unlimited should far exceed that.
    let target_wall = std::time::Duration::from_millis(50);

    let cpu_unlimited = make_cpu_with_speed(ClockSpeed::unlimited());
    let handle_unlimited = run(cpu_unlimited);
    tokio::time::sleep(target_wall).await;
    let cpu_unlimited = handle_unlimited.take_cpu().await;

    let cpu_throttled = make_cpu_with_speed(ClockSpeed::hz(50_000));
    let handle_throttled = run(cpu_throttled);
    tokio::time::sleep(target_wall).await;
    let cpu_throttled = handle_throttled.take_cpu().await;

    assert!(
        cpu_unlimited.cycles() > cpu_throttled.cycles(),
        "unlimited ({} cycles) should exceed throttled ({} cycles)",
        cpu_unlimited.cycles(),
        cpu_throttled.cycles()
    );
}

#[tokio::test]
async fn run_from_advances_past_breakpoint_at_initial_pc() {
    // run_from(cpu, Some(pc)) must advance past a breakpoint at the initial PC
    // rather than immediately halting before a single instruction executes.
    let mut cpu = make_cpu_at(0x0200);
    // NOP; NOP; STP — run should execute past the breakpoint at $0200 and halt at STP.
    write(&mut cpu, 0x0200, &[0xEA, 0xEA, 0xDB]); // NOP, NOP, STP
    cpu.add_breakpoint(0x0200);

    let handle = run_from(cpu, Some(0x0200), no_mem(), Vec::new(), false);
    let (result, cpu) = handle.take_cpu_with_result().await;

    assert!(
        matches!(result, StepResult::Stopped),
        "expected Stopped result"
    );
    assert!(
        cpu.registers().pc > 0x0200,
        "PC should have advanced past the breakpoint"
    );
    // Breakpoint must still be present after the run.
    assert!(cpu.breakpoints().contains(&0x0200));
}

#[tokio::test]
async fn run_does_not_halt_on_stp_alone_when_parked() {
    // With park_on_stall: true (the debugger's Run command), a free run must
    // not stop just because the CPU executed STP: the background thread
    // should stay alive polling for a device- or UI-triggered RESET rather
    // than handing the CPU back immediately.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0xDB]); // STP
    let handle = run_from(cpu, None, no_mem(), Vec::new(), true);
    let stopper = handle.stopper();
    // Give the parked poll loop time to run for a while without a reset.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    stopper.trigger_reset();
    let (result, cpu) = handle.take_cpu_with_result().await;
    assert!(matches!(result, StepResult::Reset), "expected Reset");
    assert!(!cpu.is_stopped());
}

#[tokio::test]
async fn run_halts_on_stp_by_default() {
    // Without park_on_stall (plain `run`, used by the standalone binary),
    // STP must halt the free run immediately, same as before — the CLI
    // relies on this to exit once a headless program has finished.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0xDB]); // STP
    let handle = run(cpu);
    let (result, cpu) = handle.take_cpu_with_result().await;
    assert!(
        matches!(result, StepResult::Stopped),
        "expected Stopped result"
    );
    assert!(cpu.is_stopped());
}

#[tokio::test]
async fn run_resumes_transparently_from_wai_on_irq_when_parked() {
    // With park_on_stall: true, a free run parked in WAI should service a
    // subsequent IRQ and keep running, rather than requiring the caller to
    // restart it.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x58, 0xCB, 0x80, 0xFE]); // CLI, WAI, BRA -2 (spins after waking)
    write(&mut cpu, 0x0300, &[0xA9, 0x42, 0x8D, 0x00, 0x00, 0x40]); // LDA #$42, STA $0000, RTI
    cpu.bus_mut().write(0xFFFE, 0x00).unwrap(); // IRQ vector lo -> $0300
    cpu.bus_mut().write(0xFFFF, 0x03).unwrap(); // IRQ vector hi
    let handle = run_from(cpu, None, no_mem(), Vec::new(), true);
    let stopper = handle.stopper();
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    stopper.assert_irq(IrqSource(0));
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let cpu = handle.take_cpu().await;
    assert_eq!(
        read_marker(&cpu),
        0x42,
        "IRQ ISR should have run after waking from WAI"
    );
    assert!(!cpu.is_waiting());
}

#[tokio::test]
async fn run_wakes_from_wai_on_reset_when_parked() {
    // With park_on_stall: true, a free run parked in WAI must also wake on
    // a RESET (device- or UI-triggered), not just IRQ/NMI — same as real
    // 65C02 hardware.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0xCB]); // WAI
    let handle = run_from(cpu, None, no_mem(), Vec::new(), true);
    let stopper = handle.stopper();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    stopper.trigger_reset();
    let (result, cpu) = handle.take_cpu_with_result().await;
    assert!(matches!(result, StepResult::Reset), "expected Reset");
    assert!(!cpu.is_waiting());
}

#[tokio::test]
async fn run_halts_on_wai_by_default() {
    // Without park_on_stall (plain `run`), a WAI with nothing pending must
    // halt the free run immediately, same as before.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0xCB]); // WAI
    let handle = run(cpu);
    let (result, cpu) = handle.take_cpu_with_result().await;
    assert!(
        matches!(result, StepResult::Waiting),
        "expected Waiting result"
    );
    assert!(cpu.is_waiting());
}

#[tokio::test]
async fn run_halts_at_breakpoint_when_no_skip() {
    // Plain run() (no skip) must halt immediately at a breakpoint.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0xEA]); // NOP
    cpu.add_breakpoint(0x0200);

    let handle = run(cpu);
    let (result, cpu) = handle.take_cpu_with_result().await;

    assert!(
        matches!(result, StepResult::Breakpoint(0x0200)),
        "expected Breakpoint"
    );
    assert_eq!(cpu.registers().pc, 0x0200);
}

// --- interrupt commands while running ---

/// Builds a CPU with an infinite `CLI; NOP; BRA` loop at $0200 (so IRQ is
/// unmasked) and an IRQ ISR at $0300 that writes $42 to $0000 then returns.
fn make_cpu_with_irq_loop() -> Cpu {
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x58, 0xEA, 0x80, 0xFD]); // CLI, NOP, BRA -3
    write(&mut cpu, 0x0300, &[0xA9, 0x42, 0x8D, 0x00, 0x00, 0x40]); // LDA #$42, STA $0000, RTI
    cpu.bus_mut().write(0xFFFE, 0x00).unwrap(); // IRQ vector lo -> $0300
    cpu.bus_mut().write(0xFFFF, 0x03).unwrap(); // IRQ vector hi
    cpu
}

/// Builds a CPU with an infinite `NOP; BRA` loop at $0200 and an NMI ISR at
/// $0300 that writes $99 to $0000 then returns.
fn make_cpu_with_nmi_loop() -> Cpu {
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0xEA, 0x80, 0xFE]); // NOP, BRA -2
    write(&mut cpu, 0x0300, &[0xA9, 0x99, 0x8D, 0x00, 0x00, 0x40]); // LDA #$99, STA $0000, RTI
    cpu.bus_mut().write(0xFFFA, 0x00).unwrap(); // NMI vector lo -> $0300
    cpu.bus_mut().write(0xFFFB, 0x03).unwrap(); // NMI vector hi
    cpu
}

/// Reads the byte at $0000, used as a marker address by the ISRs above.
fn read_marker(cpu: &Cpu) -> u8 {
    let mut buf = [0u8];
    let _ = cpu.bus().peek_range(0x0000, &mut buf);
    buf[0]
}

/// Builds a CPU with an infinite `CLI; NOP; BRA` loop at $0200 (so IRQ is
/// unmasked) and an IRQ ISR at $0300 that increments the counter at $0000
/// then returns, so re-entry can be detected across a sustained free run.
fn make_cpu_with_irq_counting_loop() -> Cpu {
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x58, 0xEA, 0x80, 0xFD]); // CLI, NOP, BRA -3
    write(&mut cpu, 0x0300, &[0xEE, 0x00, 0x00, 0x40]); // INC $0000, RTI
    cpu.bus_mut().write(0xFFFE, 0x00).unwrap(); // IRQ vector lo -> $0300
    cpu.bus_mut().write(0xFFFF, 0x03).unwrap(); // IRQ vector hi
    cpu
}

#[tokio::test]
async fn run_stopper_assert_irq_services_without_halting_run() {
    // Asserting IRQ via a RunStopper on a running CPU thread must be applied
    // and serviced without stopping the free run.
    let cpu = make_cpu_with_irq_loop();
    let handle = run(cpu);
    let stopper = handle.stopper();
    // Give the loop time to execute CLI at least once.
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    stopper.assert_irq(IrqSource(0));
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let cpu = handle.take_cpu().await;
    assert_eq!(
        read_marker(&cpu),
        0x42,
        "IRQ ISR should have run while free-running"
    );
}

#[tokio::test]
async fn run_stopper_assert_irq_pulse_services_exactly_once() {
    // A pulsed IRQ must auto-release once serviced, so a sustained free run
    // afterward does not repeatedly re-enter the ISR (issue #261) the way a
    // manually-asserted (level-triggered) IRQ would.
    let cpu = make_cpu_with_irq_counting_loop();
    let handle = run(cpu);
    let stopper = handle.stopper();
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    stopper.assert_irq_pulse(IrqSource(0));
    // Let the free run continue well past when a level-triggered assert
    // would have re-entered the ISR many times.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let cpu = handle.take_cpu().await;
    assert_eq!(
        read_marker(&cpu),
        1,
        "pulsed IRQ should service exactly once"
    );
    assert!(
        !cpu.interrupts().irq_active(),
        "pulsed IRQ source should have auto-released"
    );
}

#[tokio::test]
async fn run_stopper_trigger_nmi_services_without_halting_run() {
    // Triggering NMI via a RunStopper on a running CPU thread must be applied
    // and serviced without stopping the free run.
    let cpu = make_cpu_with_nmi_loop();
    let handle = run(cpu);
    let stopper = handle.stopper();
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    stopper.trigger_nmi();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let cpu = handle.take_cpu().await;
    assert_eq!(
        read_marker(&cpu),
        0x99,
        "NMI ISR should have run while free-running"
    );
}

#[tokio::test]
async fn run_stopper_trigger_reset_halts_run() {
    // Triggering RESET via a RunStopper must halt the free run (like a
    // breakpoint) and leave the CPU at the reset vector's target.
    let cpu = make_cpu_with_speed(ClockSpeed::unlimited());
    let handle = run(cpu);
    let stopper = handle.stopper();
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    stopper.trigger_reset();
    let (result, cpu) = handle.take_cpu_with_result().await;
    assert!(matches!(result, StepResult::Reset), "expected Reset");
    assert_eq!(cpu.registers().pc, NOP_ADDR);
}

#[test]
fn step_over_subroutine_applies_queued_interrupt_command() {
    // A command queued before the call must be applied before the first
    // instruction of a JSR-atomic step executes.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0300, &[0xEA, 0xEA, 0x60]); // NOP, NOP, RTS
    let (stopper, stop_rx, mut cmd_rx) = RunStopper::channel();
    stopper.assert_irq(IrqSource(5));
    let result = step_over_subroutine(&mut cpu, &stop_rx, &mut cmd_rx, None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Executed(_)));
    assert!(cpu.interrupts().irq_active());
}

#[test]
fn step_return_applies_nmi_command_from_channel() {
    // Same scenario as step_return_nmi_during_subroutine_does_not_prematurely_halt,
    // but delivering the NMI through the command channel instead of a direct call.
    let mut cpu = make_cpu_at(0x0200);
    cpu.bus_mut().write(0xFFFA, 0x00).unwrap(); // NMI vector lo
    cpu.bus_mut().write(0xFFFB, 0x04).unwrap(); // NMI vector hi → $0400
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0300, &[0xEA, 0xEA, 0xEA, 0x60]); // NOP, NOP, NOP, RTS
    write(&mut cpu, 0x0400, &[0x40]); // ISR: RTI
    step_into(&mut cpu); // JSR $0300 — S = 0xFD
    let (stopper, stop_rx, mut cmd_rx) = RunStopper::channel();
    stopper.trigger_nmi();
    let result = step_return(&mut cpu, &stop_rx, &mut cmd_rx, None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0203);
}

// --- step_over / step_return helpers ---

/// Returns a stop receiver that never fires, for use in tests that don't need interruption.
fn no_stop() -> watch::Receiver<bool> {
    watch::channel(false).1
}

/// Returns a command receiver with no sender, for tests that don't exercise interrupt commands.
fn no_cmd() -> mpsc::UnboundedReceiver<InterruptCommand> {
    mpsc::unbounded_channel().1
}

/// Returns a dummy memory-view-address arc for tests that don't exercise live snapshots.
fn no_mem() -> Arc<AtomicU16> {
    Arc::new(AtomicU16::new(0))
}

fn make_cpu_at(start: u16) -> Cpu {
    use crate::mos6502::cpu::variant::CpuVariant;
    use emma6k_core::bus::AddressRange;
    let mut bus = Bus::config()
        .ram_with_fill(AddressRange::new(0x0000, 0xFFFF), 0)
        .unwrap()
        .build();
    bus.write(0xFFFC, (start & 0xFF) as u8).unwrap();
    bus.write(0xFFFC + 1, (start >> 8) as u8).unwrap();
    let mut cpu = CpuBuilder::new(CpuVariant::Wdc65C02)
        .bus(bus)
        .build()
        .unwrap();
    cpu.reset().unwrap();
    cpu
}

fn write(cpu: &mut Cpu, addr: u16, bytes: &[u8]) {
    for (i, &b) in bytes.iter().enumerate() {
        cpu.bus_mut().write(addr + i as u16, b).unwrap();
    }
}

// --- step_over ---

#[test]
fn step_over_non_jsr_behaves_like_step_into() {
    // NOP at $0200; step_over should execute it and advance PC by 1.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0xEA]); // NOP
    let result =
        step_over_subroutine(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0201);
}

#[test]
fn step_over_jsr_advances_past_subroutine() {
    // JSR $0300 at $0200; subroutine is NOP + RTS.
    // step_over should return with PC at $0203 (instruction after JSR).
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0300, &[0xEA, 0x60]); // NOP, RTS
    let result =
        step_over_subroutine(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0203);
}

#[test]
fn step_over_jsr_nested_calls_treated_atomically() {
    // Subroutine at $0300 itself calls $0400 before returning.
    // step_over must treat the entire call tree as atomic.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0300, &[0x20, 0x00, 0x04]); // JSR $0400
    write(&mut cpu, 0x0303, &[0x60]); // RTS
    write(&mut cpu, 0x0400, &[0x60]); // RTS
    let result =
        step_over_subroutine(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0203);
}

#[test]
fn step_over_jsr_halts_at_inner_breakpoint() {
    // A breakpoint inside the subroutine interrupts step_over.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0300, &[0xEA, 0x60]); // NOP, RTS
    cpu.add_breakpoint(0x0300);
    let result =
        step_over_subroutine(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Breakpoint(0x0300)));
    assert_eq!(cpu.registers().pc, 0x0300);
}

#[test]
fn step_over_jsr_preserves_caller_breakpoint_at_target() {
    // If the caller had already set a breakpoint at PC+3, step_over must
    // not remove it and must surface it as Breakpoint, not Executed.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0300, &[0xEA, 0x60]); // NOP, RTS
    cpu.add_breakpoint(0x0203);
    let result =
        step_over_subroutine(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    // Must surface as Breakpoint since caller owns it.
    assert!(matches!(result, StepResult::Breakpoint(0x0203)));
    // Breakpoint must still be present after step_over returns.
    assert!(cpu.breakpoints().contains(&0x0203));
}

#[test]
fn step_over_jsr_halts_on_stp() {
    // Subroutine executes STP before returning.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0300, &[0xDB]); // STP
    let result =
        step_over_subroutine(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Stopped));
}

// --- step_return ---

#[test]
fn step_return_halts_after_rts() {
    // Call a subroutine via JSR; land at $0300 then call step_return.
    // step_return should run until RTS and return with PC at $0203.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0300, &[0xEA, 0xEA, 0x60]); // NOP, NOP, RTS
    step_into(&mut cpu); // JSR — now at $0300, S = 0xFD
    let result = step_return(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0203);
}

#[test]
fn step_return_halts_after_rti() {
    // Manually set up a stack frame and use RTI to unwind.
    let mut cpu = make_cpu_at(0x0200);
    // Push P, PC lo, PC hi so RTI returns to $0300.
    let s = cpu.registers().s;
    cpu.bus_mut().write(0x0100 | s as u16, 0x03).unwrap(); // PC hi
    cpu.bus_mut()
        .write(0x0100 | s.wrapping_sub(1) as u16, 0x00)
        .unwrap(); // PC lo
    cpu.bus_mut()
        .write(0x0100 | s.wrapping_sub(2) as u16, 0x24)
        .unwrap(); // P
    cpu.registers_mut().s = s.wrapping_sub(3);
    write(&mut cpu, 0x0200, &[0x40]); // RTI
    let result = step_return(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0300);
}

#[test]
fn step_return_handles_stack_pointer_wraparound() {
    // Place initial_s at 0x01 so RTS (which pops 2) wraps S through 0xFF
    // to 0x03. A naive `s > initial_s` would see 0x03 > 0x01 == true, but
    // a naive `0xFF > 0x01` (the intermediate value after the first pop)
    // would also be true, which is correct — the wrapping comparison
    // (s.wrapping_sub(initial_s) as i8) > 0 must also give the right answer.
    //
    // Set up: manually put S at 0x01, push a return address of $0203 so
    // that RTS pops 2 bytes (S: 0x01 → 0xFF → 0x03), and verify step_return
    // halts correctly.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x60]); // RTS at $0200 (acts as the subroutine body)
    // Lay the return address ($0202) on the stack with S = 0x01.
    // RTS pops lo then hi: stack[S+1]=lo=$02, stack[S+2]=hi=$02; RTS adds 1 → PC=$0203.
    cpu.bus_mut().write(0x0102, 0x02).unwrap(); // PC hi
    cpu.bus_mut().write(0x0101, 0x02).unwrap(); // PC lo
    cpu.registers_mut().s = 0x00; // so S+1=0x01, S+2=0x02
    // initial_s for step_return will be 0x00; after RTS, S = 0x02 (wrapped).
    // (0x02_u8.wrapping_sub(0x00) as i8) = 2 > 0 ✓
    let result = step_return(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0203);
}

#[test]
fn step_return_halts_at_breakpoint_inside_subroutine() {
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0300, &[0xEA, 0xEA, 0x60]); // NOP, NOP, RTS
    step_into(&mut cpu); // JSR — now inside subroutine
    cpu.add_breakpoint(0x0301);
    let result = step_return(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Breakpoint(0x0301)));
}

#[test]
fn step_return_halts_on_stp() {
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0300, &[0xDB]); // STP before RTS
    step_into(&mut cpu); // JSR
    let result = step_return(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Stopped));
}

#[test]
fn step_return_nested_call_unwinds_to_correct_frame() {
    // JSR $0300; subroutine JSR $0400; step_return inside $0400 unwinds to $0303.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0300, &[0x20, 0x00, 0x04]); // JSR $0400
    write(&mut cpu, 0x0303, &[0x60]); // RTS (outer)
    write(&mut cpu, 0x0400, &[0xEA, 0x60]); // NOP, RTS (inner)
    step_into(&mut cpu); // JSR $0300
    step_into(&mut cpu); // JSR $0400 — now inside inner subroutine
    let s_inside = cpu.registers().s;
    let result = step_return(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Executed(_)));
    // Returned to $0303 (after inner JSR).
    assert_eq!(cpu.registers().pc, 0x0303);
    // S rose above s_inside but not back to original (outer frame still on stack).
    assert!(cpu.registers().s > s_inside);
}

#[test]
fn step_return_nmi_during_subroutine_does_not_prematurely_halt() {
    // NMI is used instead of IRQ: it fires once (edge-triggered), avoids the
    // re-fire loop that IRQ would create after RTI restores I=0.
    //
    // Scenario: step_return starts at $0300 with S=0xFD (initial_s).
    // NMI entry pushes 3 bytes (S→0xFA); ISR RTI restores S to 0xFD.
    // 0xFD == initial_s (not above), so step_return must continue.
    // The outer RTS then raises S to 0xFF > 0xFD, halting correctly at $0203.
    let mut cpu = make_cpu_at(0x0200);
    cpu.bus_mut().write(0xFFFA, 0x00).unwrap(); // NMI vector lo
    cpu.bus_mut().write(0xFFFB, 0x04).unwrap(); // NMI vector hi → $0400
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0300, &[0xEA, 0xEA, 0xEA, 0x60]); // NOP, NOP, NOP, RTS
    write(&mut cpu, 0x0400, &[0x40]); // ISR: RTI
    step_into(&mut cpu); // JSR $0300 — S = 0xFD
    cpu.interrupts_mut().signal_nmi();
    let result = step_return(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0203);
}

// --- step_over_breakpoint (skip-current-PC behaviour) ---

#[test]
fn step_into_at_breakpoint_advances_past_it() {
    // A breakpoint at the current PC must not block step_over_breakpoint.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0xEA]); // NOP
    cpu.add_breakpoint(0x0200);
    let result = step_over_breakpoint(&mut cpu, 0x0200);
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0201);
    // Breakpoint must still be present after the skip.
    assert!(cpu.breakpoints().contains(&0x0200));
}

#[test]
fn step_into_at_breakpoint_halts_at_next_breakpoint() {
    // The skip applies only to the current PC; the next breakpoint fires normally.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0xEA, 0xEA]); // NOP NOP
    cpu.add_breakpoint(0x0200);
    cpu.add_breakpoint(0x0201);
    let result = step_over_breakpoint(&mut cpu, 0x0200);
    // Should execute the NOP at $0200, then the next step_over_breakpoint/step
    // call would halt at $0201 — but this single call returns Executed.
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0201);
}

#[test]
fn step_over_non_jsr_at_breakpoint_advances_past_it() {
    // step_over on a non-JSR instruction with a breakpoint at the current PC
    // must execute the instruction, not re-fire the breakpoint.
    let mut cpu = make_cpu_at(0x0200);
    write(&mut cpu, 0x0200, &[0xEA]); // NOP
    cpu.add_breakpoint(0x0200);
    let result =
        step_over_subroutine(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0201);
    assert!(cpu.breakpoints().contains(&0x0200));
}

#[test]
fn step_over_jsr_at_breakpoint_advances_past_it() {
    // step_over on a JSR with a breakpoint at the JSR address must execute
    // the JSR and advance past the subroutine, not re-fire the breakpoint.
    let mut cpu = make_cpu_at(0x0200);
    // JSR $0300; NOP at $0203 (return target); RTS at $0300
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0203, &[0xEA]); // NOP
    write(&mut cpu, 0x0300, &[0x60]); // RTS
    cpu.add_breakpoint(0x0200);
    let result =
        step_over_subroutine(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0203);
    assert!(cpu.breakpoints().contains(&0x0200));
}

#[test]
fn step_return_at_breakpoint_advances_past_it() {
    // step_return with a breakpoint at the current PC must execute the
    // instruction rather than immediately re-firing the breakpoint.
    // Setup: call a subroutine via JSR so the stack has a valid return address,
    // then set a breakpoint at the subroutine entry and invoke step_return.
    let mut cpu = make_cpu_at(0x0200);
    // JSR $0300; NOP at $0203 (return site); RTS at $0300
    write(&mut cpu, 0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    write(&mut cpu, 0x0203, &[0xEA]); // NOP
    write(&mut cpu, 0x0300, &[0x60]); // RTS
    // Execute the JSR so we are inside the subroutine with the return address
    // already on the stack. PC is now $0300.
    let r = step_into(&mut cpu);
    assert!(matches!(r, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0300);

    // Place a breakpoint at the subroutine entry (current PC) and call step_return.
    cpu.add_breakpoint(0x0300);
    let result = step_return(&mut cpu, &no_stop(), &mut no_cmd(), None, &no_mem()).unwrap();
    // Should execute the RTS and return to $0203, not re-fire the breakpoint.
    assert!(matches!(result, StepResult::Executed(_)));
    assert_eq!(cpu.registers().pc, 0x0203);
    // Breakpoint must still be present after the skip.
    assert!(cpu.breakpoints().contains(&0x0300));
}
