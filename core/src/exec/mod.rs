//! Execution control; provides the main entry points for execution on the emulated CPU.

use crate::bus::IrqSource;
use crate::error::ExecError;
use crate::processor::Processor;
use crate::watch::WatchError;
use std::sync::{
    Arc,
    atomic::{AtomicU16, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, watch};

/// Result returned by [`Processor::step`] and by the stepping and run
/// functions in this module. `Op` is the CPU's decoded-instruction type.
pub enum StepResult<Op> {
    /// Instruction executed normally.
    Executed(Op),
    /// PC matched a breakpoint; instruction was NOT executed.
    Breakpoint(u16),
    /// A watch expression triggered; instruction was NOT executed.
    WatchTriggered { watch_index: usize, pc: u16 },
    /// A watch expression evaluation failed; instruction was NOT executed.
    WatchError {
        watch_index: usize,
        pc: u16,
        error: WatchError,
    },
    /// CPU is parked waiting for an interrupt (the 6502's WAI).
    Waiting,
    /// CPU is parked until a reset (the 6502's STP); only a reset clears it.
    Stopped,
    /// CPU was reset via the interrupt controller
    Reset,
    /// A free run ended because the caller asked it to stop (via
    /// [`RunHandle::stop`] or a [`RunStopper`]), not because of anything the
    /// CPU did. Never returned by [`Processor::step`] itself.
    StopRequested,
    /// A fatal execution error occurred.
    Error(ExecError),
}

/// A lightweight snapshot of CPU state published by the run loop after each
/// instruction batch so callers can display live state without stopping the CPU.
///
/// `R` is the CPU's register type ([`Processor::Registers`]).
#[derive(Clone)]
pub struct CpuLiveSnapshot<R> {
    /// Register state at the end of the most recent instruction batch.
    pub registers: R,
    /// The memory window the CPU reports as its stack (see
    /// [`Processor::stack_window`]); for the 6502, the whole of page 1.
    pub stack: Vec<u8>,
    /// Bus address of the first byte of `stack`.
    pub stack_base: u16,
    /// Total cycles executed since the last reset.
    pub cycles: u64,
    /// Cycles since CPU was started snapshot
    pub cycles_delta: u64,
    /// Elapsed time since CPU was started
    pub elapsed: Duration,
    /// True if any device is currently asserting IRQ.
    pub irq_active: bool,
    /// True if an NMI is pending (latched but not yet serviced).
    pub nmi_pending: bool,
    /// True when the CPU executed STP and is halted until reset.
    pub cpu_stopped: bool,
    /// True when the CPU executed WAI and is waiting for an interrupt.
    pub cpu_waiting: bool,
    /// The status the program asked to exit with, once the CPU has stopped for
    /// it (see [`MachineState::exit_status`](crate::MachineState::exit_status)).
    pub exit_status: Option<u8>,
    /// Paragraph-aligned start address of the memory page captured in `memory_page`.
    pub memory_page_addr: u16,
    /// 256 bytes of memory starting at `memory_page_addr`.
    pub memory_page: Vec<u8>,
    /// `(address, value)` for every address in `watch_addrs` (see `run_from`),
    /// refreshed every batch. Deliberately NOT a full 64KB copy: `Bus::peek_range`
    /// dispatches per byte (no contiguous-RAM fast path), so a full-range read
    /// costs low-single-digit milliseconds — long enough to measurably delay
    /// this thread's interrupt servicing. Reading only the handful of specific
    /// addresses a caller actually needs (e.g. the debugger's Memory Variables
    /// panel) keeps this effectively free.
    pub watched_values: Vec<(u16, u8)>,
}

/// Builds a [`CpuLiveSnapshot`] from the current state of `cpu`.
///
/// `mem_addr` is the address currently displayed in the memory panel; it is
/// paragraph-aligned (`& 0xfff0`) before the read so the stored page always
/// starts on a paragraph boundary. `watch_addrs` is an arbitrary set of
/// additional addresses to sample individually (see `watched_values`).
fn build_live_snapshot<P: Processor>(
    cpu: &P,
    mem_addr: u16,
    start_cycles: u64,
    start_timestamp: Instant,
    watch_addrs: &[u16],
) -> CpuLiveSnapshot<P::Registers> {
    let (stack_base, stack_len) = cpu.stack_window();
    let mut stack = vec![0u8; stack_len as usize];
    let _ = cpu.bus().peek_range(stack_base, &mut stack);
    let memory_page_addr = mem_addr & 0xfff0;
    let mut memory_page = vec![0u8; 256];
    let _ = cpu.bus().peek_range(memory_page_addr, &mut memory_page);
    let watched_values = watch_addrs
        .iter()
        .map(|&addr| (addr, cpu.bus().peek(addr).unwrap_or(0)))
        .collect();
    CpuLiveSnapshot {
        registers: *cpu.registers(),
        stack,
        stack_base,
        cycles: cpu.cycles(),
        cycles_delta: cpu.cycles() - start_cycles,
        elapsed: start_timestamp.elapsed(),
        irq_active: cpu.interrupts().irq_active(),
        nmi_pending: cpu.interrupts().nmi_pending(),
        cpu_stopped: cpu.is_stopped(),
        exit_status: cpu.machine().exit_status(),
        cpu_waiting: cpu.is_waiting(),
        memory_page_addr,
        memory_page,
        watched_values,
    }
}

const UNLIMITED_SENTINEL: u64 = 0;
const BATCH_SIZE: u32 = 1000;

/// How often [`run_loop`] re-polls `cpu.step()` while the CPU is parked in WAI
/// or STP with nothing to service yet. Keeps the background thread from
/// busy-spinning a full OS thread core while it waits for a device- or
/// UI-triggered interrupt/reset — `cpu.step()` doesn't advance `cycles` while
/// parked, so the normal clock-speed throttling below has nothing to pace on.
const PARKED_POLL_INTERVAL: Duration = Duration::from_millis(1);

/// A command that mutates a running CPU's interrupt state, applied by
/// [`run_loop`], [`step_over_subroutine`], or [`step_return`].
///
/// [`run_loop`] drains the channel once per batch (≤[`BATCH_SIZE`]
/// instructions) rather than once per instruction: a per-instruction
/// `try_recv()` measurably cuts unthrottled throughput (~12% in a bare
/// RAM-only benchmark), while a batch's worth of delay is imperceptible for
/// a UI-driven command. [`step_over_subroutine`] and [`step_return`] drain
/// once per instruction instead, since those operations are typically much
/// shorter than a full batch and must not silently drop a queued command.
#[derive(Debug, Clone, Copy)]
pub enum InterruptCommand {
    /// Asserts the IRQ line from the given source.
    AssertIrq(IrqSource),
    /// Asserts the IRQ line from the given source in one-shot ("pulse") mode; the
    /// source is automatically released once the CPU services an IRQ.
    AssertIrqPulse(IrqSource),
    /// Releases the IRQ line from the given source.
    ReleaseIrq(IrqSource),
    /// Latches a pending NMI.
    TriggerNmi,
    /// Latches a pending RESET.
    TriggerReset,
}

/// Applies `cmd` to `cpu`'s interrupt controller.
fn apply_interrupt_command<P: Processor>(cpu: &mut P, cmd: InterruptCommand) {
    match cmd {
        InterruptCommand::AssertIrq(source) => cpu.interrupts_mut().assert_irq(source),
        InterruptCommand::AssertIrqPulse(source) => cpu.interrupts_mut().assert_irq_pulse(source),
        InterruptCommand::ReleaseIrq(source) => cpu.interrupts_mut().release_irq(source),
        InterruptCommand::TriggerNmi => cpu.interrupts_mut().signal_nmi(),
        InterruptCommand::TriggerReset => cpu.interrupts_mut().signal_reset(),
    }
}

/// Drains and applies all commands currently queued on `cmd_rx`, without blocking.
fn drain_interrupt_commands<P: Processor>(
    cpu: &mut P,
    cmd_rx: &mut mpsc::UnboundedReceiver<InterruptCommand>,
) {
    while let Ok(cmd) = cmd_rx.try_recv() {
        apply_interrupt_command(cpu, cmd);
    }
}

/// Target clock frequency for the emulated CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockSpeed {
    hz: u64,
}

