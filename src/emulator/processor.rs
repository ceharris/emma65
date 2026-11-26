//! The [`Processor`] trait: what the [`exec`](crate::emulator::exec) module needs
//! from a CPU in order to run, step, and inspect it.
//!
//! Each CPU family implements `Processor` once. `exec`'s run loop and stepping
//! functions are generic over `P: Processor`, so every CPU gets its own compiled
//! copy of the loop and `step()` stays a static call: there is no dynamic
//! dispatch on the hot path.

use crate::emulator::bus::{Bus, InterruptController};
use crate::emulator::error::ExecError;
use crate::emulator::exec::{ClockSpeed, StepResult};
use crate::emulator::machine::MachineState;
use crate::watch::{Operand, WatchEvaluator};
use std::collections::HashSet;

/// Whether a CPU is executing instructions or parked waiting to be woken.
///
/// The 6502's WAI and STP map onto `Waiting` and `Stopped`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaltState {
    /// Executing instructions normally.
    Running,
    /// Parked until an interrupt (or reset) arrives, e.g. after the 6502's WAI.
    Waiting,
    /// Parked until a reset, e.g. after the 6502's STP.
    Stopped,
}

/// A CPU that the [`exec`](crate::emulator::exec) module can run and step.
///
/// An implementation owns a [`MachineState`] (bus, interrupt controller, watch
/// evaluator, breakpoints, clock speed, cycle count) plus its own registers and
/// decoder. The default methods forward to [`machine`](Processor::machine), so
/// generic code can reach the shared state without knowing the CPU.
pub trait Processor: Send + 'static {
    /// The register snapshot published in live snapshots and shown by a debugger.
    type Registers: Copy + Send + Sync + 'static;
    /// The decoded instruction carried by [`StepResult::Executed`].
    type Op: Copy + Send + 'static;
    /// An opaque stack-depth marker used by
    /// [`step_return`](crate::emulator::exec::step_return).
    type StackMark: Copy + Send;

    /// Returns the machine state shared by every CPU.
    fn machine(&self) -> &MachineState;
    /// Returns the machine state shared by every CPU, mutably.
    fn machine_mut(&mut self) -> &mut MachineState;

    /// Fetches, decodes, and executes one instruction, or services a pending
    /// interrupt or reset. Skips the breakpoint/watch check at `skip_pc`, and
    /// skips it everywhere if `check_breakpoints` is `false`.
    fn step(&mut self, skip_pc: Option<u16>, check_breakpoints: bool) -> StepResult<Self::Op>;
    /// Resets the CPU: reads its reset vector, initializes registers, and
    /// clears any halt state.
    fn reset(&mut self) -> Result<(), ExecError>;
    /// Returns the current register state.
    fn registers(&self) -> &Self::Registers;
    /// Returns the program counter.
    fn pc(&self) -> u16;
    /// Returns whether the CPU is running or parked.
    fn halt_state(&self) -> HaltState;
    /// Flushes the trace stream, if tracing is on, so an independent reader
    /// sees every record emitted so far.
    fn flush_trace(&mut self);

    /// Returns the instruction's length in bytes if the instruction at `pc`
    /// is a subroutine call, or `None` otherwise. Used by step-over to run the
    /// whole call as one step.
    fn subroutine_call_len(&self, pc: u16) -> Option<u16>;
    /// Returns a marker for the current stack depth, for step-out.
    fn stack_mark(&self) -> Self::StackMark;
    /// Returns `true` once the stack has unwound past `mark`, meaning the
    /// subroutine that was running when `mark` was taken has returned.
    fn has_returned_from(&self, mark: Self::StackMark) -> bool;
    /// Returns `(base, len)`: the memory window a live snapshot captures as
    /// "the stack".
    fn stack_window(&self) -> (u16, u16);

    /// Maps a register name to its `Operand` ID, for
    /// [`WatchCompiler::new`](crate::watch::WatchCompiler::new).
    fn map_register_name(name: &str) -> Option<Operand>;
    /// Maps a flag name to its `Operand` ID, for
    /// [`WatchCompiler::new`](crate::watch::WatchCompiler::new).
    fn map_flag_name(name: &str) -> Option<Operand>;

    /// Returns a reference to the bus.
    fn bus(&self) -> &Bus {
        &self.machine().bus
    }
    /// Returns a mutable reference to the bus.
    fn bus_mut(&mut self) -> &mut Bus {
        &mut self.machine_mut().bus
    }
    /// Returns a reference to the interrupt controller.
    fn interrupts(&self) -> &InterruptController {
        &self.machine().interrupts
    }
    /// Returns a mutable reference to the interrupt controller.
    fn interrupts_mut(&mut self) -> &mut InterruptController {
        &mut self.machine_mut().interrupts
    }
    /// Returns a reference to the watch evaluator.
    fn evaluator(&self) -> &WatchEvaluator {
        &self.machine().evaluator
    }
    /// Returns a mutable reference to the watch evaluator.
    fn evaluator_mut(&mut self) -> &mut WatchEvaluator {
        &mut self.machine_mut().evaluator
    }
    /// Adds `addr` to the breakpoint set.
    fn add_breakpoint(&mut self, addr: u16) {
        self.machine_mut().breakpoints.insert(addr);
    }
    /// Removes `addr` from the breakpoint set. Returns `true` if it was present.
    fn remove_breakpoint(&mut self, addr: u16) -> bool {
        self.machine_mut().breakpoints.remove(&addr)
    }
    /// Clears all breakpoints.
    fn clear_breakpoints(&mut self) {
        self.machine_mut().breakpoints.clear();
    }
    /// Returns the current breakpoint set.
    fn breakpoints(&self) -> &HashSet<u16> {
        &self.machine().breakpoints
    }
    /// Returns the total number of clock cycles elapsed since the last reset.
    fn cycles(&self) -> u64 {
        self.machine().cycles
    }
    /// Returns the configured target clock speed.
    fn clock_speed(&self) -> ClockSpeed {
        self.machine().clock_speed
    }
    /// Returns `true` if the CPU is parked waiting for an interrupt.
    fn is_waiting(&self) -> bool {
        self.halt_state() == HaltState::Waiting
    }
    /// Returns `true` if the CPU is parked until a reset.
    fn is_stopped(&self) -> bool {
        self.halt_state() == HaltState::Stopped
    }
}
