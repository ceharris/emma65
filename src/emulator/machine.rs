//! Machine state shared by every CPU: the bus, interrupt controller, watch
//! evaluator, breakpoints, clock speed, cycle count, vector resolver, and log
//! sender.
//!
//! The 6502 [`Cpu`](crate::emulator::Cpu) embeds a [`MachineState`] alongside
//! the state that belongs to its instruction set (registers, decode table,
//! variant, invalid-opcode policy, WAI/STP flags, and tracing).

use crate::emulator::LogSender;
use crate::emulator::bus::{Bus, InterruptController};
use crate::emulator::cpu::StepResult;
use crate::emulator::cpu::vector::VectorResolver;
use crate::emulator::exec::ClockSpeed;
use crate::watch::{WatchContext, WatchEvaluator};
use std::collections::HashSet;

/// Machine state that isn't specific to any instruction set.
pub(crate) struct MachineState {
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
}

impl MachineState {
    /// Creates machine state around `bus`, with no breakpoints or
    /// watchpoints and a cycle count of zero.
    pub(crate) fn new(
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
        }
    }

    /// Returns `true` if at least one currently-active IRQ source is recognized by the
    /// installed [`VectorResolver`] (i.e. not masked out via
    /// [`VectorResolver::irq_mask`]). Unlike [`InterruptController::irq_active`], which
    /// reflects the raw physical IRQ line, this is what actually gates IRQ servicing and
    /// WAI wake-up — a source can assert the line without being recognized here.
    #[inline]
    pub(crate) fn irq_recognized(&self) -> bool {
        self.interrupts.active_sources_mask() & self.vector_resolver.irq_mask() != 0
    }

    /// Advances the cumulative cycle count, ticks all bus devices, and re-polls
    /// interrupt state. Called once per step after cycles for that step are known,
    /// regardless of which path (interrupt service, invalid opcode, normal
    /// execution) produced them.
    #[inline]
    pub(crate) fn finish_cycle(&mut self, cycles: u8) {
        self.cycles += cycles as u64;
        self.log_sender.set_cycles(self.cycles);
        self.bus.tick_devices(cycles as u32);
        self.interrupts
            .poll_devices(self.bus.device_interrupt_states());
    }

    /// Ticks all bus devices by one cycle and re-polls interrupt state, without
    /// advancing the cycle count. Used while the CPU is parked (WAI/STP) so that
    /// devices keep running and can raise the interrupt that wakes it.
    #[inline]
    pub(crate) fn tick_idle(&mut self) {
        self.bus.tick_devices(1);
        self.interrupts
            .poll_devices(self.bus.device_interrupt_states());
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
    pub(crate) fn check_halt<'a, C, F>(
        &'a mut self,
        pc: u16,
        skip_pc: Option<u16>,
        make_ctx: F,
    ) -> Option<StepResult>
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