impl ClockSpeed {
    /// Creates a clock speed from a frequency in MHz (e.g. `1.8432` for 1.8432 MHz).
    pub fn mhz(mhz: f64) -> Self {
        Self {
            hz: (mhz * 1_000_000.0).round() as u64,
        }
    }

    /// Creates a clock speed from a frequency in Hz.
    pub fn hz(hz: u64) -> Self {
        assert!(
            hz > 0,
            "hz must be non-zero; use ClockSpeed::unlimited() for no throttling"
        );
        Self { hz }
    }

    /// Creates an unlimited clock speed, disabling throttling in free-running mode.
    pub fn unlimited() -> Self {
        Self {
            hz: UNLIMITED_SENTINEL,
        }
    }

    /// Returns `true` if this clock speed has no throttling limit.
    pub fn is_unlimited(&self) -> bool {
        self.hz == UNLIMITED_SENTINEL
    }

    /// Returns the clock frequency in Hz, or `None` if unlimited.
    pub fn hz_value(&self) -> Option<u64> {
        if self.is_unlimited() {
            None
        } else {
            Some(self.hz)
        }
    }
}

/// Clonable handle for signaling a free-running CPU thread to stop, or to apply
/// an [`InterruptCommand`] without stopping it.
///
/// Obtained from [`RunHandle::stopper`] or created independently via
/// [`RunStopper::channel`]. Multiple stoppers share the same underlying
/// channels, so any one of them can halt the run or send a command.
pub struct RunStopper {
    stop_tx: watch::Sender<bool>,
    cmd_tx: mpsc::UnboundedSender<InterruptCommand>,
}

