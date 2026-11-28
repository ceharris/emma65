//! Machine state shared by every CPU: the bus, interrupt controller, watch
//! evaluator, breakpoints, clock speed, cycle count, vector resolver, and log
//! sender.
//!
//! Each CPU embeds a [`MachineState`] alongside the state that belongs to its
//! instruction set (for the 6502: registers, decode table, variant,
//! invalid-opcode policy, WAI/STP flags, and tracing), and exposes it through
//! [`Processor::machine`](crate::Processor::machine).

use crate::LogSender;
use crate::bus::vector::VectorResolver;
use crate::bus::{Bus, InterruptController};
use crate::exec::{ClockSpeed, StepResult};
use crate::watch::{WatchContext, WatchEvaluator};
use std::collections::HashSet;

/// Machine state that isn't specific to any instruction set.
///
/// Reached through [`Processor::machine`](crate::Processor::machine); its
/// fields are crate-private, and the [`Processor`](crate::Processor)
/// trait's default methods expose them.
pub struct MachineState {
    /// The memory bus; owns all RAM, ROM, and IO device regions.
    pub(crate) bus: Bus,
    /// Interrupt controller; tracks IRQ sources and pending NMI.
    pub(crate) interrupts: InterruptController,
    /// Watch expression evaluator; owns watchpoints and variable storage.
    pub(crate) evaluator: WatchEvaluator,
    /// PC addresses that trigger a `StepResult::Breakpoint` before execution.
    pub(crate) breakpoints: HashSet<u16>,
    /// Target clock frequency; used by free-running mode to throttle execution.
    pub(crate) clock_speed: ClockSpeed,
    /// Cumulative clock cycles elapsed since the last reset.
    pub(crate) cycles: u64,
    /// Resolves the effective address for a RESET/NMI/IRQ/BRK vector fetch.
    pub(crate) vector_resolver: Box<dyn VectorResolver>,
    /// Sender for structured diagnostic messages.
    pub(crate) log_sender: LogSender,
    /// The status a program asked to exit with (see the `exit` device), once
    /// the CPU has stopped for it.
    pub(crate) exit_status: Option<u8>,
}

impl MachineState {
    /// Creates machine state around `bus`, with no breakpoints or
    /// watchpoints and a cycle count of zero.
    pub fn new(
        bus: Bus,
        clock_speed: ClockSpeed,
        vector_resolver: Box<dyn VectorResolver>,
        log_sender: LogSender,
    ) -> Self {
        Self {
            bus,
            interrupts: InterruptController::new(),
            evaluator: WatchEvaluator::new(),
            breakpoints: HashSet::new(),
            clock_speed,
            cycles: 0,
            vector_resolver,
            log_sender,
            exit_status: None,
        }
    }

    /// The status a program asked to exit with through the `exit` device, if
    /// the CPU has stopped for one since its last reset. A host ends the run
    /// with this as its exit code.
    #[inline]
    pub fn exit_status(&self) -> Option<u8> {
        self.exit_status
    }

    /// Records (or, with `None`, clears) the exit status. A CPU sets it when it
    /// stops for a [`CpuRequest::Exit`](crate::bus::CpuRequest::Exit) and
    /// clears it on reset.
    pub fn set_exit_status(&mut self, status: Option<u8>) {
        self.exit_status = status;
    }

    /// Returns the bus.
    #[inline]
    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    /// Returns the bus, mutably.
    #[inline]
    pub fn bus_mut(&mut self) -> &mut Bus {
        &mut self.bus
    }

    /// Returns the interrupt controller.
    #[inline]
    pub fn interrupts(&self) -> &InterruptController {
        &self.interrupts
    }

    /// Returns the interrupt controller, mutably.
    #[inline]
    pub fn interrupts_mut(&mut self) -> &mut InterruptController {
        &mut self.interrupts
    }

    /// Returns the watch evaluator whose watchpoints [`check_halt`](Self::check_halt)
    /// evaluates.
    #[inline]
    pub fn evaluator(&self) -> &WatchEvaluator {
        &self.evaluator
    }

    /// Returns the watch evaluator, mutably.
    #[inline]
    pub fn evaluator_mut(&mut self) -> &mut WatchEvaluator {
        &mut self.evaluator
    }

    /// Returns the breakpoint set [`check_halt`](Self::check_halt) consults.
    #[inline]
    pub fn breakpoints(&self) -> &HashSet<u16> {
        &self.breakpoints
    }

    /// Returns the breakpoint set, mutably.
    #[inline]
    pub fn breakpoints_mut(&mut self) -> &mut HashSet<u16> {
        &mut self.breakpoints
    }