impl RunStopper {
    /// Creates a `(stopper, stop_receiver, command_receiver)` triple for use
    /// with [`step_over_subroutine`] and [`step_return`].
    pub fn channel() -> (
        Self,
        watch::Receiver<bool>,
        mpsc::UnboundedReceiver<InterruptCommand>,
    ) {
        let (stop_tx, stop_rx) = watch::channel(false);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        (Self { stop_tx, cmd_tx }, stop_rx, cmd_rx)
    }

    /// Signals the CPU thread to stop after the current instruction.
    ///
    /// Non-blocking. The CPU will stop before the next instruction fetch.
    pub fn stop(&self) {
        let _ = self.stop_tx.send(true);
    }

    /// Asserts the IRQ line from `source` on the running CPU.
    ///
    /// Non-blocking; applied between instructions. Does not stop the run —
    /// the CPU services the interrupt transparently, the same as a
    /// device-sourced IRQ.
    pub fn assert_irq(&self, source: IrqSource) {
        let _ = self.cmd_tx.send(InterruptCommand::AssertIrq(source));
    }

    /// Asserts the IRQ line from `source` on the running CPU in one-shot ("pulse")
    /// mode: `source` is automatically released once the CPU services an IRQ, rather
    /// than requiring a separate `release_irq` call.
    ///
    /// Non-blocking; applied between instructions. Does not stop the run — the CPU
    /// services the interrupt transparently, then releases `source` itself.
    pub fn assert_irq_pulse(&self, source: IrqSource) {
        let _ = self.cmd_tx.send(InterruptCommand::AssertIrqPulse(source));
    }

    /// Releases the IRQ line from `source` on the running CPU.
    ///
    /// Non-blocking; applied between instructions.
    pub fn release_irq(&self, source: IrqSource) {
        let _ = self.cmd_tx.send(InterruptCommand::ReleaseIrq(source));
    }

    /// Latches a pending NMI on the running CPU.
    ///
    /// Non-blocking; applied between instructions. Does not stop the run —
    /// the CPU services the NMI transparently.
    pub fn trigger_nmi(&self) {
        let _ = self.cmd_tx.send(InterruptCommand::TriggerNmi);
    }

    /// Latches a pending RESET on the running CPU.
    ///
    /// Non-blocking; applied between instructions. Once serviced, the CPU
    /// returns [`StepResult::Reset`], which halts the run (the same way a
    /// breakpoint does) so the caller can inspect post-reset state.
    pub fn trigger_reset(&self) {
        let _ = self.cmd_tx.send(InterruptCommand::TriggerReset);
    }
}

/// Handle returned by [`run`] for controlling a free-running CPU thread.
///
/// Dropping the handle without calling [`stop`](RunHandle::stop) or
/// [`take_cpu`](RunHandle::take_cpu) will leave the CPU thread running until it
/// stops on its own (breakpoint, watch trigger, error, or STP/WAI with no
/// interrupt — unless started with `park_on_stall: true`, see [`run_from`]).
pub struct RunHandle<P: Processor> {
    /// Sends `true` to ask the CPU thread to stop after the current instruction.
    stop_tx: watch::Sender<bool>,
    /// Sends [`InterruptCommand`]s to be applied between instructions.
    cmd_tx: mpsc::UnboundedSender<InterruptCommand>,
    /// Receives the `StepResult` that caused execution to stop.
    result_rx: oneshot::Receiver<StepResult<P::Op>>,
    /// Receives ownership of the CPU after the thread exits.
    cpu_rx: oneshot::Receiver<P>,
    /// Receives live CPU snapshots published by the run loop after each batch.
    live_rx: watch::Receiver<Option<CpuLiveSnapshot<P::Registers>>>,
}

impl<P: Processor> RunHandle<P> {
    /// Returns a [`RunStopper`] that shares the underlying stop and command channels.
    ///
    /// The stopper can be cloned and stored independently; calling any of its
    /// methods on any clone affects this run.
    pub fn stopper(&self) -> RunStopper {
        RunStopper {
            stop_tx: self.stop_tx.clone(),
            cmd_tx: self.cmd_tx.clone(),
        }
    }

    /// Signals the CPU thread to stop after the current instruction.
    ///
    /// Non-blocking. The CPU will stop before the next instruction fetch.
    pub fn stop(&self) {
        let _ = self.stop_tx.send(true);
    }

    /// Awaits the [`StepResult`] that caused execution to stop.
    ///
    /// If execution was stopped via [`stop`](RunHandle::stop) or a
    /// [`RunStopper`], the result is [`StepResult::StopRequested`].
    pub async fn wait(self) -> StepResult<P::Op> {
        self.result_rx
            .await
            .expect("CPU thread exited without sending result")
    }

    /// Signals the CPU thread to stop and awaits the CPU being returned.
    pub async fn take_cpu(self) -> P {
        self.stop();
        self.cpu_rx
            .await
            .expect("CPU thread exited without returning CPU")
    }

    /// Awaits both the final [`StepResult`] and the CPU without sending a stop
    /// signal.
    ///
    /// Use a [`RunStopper`] obtained from [`stopper`](RunHandle::stopper) to
    /// trigger a stop externally before awaiting here.
    pub async fn take_cpu_with_result(self) -> (StepResult<P::Op>, P) {
        let result = self
            .result_rx
            .await
            .expect("CPU thread exited without sending result");
        let cpu = self
            .cpu_rx
            .await
            .expect("CPU thread exited without returning CPU");
        (result, cpu)
    }

    /// Returns the most recent live snapshot published by the run loop, or
    /// `None` if no batch has completed yet.
    pub fn live_snapshot(&self) -> Option<CpuLiveSnapshot<P::Registers>> {
        self.live_rx.borrow().clone()
    }

    /// Returns a cloned receiver for the live snapshot channel so the caller
    /// can poll it independently (e.g. from a background task).
    pub fn subscribe_live(&self) -> watch::Receiver<Option<CpuLiveSnapshot<P::Registers>>> {
        self.live_rx.clone()
    }
}

/// Fetches, decodes, and executes one instruction. Returns the step result.
///
/// Halts before executing if PC matches a breakpoint or a watch expression triggers.
/// Use [`step_over_breakpoint`] to advance past the breakpoint at the current PC without
/// disabling it.
pub fn step_into<P: Processor>(cpu: &mut P) -> StepResult<P::Op> {
    let result = cpu.step(None, true);
    cpu.flush_trace();
    result
}