    /// Returns the target clock speed.
    #[inline]
    pub fn clock_speed(&self) -> ClockSpeed {
        self.clock_speed
    }

    /// Returns the cycles elapsed since the last reset, as advanced by
    /// [`finish_cycle`](Self::finish_cycle).
    #[inline]
    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    /// Zeroes the cycle count (and the log sender's cycle stamp), as a CPU
    /// does on reset.
    #[inline]
    pub fn reset_cycles(&mut self) {
        self.cycles = 0;
        self.log_sender.set_cycles(0);
    }

    /// Returns the sender for structured diagnostic messages.
    #[inline]
    pub fn log_sender(&self) -> &LogSender {
        &self.log_sender
    }

    /// Replaces the sender for structured diagnostic messages.
    pub fn set_log_sender(&mut self, sender: LogSender) {
        self.log_sender = sender;
    }

    /// Resolves the effective address to read a vector from, given its
    /// nominal address, through the installed [`VectorResolver`] (which may
    /// consult the interrupt controller, e.g. to redirect an IRQ vector to the
    /// highest-priority active source).
    #[inline]
    pub fn resolve_vector(&self, vector_addr: u16) -> u16 {
        self.vector_resolver.resolve(vector_addr, &self.interrupts)
    }

    /// Returns `true` if at least one currently-active IRQ source is recognized by the
    /// installed [`VectorResolver`] (i.e. not masked out via
    /// [`VectorResolver::irq_mask`]). Unlike [`InterruptController::irq_active`], which
    /// reflects the raw physical IRQ line, this is what actually gates IRQ servicing and
    /// WAI wake-up — a source can assert the line without being recognized here.
    #[inline]
    pub fn irq_recognized(&self) -> bool {
        self.interrupts.active_sources_mask() & self.vector_resolver.irq_mask() != 0
    }

    /// Advances the cumulative cycle count, ticks all bus devices, and re-polls
    /// interrupt state. Called once per step after cycles for that step are known,
    /// regardless of which path (interrupt service, invalid opcode, normal
    /// execution) produced them.
    #[inline]
    pub fn finish_cycle(&mut self, cycles: u8) {
        self.cycles += cycles as u64;
        self.log_sender.set_cycles(self.cycles);
        self.bus.tick_devices(cycles as u32);
        self.poll_devices();
    }

    /// Syncs device interrupt state, and any request a device made, into the
    /// interrupt controller.
    #[inline]
    fn poll_devices(&mut self) {
        self.interrupts
            .poll_devices(self.bus.device_interrupt_states());
        if let Some(request) = self.bus.take_written_request() {
            self.interrupts.latch_request(request);
        }
    }

    /// Ticks all bus devices by one cycle and re-polls interrupt state, without
    /// advancing the cycle count. Used while the CPU is parked (WAI/STP) so that
    /// devices keep running and can raise the interrupt that wakes it.
    #[inline]
    pub fn tick_idle(&mut self) {
        self.bus.tick_devices(1);
        self.poll_devices();
    }

    /// Checks whether execution should halt before the instruction at `pc`:
    /// returns `StepResult::Breakpoint` if `pc` is a breakpoint, or
    /// `StepResult::WatchTriggered`/`StepResult::WatchError` if a watch
    /// expression fires or fails. Returns `None` if execution should proceed.
    /// Both checks are skipped when `pc == skip_pc`, so a debugger can advance
    /// past an address it is already halted at.
    ///
    /// `make_ctx` builds the CPU's own [`WatchContext`] over the bus, so
    /// register, flag, and endianness semantics stay with the instruction set.
    /// It is called only when at least one watchpoint is installed.
    #[inline]
    pub fn check_halt<'a, Op, C, F>(
        &'a mut self,
        pc: u16,
        skip_pc: Option<u16>,
        make_ctx: F,
    ) -> Option<StepResult<Op>>
    where
        C: WatchContext + 'a,
        F: FnOnce(&'a Bus) -> C,
    {
        if skip_pc == Some(pc) {
            return None;
        }

        if !self.breakpoints.is_empty() && self.breakpoints.contains(&pc) {
            return Some(StepResult::Breakpoint(pc));
        }

        if self.evaluator.is_empty() {
            return None;
        }

        let ctx = make_ctx(&self.bus);
        match self.evaluator.evaluate_all(&ctx) {
            Ok(Some(index)) => Some(StepResult::WatchTriggered {
                watch_index: index,
                pc,
            }),
            Err((index, error)) => Some(StepResult::WatchError {
                watch_index: index,
                pc,
                error,
            }),
            Ok(None) => None,
        }
    }
}