/// Like [`step_into`], but skips the breakpoint and watch check at `skip_pc`.
///
/// Use this when the debugger is already halted at `skip_pc` (due to a breakpoint or
/// watch trigger) and needs to advance past it without requiring the breakpoint to be
/// disabled first. All other addresses are checked normally.
pub fn step_over_breakpoint<P: Processor>(cpu: &mut P, skip_pc: u16) -> StepResult<P::Op> {
    let result = cpu.step(Some(skip_pc), true);
    cpu.flush_trace();
    result
}

/// Executes one logical step, treating a subroutine call as atomic.
///
/// If the instruction at the current PC is a subroutine call (as reported by
/// [`Processor::subroutine_call_len`]; for the 6502, JSR), sets a temporary
/// breakpoint just past it and steps until that breakpoint is hit (or
/// execution halts for any other reason). For all other instructions, behaves
/// identically to [`step_over_breakpoint`] at the current PC.
///
/// The temporary breakpoint is always removed before returning, even if
/// execution stops before reaching it. Any pre-existing breakpoint at the
/// target address is preserved.
///
/// `stop_rx` is checked between instructions; if it becomes `true` the
/// operation is interrupted and `None` is returned with the CPU left at its
/// current PC. Returns `Some(result)` on natural completion.
///
/// `cmd_rx` is drained between instructions, applying any queued
/// [`InterruptCommand`]s to the CPU.
///
/// If `live_tx` is `Some`, a [`CpuLiveSnapshot`] is published periodically
/// (based on a fixed batch size) so callers can display live state while a long
/// subroutine runs. `mem_view_addr` selects which 256-byte page is captured in
/// each snapshot; pass `Arc::new(AtomicU16::new(0))` if not needed.
pub fn step_over_subroutine<P: Processor>(
    cpu: &mut P,
    stop_rx: &watch::Receiver<bool>,
    cmd_rx: &mut mpsc::UnboundedReceiver<InterruptCommand>,
    live_tx: Option<&watch::Sender<Option<CpuLiveSnapshot<P::Registers>>>>,
    mem_view_addr: &Arc<AtomicU16>,
) -> Option<StepResult<P::Op>> {
    let pc = cpu.pc();
    let Some(call_len) = cpu.subroutine_call_len(pc) else {
        return Some(step_over_breakpoint(cpu, pc));
    };

    let target = pc.wrapping_add(call_len);
    let already_set = cpu.breakpoints().contains(&target);
    if !already_set {
        cpu.add_breakpoint(target);
    }

    // The first iteration skips the breakpoint/watch check at the current PC so
    // that a breakpoint there does not immediately re-fire before the call executes.
    // Subsequent iterations use the normal step() path.
    let mut first = true;
    let mut steps = 0u32;
    let start_cycles = cpu.cycles();
    let start_timestamp = Instant::now();
    // Steps directly via `cpu.step()` rather than `step_into`/`step_over_breakpoint`
    // (which each flush the trace stream on their own): this loop may execute many
    // instructions, and the trace stream only needs to be flushed once, when this
    // function itself returns.
    let result = loop {
        if *stop_rx.borrow() {
            if !already_set {
                cpu.remove_breakpoint(target);
            }
            cpu.flush_trace();
            return None;
        }
        drain_interrupt_commands(cpu, cmd_rx);
        let res = if first {
            first = false;
            cpu.step(Some(pc), true)
        } else {
            cpu.step(None, true)
        };
        steps += 1;
        if let Some(tx) = live_tx.filter(|_| steps.is_multiple_of(BATCH_SIZE)) {
            let _ = tx.send(Some(build_live_snapshot(
                cpu,
                mem_view_addr.load(Ordering::Relaxed),
                start_cycles,
                start_timestamp,
                &[],
            )));
        }
        match res {
            StepResult::Executed(op) => {
                if cpu.pc() == target {
                    break if already_set {
                        StepResult::Breakpoint(target)
                    } else {
                        StepResult::Executed(op)
                    };
                }
            }
            StepResult::Breakpoint(addr) if addr == target => {
                // The temporary breakpoint fired before the instruction executed.
                // Surface as Breakpoint only if the caller owns it; otherwise
                // remove it and let the next step execute the instruction.
                if already_set {
                    break StepResult::Breakpoint(target);
                }
                cpu.remove_breakpoint(target);
                match cpu.step(None, true) {
                    StepResult::Executed(op) => break StepResult::Executed(op),
                    other => break other,
                }
            }
            other => break other,
        }
    };

    if !already_set {
        cpu.remove_breakpoint(target);
    }
    cpu.flush_trace();
    Some(result)
}

/// Runs until the current subroutine returns.
///
/// Return is detected from the stack: a [`Processor::stack_mark`] is taken at
/// the start, and execution halts after the first instruction once
/// [`Processor::has_returned_from`] reports that the stack has unwound past it
/// (for the 6502: `S` has risen above its starting value, using wrapping 8-bit
/// arithmetic, after an `RTS` or `RTI`). Execution also
/// halts on any non-[`StepResult::Executed`] result (breakpoint, watch trigger,
/// error, STP, WAI stall).
///
/// `stop_rx` is checked between instructions; if it becomes `true` the
/// operation is interrupted and `None` is returned with the CPU left at its
/// current PC. Returns `Some(result)` on natural completion.
///
/// `cmd_rx` is drained between instructions, applying any queued
/// [`InterruptCommand`]s to the CPU.
///
/// If `live_tx` is `Some`, a [`CpuLiveSnapshot`] is published periodically
/// (based on a fixed batch size) so callers can display live state while a long
/// subroutine runs. `mem_view_addr` selects which 256-byte page is captured in
/// each snapshot; pass `Arc::new(AtomicU16::new(0))` if not needed.
pub fn step_return<P: Processor>(
    cpu: &mut P,
    stop_rx: &watch::Receiver<bool>,
    cmd_rx: &mut mpsc::UnboundedReceiver<InterruptCommand>,
    live_tx: Option<&watch::Sender<Option<CpuLiveSnapshot<P::Registers>>>>,
    mem_view_addr: &Arc<AtomicU16>,
) -> Option<StepResult<P::Op>> {
    let mark = cpu.stack_mark();
    // The first iteration skips the breakpoint/watch check at the current PC so
    // that a breakpoint there does not immediately re-fire before the instruction executes.
    let mut first = true;
    let mut steps = 0u32;
    let start_cycles = cpu.cycles();
    let start_timestamp = Instant::now();
    // Steps directly via `cpu.step()` rather than `step_into`/`step_over_breakpoint`
    // (which each flush the trace stream on their own): this loop may execute many
    // instructions, and the trace stream only needs to be flushed once, when this
    // function itself returns.
    loop {
        if *stop_rx.borrow() {
            cpu.flush_trace();
            return None;
        }
        drain_interrupt_commands(cpu, cmd_rx);
        let pc = cpu.pc();
        let res = if first {
            first = false;
            cpu.step(Some(pc), true)
        } else {
            cpu.step(None, true)
        };
        steps += 1;
        if let Some(tx) = live_tx.filter(|_| steps.is_multiple_of(BATCH_SIZE)) {
            let _ = tx.send(Some(build_live_snapshot(
                cpu,
                mem_view_addr.load(Ordering::Relaxed),
                start_cycles,
                start_timestamp,
                &[],
            )));
        }
        match res {
            StepResult::Executed(op) if cpu.has_returned_from(mark) => {
                cpu.flush_trace();
                return Some(StepResult::Executed(op));
            }
            StepResult::Executed(_) => {}
            other => {
                cpu.flush_trace();
                return Some(other);
            }
        }
    }
}

/// Moves `cpu` into a dedicated OS thread and begins executing instructions.
///
/// Returns a [`RunHandle`] immediately. The CPU thread runs `cpu.step()` in a
/// tight loop, checking the stop signal between instructions. When execution
/// halts (stop signal, breakpoint, watch trigger, error, STP, or WAI stall),
/// both the final [`StepResult`] and the CPU are sent back via the handle's
/// channels.
///
/// When `cpu.clock_speed()` is not unlimited, the loop sleeps as needed to
/// match the target frequency. Throttling is batched over ~1000 instructions
/// to avoid per-instruction syscall overhead.
pub fn run<P: Processor>(cpu: P) -> RunHandle<P> {
    run_from(cpu, None, Arc::new(AtomicU16::new(0)), Vec::new(), false)
}

/// Like [`run`], but skips the breakpoint/watch check at `skip_pc` on the
/// first instruction. Use this when the CPU is already halted at a breakpoint
/// and the caller wants execution to continue past it without disabling it.
///
/// `mem_view_addr` is an atomically-updated address that controls which 256-byte
/// memory page is captured in each live snapshot. The caller should store the
/// paragraph-aligned address currently shown in the memory panel, and may update
/// it at any time while running.
///
/// `park_on_stall` selects what happens when the CPU executes WAI or STP with
/// nothing yet to wake it: `false` (the default via [`run`]) halts the loop
/// immediately, same as a breakpoint — the natural behavior for a headless run
/// where a halted CPU means the program is done. `true` keeps the background
/// thread alive instead, calling `cpu.step()` at a throttled poll rate (see
/// [`PARKED_POLL_INTERVAL`]) so devices keep ticking and a later device- or
/// UI-triggered interrupt/reset is picked up with no caller action needed —
/// used by the debugger's Run command, where landing back in the halted state
/// on every WAI/STP would defeat the point of pressing Run. WAI resumes
/// transparently (execution just continues); STP still halts the loop once
/// serviced, via [`StepResult::Reset`], same as a manually-triggered reset.
///
/// `watch_addrs` is an arbitrary, fixed set of additional addresses sampled
/// individually into each live snapshot's `watched_values` (see
/// `CpuLiveSnapshot`) — for a caller that needs to track several scattered
/// addresses (e.g. the debugger's Memory Variables panel) without paying for
/// a full-address-space read. Fixed for the run's duration: pass `Vec::new()`
/// if not needed.
pub fn run_from<P: Processor>(
    cpu: P,
    skip_pc: Option<u16>,
    mem_view_addr: Arc<AtomicU16>,
    watch_addrs: Vec<u16>,
    park_on_stall: bool,
) -> RunHandle<P> {
    let (stop_tx, stop_rx) = watch::channel(false);
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (result_tx, result_rx) = oneshot::channel();
    let (cpu_tx, cpu_rx) = oneshot::channel();
    let (live_tx, live_rx) = watch::channel(None);

    std::thread::spawn(move || {
        run_loop(
            cpu,
            skip_pc,
            mem_view_addr,
            watch_addrs,
            RunLoopChannels {
                stop_rx,
                cmd_rx,
                live_tx,
                result_tx,
                cpu_tx,
            },
            park_on_stall,
        );
    });

    RunHandle {
        stop_tx,
        cmd_tx,
        result_rx,
        cpu_rx,
        live_rx,
    }
}

/// The channel endpoints [`run_loop`] reports through, counterparts to the ones [`RunHandle`]
/// holds. Grouped into a struct because `run_loop` otherwise exceeds clippy's argument-count
/// lint.
struct RunLoopChannels<P: Processor> {
    stop_rx: watch::Receiver<bool>,
    cmd_rx: mpsc::UnboundedReceiver<InterruptCommand>,
    live_tx: watch::Sender<Option<CpuLiveSnapshot<P::Registers>>>,
    result_tx: oneshot::Sender<StepResult<P::Op>>,
    cpu_tx: oneshot::Sender<P>,
}

/// The free-running loop behind [`run_from`]. Generic over `P`, so each CPU
/// gets its own compiled copy and `cpu.step()` is a static call.
fn run_loop<P: Processor>(
    mut cpu: P,
    skip_pc: Option<u16>,
    mem_view_addr: Arc<AtomicU16>,
    watch_addrs: Vec<u16>,
    channels: RunLoopChannels<P>,
    park_on_stall: bool,
) {
    let RunLoopChannels {
        stop_rx,
        mut cmd_rx,
        live_tx,
        result_tx,
        cpu_tx,
    } = channels;
    let start_cycles = cpu.cycles();
    let start_timestamp = Instant::now();
    let hz = cpu.clock_speed().hz_value();

    let mut first = skip_pc.is_some();
    let check_breakpoints = !cpu.breakpoints().is_empty() || !cpu.evaluator().is_empty();

    let final_result = 'outer: loop {
        // Drained once per batch rather than once per instruction: a
        // per-instruction `try_recv()` measurably cuts unthrottled
        // throughput (see InterruptCommand doc comment), while a batch's
        // worth of delay (≤1000 instructions, sub-millisecond even at
        // throttled clock speeds) is imperceptible for a UI-driven command.
        drain_interrupt_commands(&mut cpu, &mut cmd_rx);
        let mut parked = false;
        for _ in 0..BATCH_SIZE {
            if *stop_rx.borrow() {
                break 'outer None;
            }
            let res = if first {
                first = false;
                cpu.step(skip_pc, check_breakpoints)
            } else {
                cpu.step(None, check_breakpoints)
            };
            match res {
                StepResult::Executed(_) => {}
                StepResult::Waiting | StepResult::Stopped if park_on_stall => {
                    // Parked with nothing to service yet; break out of the batch
                    // so a queued interrupt command gets drained and the poll
                    // rate gets throttled below, rather than spinning through
                    // the rest of this batch doing nothing.
                    parked = true;
                    break;
                }
                other => break 'outer Some(other),
            }
        }

        // Publish a live snapshot after each batch so the frontend can display
        // current state without stopping the CPU.
        let snapshot = build_live_snapshot(
            &cpu,
            mem_view_addr.load(Ordering::Relaxed),
            start_cycles,
            start_timestamp,
            &watch_addrs,
        );
        let _ = live_tx.send(Some(snapshot));

        if parked {
            // Keep polling — a device driven by `cpu.step()`'s tick may still
            // assert IRQ/NMI/RESET, and a UI-driven command may still arrive on
            // the next batch's drain.
            std::thread::sleep(PARKED_POLL_INTERVAL);
            continue;
        }

        if let Some(hz) = hz {
            let elapsed_ns = start_timestamp.elapsed().as_nanos() as u64;
            let expected_cycles = (elapsed_ns as u128 * hz as u128 / 1_000_000_000) as u64;
            let actual_cycles = cpu.cycles() - start_cycles;
            if actual_cycles > expected_cycles {
                let excess = actual_cycles - expected_cycles;
                let sleep_ns = excess * 1_000_000_000 / hz;
                std::thread::sleep(std::time::Duration::from_nanos(sleep_ns));
            }
        }
    };

    // A clean stop (the loop broke out with `None`) is reported explicitly.
    let step_result = final_result.unwrap_or(StepResult::StopRequested);

    // Flush once here, rather than after every instruction in the loop above:
    // this is the point at which the caller awaiting `RunHandle` resumes, and the
    // trace stream only needs to reflect this run's instructions by then.
    cpu.flush_trace();

    let _ = result_tx.send(step_result);
    let _ = cpu_tx.send(cpu);
}

#[cfg(test)]
mod toy;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mhz_converts_to_hz() {
        assert_eq!(ClockSpeed::mhz(1.0).hz_value(), Some(1_000_000));
        assert_eq!(ClockSpeed::mhz(1.8432).hz_value(), Some(1_843_200));
        assert_eq!(ClockSpeed::mhz(2.0).hz_value(), Some(2_000_000));
    }

    #[test]
    fn hz_constructor() {
        assert_eq!(ClockSpeed::hz(1_843_200).hz_value(), Some(1_843_200));
    }

    #[test]
    fn unlimited_sentinel() {
        let s = ClockSpeed::unlimited();
        assert!(s.is_unlimited());
        assert_eq!(s.hz_value(), None);
    }

    #[test]
    fn non_unlimited_is_not_unlimited() {
        assert!(!ClockSpeed::mhz(1.0).is_unlimited());
        assert!(!ClockSpeed::hz(1).is_unlimited());
    }
}
