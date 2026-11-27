//! The emulated Motorola 6809: registers, instruction fetch, decode and
//! execution, and its [`Processor`] implementation.
//!
//! The CPU builds only on `emma6k-core`'s public API: it embeds a
//! [`MachineState`] and uses its per-step helpers (`check_halt`,
//! `finish_cycle`, `tick_idle`), as every CPU does. It adds an instruction's
//! cycles in one go after executing it, not per bus cycle.
//!
//! **Interrupts.** Before each instruction the CPU services, in priority
//! order, a pending reset, then NMI, then FIRQ (unless CC's F flag masks it),
//! then IRQ (unless I masks it). Devices reach FIRQ through the configuration
//! framework's `interrupt = "firq"` routing.
//!
//! - **NMI** is edge-triggered and stays disarmed after a reset until a program
//!   first loads S (LDS, LEAS, TFR or EXG into S, or PULU S). An NMI that
//!   arrives while disarmed stays pending until then. Service sets E, stacks
//!   the entire state, sets I and F, and takes 19 cycles.
//! - **FIRQ** clears E, stacks only PC and CC, sets I and F, and takes 10
//!   cycles.
//! - **IRQ** sets E, stacks the entire state, sets I, and takes 19 cycles.
//!
//! CWAI stacks the entire state, then waits for an unmasked interrupt (or a
//! reset). Servicing it then skips the stacking and takes
//! [`CWAI_WAKE_CYCLES`], keeping E set so RTI restores everything, even for
//! FIRQ. SYNC waits for any interrupt line, masked or not: an unmasked one is
//! serviced as usual, and a masked one just ends the wait. The vectors are
//! read through the machine's vector resolver.

pub mod cc;
mod execute;
pub mod indexed;
pub mod opcodes;
pub mod trace;

use crate::config::CpuVariantSpec;
use cc::ConditionCodes;
use emma6k_core::bus::vector::{IdentityVectorResolver, VectorResolver};
use emma6k_core::bus::{Bus, BusOp};
use emma6k_core::error::{BusError, CpuBuildError, ExecError};
use emma6k_core::exec::{self, ClockSpeed};
use emma6k_core::trace::TraceState;
use emma6k_core::watch::{Operand, WatchContext};
use emma6k_core::{HaltState, MachineState, Processor};
use emma6k_core::{LogCategory, LogLevel, LogSender, log_msg};
use opcodes::{DecodedOp, Page};
use trace::{TraceCallback, TraceKind, TraceRecord};

/// Bus address of the RESET vector.
pub const RESET_VECTOR: u16 = 0xFFFE;
/// Bus address of the NMI vector.
pub const NMI_VECTOR: u16 = 0xFFFC;
/// Bus address of the SWI vector.
pub const SWI_VECTOR: u16 = 0xFFFA;
/// Bus address of the IRQ vector.
pub const IRQ_VECTOR: u16 = 0xFFF8;
/// Bus address of the FIRQ vector.
pub const FIRQ_VECTOR: u16 = 0xFFF6;
/// Bus address of the SWI2 vector.
pub const SWI2_VECTOR: u16 = 0xFFF4;
/// Bus address of the SWI3 vector.
pub const SWI3_VECTOR: u16 = 0xFFF2;

/// Cycles to service an interrupt that ends a CWAI wait. CWAI has already
/// stacked the state, so this is the interrupt sequence less its stacking.
pub const CWAI_WAKE_CYCLES: u8 = 7;
/// Cycles to service an NMI or IRQ: stacking the entire state and fetching
/// the vector.
pub const ENTIRE_INTERRUPT_CYCLES: u8 = 19;
/// Cycles to service a FIRQ: stacking PC and CC and fetching the vector.
pub const FAST_INTERRUPT_CYCLES: u8 = 10;

/// Bytes below `S` that a live snapshot's stack window starts at.
const STACK_WINDOW_BELOW: u16 = 32;
/// Bytes from `S` upward that a live snapshot's stack window covers.
const STACK_WINDOW_ABOVE: u32 = 96;

/// The 6809's registers. D is A (high byte) and B (low byte) together; see
/// [`d`](Registers::d) and [`set_d`](Registers::set_d).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registers {
    /// Accumulator A, the high byte of D.
    pub a: u8,
    /// Accumulator B, the low byte of D.
    pub b: u8,
    /// Direct page register: the high byte of a direct-mode address.
    pub dp: u8,
    /// Condition codes.
    pub cc: ConditionCodes,
    /// Index register X.
    pub x: u16,
    /// Index register Y.
    pub y: u16,
    /// User stack pointer U.
    pub u: u16,
    /// Hardware stack pointer S.
    pub s: u16,
    /// Program counter.
    pub pc: u16,
}

impl Registers {
    fn new() -> Self {
        Self {
            a: 0,
            b: 0,
            dp: 0,
            cc: ConditionCodes::I | ConditionCodes::F,
            x: 0,
            y: 0,
            u: 0,
            s: 0,
            pc: 0,
        }
    }

    /// Returns D: A in the high byte, B in the low byte.
    #[inline]
    pub fn d(&self) -> u16 {
        u16::from_be_bytes([self.a, self.b])
    }

    /// Sets D: A gets the high byte, B the low byte.
    #[inline]
    pub fn set_d(&mut self, value: u16) {
        [self.a, self.b] = value.to_be_bytes();
    }
}

/// Controls how undocumented opcodes, and undefined indexed postbytes, are
/// handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidOpcodePolicy {
    /// Skip the instruction: its prefix and opcode bytes, plus the postbyte
    /// when the postbyte is the undefined part. Takes
    /// [`INVALID_OPCODE_CYCLES`](opcodes::INVALID_OPCODE_CYCLES) for an
    /// undocumented opcode, or the instruction's base cycles for an undefined
    /// postbyte.
    Nop,
    /// Return [`ExecError::InvalidOpcode`], leaving PC at the instruction.
    Error,
}

/// Result returned by [`Processor::step`]: [`exec::StepResult`] for the
/// 6809's [`DecodedOp`].
pub type StepResult = exec::StepResult<DecodedOp>;

/// [`exec::RunHandle`] for the 6809 [`Cpu`].
pub type RunHandle = exec::RunHandle<Cpu>;

/// [`exec::CpuLiveSnapshot`] for the 6809's [`Registers`].
pub type CpuLiveSnapshot = exec::CpuLiveSnapshot<Registers>;

/// The 6809 CPU: registers, machine state, and execution state.
pub struct Cpu {
    /// The programmer-visible registers.
    regs: Registers,
    /// Machine state shared with every CPU: bus, interrupt controller, watch
    /// evaluator, breakpoints, clock speed, cycle count, vector resolver, and
    /// log sender.
    machine: MachineState,
    /// Governs how undocumented opcodes and undefined postbytes are handled.
    invalid_opcode_policy: InvalidOpcodePolicy,
    /// Whether the CPU is parked by CWAI or SYNC.
    wait: Wait,
    /// Whether NMI is armed: false from reset until a program first loads S.
    nmi_armed: bool,
    /// Address of the instruction being executed: its prefix or opcode byte.
    instr_pc: u16,
    /// Whether a trace callback is installed.
    tracing: bool,
    /// The current step's trace id and not-yet-emitted register snapshot.
    trace_state: TraceState<Registers>,
    /// Receives a record for every bus read and write (not `peek`), each
    /// step's registers, and its cycles.
    trace_callback: Option<Box<dyn TraceCallback<Registers>>>,
}

/// What, if anything, the CPU is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wait {
    /// Running.
    None,
    /// After CWAI: the entire state is stacked; an unmasked interrupt (or a
    /// reset) ends the wait.
    Cwai,
    /// After SYNC: any interrupt line, masked or not (or a reset), ends the
    /// wait.
    Sync,
}

/// A hardware interrupt the CPU can service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Interrupt {
    Nmi,
    Firq,
    Irq,
}

impl Cpu {
    /// Returns a `CpuBuilder` for constructing a `Cpu`.
    pub fn builder() -> CpuBuilder {
        CpuBuilder::new()
    }

    /// Returns the registers, mutably.
    pub fn registers_mut(&mut self) -> &mut Registers {
        &mut self.regs
    }

    /// Returns the invalid-opcode policy.
    pub fn invalid_opcode_policy(&self) -> InvalidOpcodePolicy {
        self.invalid_opcode_policy
    }

    /// Reads the reset vector (through the installed [`VectorResolver`]),
    /// sets DP to 0, sets the I and F masks, disarms NMI, and clears any
    /// wait.
    pub fn reset(&mut self) -> Result<(), ExecError> {
        self.machine.bus_mut().reset_devices();
        let vector = self.machine.resolve_vector(RESET_VECTOR);
        self.regs.pc = self.read16::<true>(vector)?;
        self.regs.dp = 0;
        self.regs.cc.insert(ConditionCodes::I | ConditionCodes::F);
        self.machine.reset_cycles();
        self.wait = Wait::None;
        self.nmi_armed = false;
        log_msg!(
            self.machine.log_sender(),
            LogLevel::Info,
            LogCategory::Cpu,
            "6809 CPU reset"
        );
        Ok(())
    }

    /// The CPU variant: always the MC6809, until the 6309 exists.
    pub fn variant(&self) -> CpuVariantSpec {
        CpuVariantSpec::Mc6809
    }

    /// Installs a trace callback, or removes one with `None`. While one is
    /// installed, each step records its starting registers, every bus read
    /// and write (never `peek`), and its cycles.
    pub fn set_trace_callback(&mut self, callback: Option<Box<dyn TraceCallback<Registers>>>) {
        self.tracing = callback.is_some();
        self.trace_callback = callback;
    }

    /// Flushes the trace callback, if one is installed, making every record
    /// emitted so far visible to an independent reader of the trace stream.
    pub fn flush_trace(&mut self) {
        if let Some(cb) = &mut self.trace_callback {
            cb.flush();
        }
    }

    /// Fetches, decodes and executes one instruction, or services a pending
    /// reset or interrupt. Skips the breakpoint and watch check at `skip_pc`,
    /// and everywhere if `check_breakpoints` is `false`.
    pub fn step(&mut self, skip_pc: Option<u16>, check_breakpoints: bool) -> StepResult {
        // Two compiled copies of the step: an untraced one with no trace
        // checks at all on the bus path, and a traced one.
        if self.tracing {
            self.step_traced(skip_pc, check_breakpoints)
        } else {
            self.step_with::<false>(skip_pc, check_breakpoints)
        }
    }

    /// The traced step, kept out of line so the untraced one stays lean.
    #[cold]
    #[inline(never)]
    fn step_traced(&mut self, skip_pc: Option<u16>, check_breakpoints: bool) -> StepResult {
        self.step_with::<true>(skip_pc, check_breakpoints)
    }

    /// [`step`](Self::step), recording a trace if `TRACE`.
    #[inline(always)]
    fn step_with<const TRACE: bool>(
        &mut self,
        skip_pc: Option<u16>,
        check_breakpoints: bool,
    ) -> StepResult {
        match self.wait {
            Wait::None => {}
            Wait::Cwai => {
                // Keep devices running while parked, so one can interrupt.
                self.machine.tick_idle();
                if !self.machine.interrupts().reset_pending() && self.pending_interrupt().is_none()
                {
                    return StepResult::Waiting;
                }
            }
            Wait::Sync => {
                self.machine.tick_idle();
                if !self.machine.interrupts().reset_pending() && !self.interrupt_line_asserted() {
                    return StepResult::Waiting;
                }
                // Any interrupt line ends SYNC. An unmasked one is serviced
                // below; a masked one isn't, and execution continues.
                self.wait = Wait::None;
            }
        }

        if TRACE {
            self.trace_state.begin_instruction(self.regs);
        }

        let start = self.regs.pc;
        if check_breakpoints {
            let regs = &self.regs;
            if let Some(halt) = self
                .machine
                .check_halt(start, skip_pc, |bus| CpuWatchContext { regs, bus })
            {
                return halt;
            }
        }

        if self.machine.interrupts_mut().take_reset() {
            return match self.reset() {
                Ok(()) => StepResult::Reset,
                Err(e) => StepResult::Error(e),
            };
        }

        // One `finish_cycle` call for every path keeps it inlined here.
        match self.service_or_execute::<TRACE>(start) {
            Ok((cycles, op)) => {
                self.machine.finish_cycle(cycles);
                if TRACE {
                    self.emit_trace(TraceKind::Cycles(cycles));
                }
                StepResult::Executed(op)
            }
            Err(e) => StepResult::Error(e),
        }
    }

    /// Services a pending unmasked interrupt, or fetches, decodes and
    /// executes the instruction at `start`, applying the invalid-opcode
    /// policy. Returns the cycles taken and the step's op. On an error, PC is
    /// left at `start`.
    #[inline]
    fn service_or_execute<const TRACE: bool>(
        &mut self,
        start: u16,
    ) -> Result<(u8, DecodedOp), ExecError> {
        if let Some(interrupt) = self.pending_interrupt() {
            let cycles = self.service_interrupt::<TRACE>(interrupt)?;
            return Ok((cycles, DecodedOp::interrupt(cycles)));
        }

        let first = self.read8::<TRACE>(start)?;
        let decoded = match first {
            0x10 | 0x11 => {
                let second = self.read8::<TRACE>(start.wrapping_add(1))?;
                let page = if first == 0x10 {
                    Page::Page2
                } else {
                    Page::Page3
                };
                DecodedOp::new(page, second)
            }
            _ => DecodedOp::new(Page::Page1, first),
        };
        let opcode_len = decoded.page.prefix_len() + 1;
        let invalid = || ExecError::InvalidOpcode {
            addr: start,
            opcode: decoded.opcode,
        };

        if decoded.is_invalid() {
            return match self.invalid_opcode_policy {
                InvalidOpcodePolicy::Nop => {
                    self.regs.pc = start.wrapping_add(opcode_len);
                    Ok((opcodes::INVALID_OPCODE_CYCLES, decoded))
                }
                InvalidOpcodePolicy::Error => Err(invalid()),
            };
        }

        self.instr_pc = start;
        self.regs.pc = start.wrapping_add(opcode_len);
        match self.execute::<TRACE>(decoded) {
            Ok(cycles) => Ok((cycles, decoded)),
            // `execute` reports an undefined indexed postbyte as an invalid
            // opcode, before the instruction has changed anything but PC.
            Err(ExecError::InvalidOpcode { .. })
                if self.invalid_opcode_policy == InvalidOpcodePolicy::Nop =>
            {
                self.regs.pc = start.wrapping_add(decoded.base_len());
                Ok((decoded.base_cycles, decoded))
            }
            Err(ExecError::InvalidOpcode { .. }) => {
                self.regs.pc = start;
                Err(invalid())
            }
            Err(e) => {
                self.regs.pc = start;
                Err(e)
            }
        }
    }

    /// Returns the highest-priority interrupt that is pending and not
    /// masked: an armed NMI, then FIRQ unless F is set, then IRQ unless I is
    /// set.
    #[inline]
    fn pending_interrupt(&self) -> Option<Interrupt> {
        let interrupts = self.machine.interrupts();
        if self.nmi_armed && interrupts.nmi_pending() {
            Some(Interrupt::Nmi)
        } else if !self.regs.cc.contains(ConditionCodes::F) && interrupts.firq_active() {
            Some(Interrupt::Firq)
        } else if !self.regs.cc.contains(ConditionCodes::I) && self.machine.irq_recognized() {
            Some(Interrupt::Irq)
        } else {
            None
        }
    }

    /// Returns `true` if any interrupt line is asserted, masked or not: what
    /// ends a SYNC. A disarmed NMI doesn't count.
    fn interrupt_line_asserted(&self) -> bool {
        let interrupts = self.machine.interrupts();
        (self.nmi_armed && interrupts.nmi_pending())
            || interrupts.firq_active()
            || self.machine.irq_recognized()
    }

    // --- bus helpers ---

    #[inline(always)]
    fn read8<const TRACE: bool>(&mut self, addr: u16) -> Result<u8, ExecError> {
        let result = self.machine.bus_mut().read(addr).map_err(|e| match e {
            BusError::Unmapped { addr } | BusError::RomWrite { addr } => {
                ExecError::UnmappedAddress {
                    addr,
                    op: BusOp::Read,
                }
            }
        });
        if TRACE && let Ok(value) = result {
            self.emit_trace(TraceKind::Read { addr, value });
        }
        result
    }

    #[inline(always)]
    fn write8<const TRACE: bool>(&mut self, addr: u16, value: u8) -> Result<(), ExecError> {
        let result = self
            .machine
            .bus_mut()
            .write(addr, value)
            .map_err(|e| match e {
                BusError::Unmapped { addr } => ExecError::UnmappedAddress {
                    addr,
                    op: BusOp::Write,
                },
                BusError::RomWrite { addr } => ExecError::RomWrite { addr, value },
            });
        if TRACE && result.is_ok() {
            self.emit_trace(TraceKind::Write { addr, value });
        }
        result
    }

    /// Records `kind` for the current step, preceded by the step's
    /// `Registers` record if this is its first record. Kept out of line, so
    /// the untraced bus helpers stay small.
    #[cold]
    #[inline(never)]
    fn emit_trace(&mut self, kind: TraceKind) {
        let Some(cb) = &mut self.trace_callback else {
            return;
        };
        let instr_id = self.trace_state.current_instr_id();
        if let Some(regs) = self.trace_state.take_pending_registers(instr_id) {
            cb.record(TraceRecord {
                instr_id,
                kind: TraceKind::Registers(regs),
            });
        }
        cb.record(TraceRecord { instr_id, kind });
    }

    /// Reads a big-endian word.
    #[inline]
    fn read16<const TRACE: bool>(&mut self, addr: u16) -> Result<u16, ExecError> {
        let hi = self.read8::<TRACE>(addr)?;
        let lo = self.read8::<TRACE>(addr.wrapping_add(1))?;
        Ok(u16::from_be_bytes([hi, lo]))
    }

    /// Writes a big-endian word.
    #[inline]
    fn write16<const TRACE: bool>(&mut self, addr: u16, value: u16) -> Result<(), ExecError> {
        let [hi, lo] = value.to_be_bytes();
        self.write8::<TRACE>(addr, hi)?;
        self.write8::<TRACE>(addr.wrapping_add(1), lo)
    }

    /// Reads the byte at PC and advances PC.
    #[inline]
    fn fetch8<const TRACE: bool>(&mut self) -> Result<u8, ExecError> {
        let value = self.read8::<TRACE>(self.regs.pc)?;
        self.regs.pc = self.regs.pc.wrapping_add(1);
        Ok(value)
    }

    /// Reads the big-endian word at PC and advances PC past it.
    #[inline]
    fn fetch16<const TRACE: bool>(&mut self) -> Result<u16, ExecError> {
        let value = self.read16::<TRACE>(self.regs.pc)?;
        self.regs.pc = self.regs.pc.wrapping_add(2);
        Ok(value)
    }
}

/// Returns the length in bytes of the subroutine call at `pc` (BSR, LBSR or
/// JSR), or `None` if the instruction there isn't one. Reads with `peek`, so
/// it has no side effects.
fn subroutine_call_len(bus: &Bus, pc: u16) -> Option<u16> {
    let peek = |addr: u16| bus.peek(addr).unwrap_or(0);
    match peek(pc) {
        0x8D | 0x9D => Some(2), // BSR, JSR direct
        0x17 | 0xBD => Some(3), // LBSR, JSR extended
        0xAD => {
            // JSR indexed: opcode, postbyte, and the postbyte's offset bytes.
            Some(2 + indexed::extra_bytes(peek(pc.wrapping_add(1))))
        }
        _ => None,
    }
}

// Register IDs used by `CpuWatchContext` and returned by `map_register_name`.
const REG_A: Operand = 0;
const REG_B: Operand = 1;
const REG_D: Operand = 2;
const REG_X: Operand = 3;
const REG_Y: Operand = 4;
const REG_U: Operand = 5;
const REG_S: Operand = 6;
const REG_PC: Operand = 7;
const REG_DP: Operand = 8;
const REG_CC: Operand = 9;

/// Maps a register name to its `Operand` ID for use with `WatchCompiler`.
///
/// Accepts `A`, `B`, `D`, `X`, `Y`, `U`, `S`, `PC`, `DP` and `CC`, in any
/// case. Returns `None` for any other name.
pub fn map_register_name(name: &str) -> Option<Operand> {
    match name.to_ascii_uppercase().as_str() {
        "A" => Some(REG_A),
        "B" => Some(REG_B),
        "D" => Some(REG_D),
        "X" => Some(REG_X),
        "Y" => Some(REG_Y),
        "U" => Some(REG_U),
        "S" => Some(REG_S),
        "PC" => Some(REG_PC),
        "DP" => Some(REG_DP),
        "CC" => Some(REG_CC),
        _ => None,
    }
}

/// Maps a flag name to its `Operand` ID (its bit mask in CC) for use with
/// `WatchCompiler`.
///
/// Accepts `E`, `F`, `H`, `I`, `N`, `Z`, `V` and `C`, in any case. Returns
/// `None` for any other name.
pub fn map_flag_name(name: &str) -> Option<Operand> {
    let flag = match name.to_ascii_uppercase().as_str() {
        "E" => ConditionCodes::E,
        "F" => ConditionCodes::F,
        "H" => ConditionCodes::H,
        "I" => ConditionCodes::I,
        "N" => ConditionCodes::N,
        "Z" => ConditionCodes::Z,
        "V" => ConditionCodes::V,
        "C" => ConditionCodes::C,
        _ => return None,
    };
    Some(flag.bits() as Operand)
}

/// Borrows CPU state to implement `WatchContext`, with side-effect-free,
/// big-endian memory reads.
struct CpuWatchContext<'a> {
    regs: &'a Registers,
    bus: &'a Bus,
}

impl CpuWatchContext<'_> {
    /// Returns a register's value and its width in bits.
    fn register(&self, id: Operand) -> (u32, u32) {
        let r = self.regs;
        match id {
            REG_A => (r.a as u32, 8),
            REG_B => (r.b as u32, 8),
            REG_D => (r.d() as u32, 16),
            REG_X => (r.x as u32, 16),
            REG_Y => (r.y as u32, 16),
            REG_U => (r.u as u32, 16),
            REG_S => (r.s as u32, 16),
            REG_PC => (r.pc as u32, 16),
            REG_DP => (r.dp as u32, 8),
            REG_CC => (r.cc.to_byte() as u32, 8),
            _ => (0, 8),
        }
    }
}

/// Sign-extends the low `bits` bits of `value` to 32 bits.
fn sign_extend(value: u32, bits: u32) -> u32 {
    let shift = 32 - bits;
    (((value << shift) as i32) >> shift) as u32
}

impl WatchContext for CpuWatchContext<'_> {
    fn read_register_u32(&self, id: Operand) -> Operand {
        self.register(id).0
    }

    fn read_register_i32(&self, id: Operand) -> Operand {
        let (value, bits) = self.register(id);
        sign_extend(value, bits)
    }

    fn read_flag(&self, flag_id: Operand) -> Operand {
        (self.regs.cc.to_byte() as Operand & flag_id != 0) as Operand
    }

    fn read_mem_u32(&self, addr: u16, width: u8) -> u32 {
        match width {
            1 | 2 | 4 => (0..width as u16).fold(0, |value, i| {
                (value << 8) | self.bus.peek(addr.wrapping_add(i)).unwrap_or(0) as u32
            }),
            _ => 0,
        }
    }

    fn read_mem_i32(&self, addr: u16, width: u8) -> u32 {
        match width {
            1 | 2 | 4 => sign_extend(self.read_mem_u32(addr, width), width as u32 * 8),
            _ => 0,
        }
    }
}

impl Processor for Cpu {
    type Registers = Registers;
    type Op = DecodedOp;
    /// The hardware stack pointer `S`.
    type StackMark = u16;

    fn machine(&self) -> &MachineState {
        &self.machine
    }

    fn machine_mut(&mut self) -> &mut MachineState {
        &mut self.machine
    }

    #[inline]
    fn step(&mut self, skip_pc: Option<u16>, check_breakpoints: bool) -> StepResult {
        Cpu::step(self, skip_pc, check_breakpoints)
    }

    fn reset(&mut self) -> Result<(), ExecError> {
        Cpu::reset(self)
    }

    fn registers(&self) -> &Registers {
        &self.regs
    }

    fn pc(&self) -> u16 {
        self.regs.pc
    }

    /// `Waiting` after CWAI or SYNC. The 6809 has no STP, so it is never
    /// `Stopped`.
    fn halt_state(&self) -> HaltState {
        match self.wait {
            Wait::None => HaltState::Running,
            Wait::Cwai | Wait::Sync => HaltState::Waiting,
        }
    }

    fn flush_trace(&mut self) {
        Cpu::flush_trace(self)
    }

    fn subroutine_call_len(&self, pc: u16) -> Option<u16> {
        subroutine_call_len(self.machine.bus(), pc)
    }

    fn stack_mark(&self) -> u16 {
        self.regs.s
    }

    /// `S` has risen above `mark`; the stack grows down. The wrapping 16-bit
    /// comparison tolerates the stack pointer wrapping around the address
    /// space.
    fn has_returned_from(&self, mark: u16) -> bool {
        (self.regs.s.wrapping_sub(mark) as i16) > 0
    }

    /// From 32 bytes below `S` to 96 bytes above it, clamped to the address
    /// space.
    fn stack_window(&self) -> (u16, u16) {
        let base = self.regs.s.saturating_sub(STACK_WINDOW_BELOW);
        let end = (self.regs.s as u32 + STACK_WINDOW_ABOVE).min(0x1_0000);
        (base, (end - base as u32) as u16)
    }

    fn map_register_name(name: &str) -> Option<Operand> {
        map_register_name(name)
    }

    fn map_flag_name(name: &str) -> Option<Operand> {
        map_flag_name(name)
    }
}

/// Builder for [`Cpu`].
pub struct CpuBuilder {
    invalid_opcode_policy: InvalidOpcodePolicy,
    clock_speed: ClockSpeed,
    bus: Option<Bus>,
    vector_resolver: Option<Box<dyn VectorResolver>>,
    log_sender: LogSender,
    machine: Option<MachineState>,
}

impl Default for CpuBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuBuilder {
    /// Creates a builder with the [`Nop`](InvalidOpcodePolicy::Nop) policy
    /// and an unlimited clock speed.
    pub fn new() -> Self {
        Self {
            invalid_opcode_policy: InvalidOpcodePolicy::Nop,
            clock_speed: ClockSpeed::unlimited(),
            bus: None,
            vector_resolver: None,
            log_sender: LogSender::default(),
            machine: None,
        }
    }

    /// Sets the invalid-opcode handling policy.
    pub fn invalid_opcode_policy(mut self, policy: InvalidOpcodePolicy) -> Self {
        self.invalid_opcode_policy = policy;
        self
    }

    /// Sets the target clock speed.
    pub fn clock_speed(mut self, speed: ClockSpeed) -> Self {
        self.clock_speed = speed;
        self
    }

    /// Provides the memory bus.
    pub fn bus(mut self, bus: Bus) -> Self {
        self.bus = Some(bus);
        self
    }

    /// Installs a custom interrupt-vector resolver, consulted whenever the
    /// CPU fetches a vector. If not called, `build()` installs
    /// [`IdentityVectorResolver`].
    pub fn vector_resolver(mut self, resolver: Box<dyn VectorResolver>) -> Self {
        self.vector_resolver = Some(resolver);
        self
    }

    /// Installs a sender for structured diagnostic messages. If not called,
    /// `build()` installs a `log`-crate-backed default.
    pub fn log_sender(mut self, sender: LogSender) -> Self {
        self.log_sender = sender;
        self
    }

    /// Builds the `Cpu` around existing machine state (for example from
    /// [`build_machine`](emma6k_core::config::build_machine)) instead of a
    /// bare bus. The machine state already holds the bus, clock speed, vector
    /// resolver and log sender, so values given to [`bus`](Self::bus),
    /// [`clock_speed`](Self::clock_speed),
    /// [`vector_resolver`](Self::vector_resolver) or
    /// [`log_sender`](Self::log_sender) are ignored.
    pub fn machine(mut self, machine: MachineState) -> Self {
        self.machine = Some(machine);
        self
    }

    /// Consumes the builder and returns a `Cpu`, or an error if no bus or
    /// machine state was given. The CPU isn't reset: call
    /// [`reset`](Cpu::reset) to load PC from the reset vector.
    pub fn build(self) -> Result<Cpu, CpuBuildError> {
        let machine = match self.machine {
            Some(machine) => machine,
            None => MachineState::new(
                self.bus.ok_or(CpuBuildError::NoBus)?,
                self.clock_speed,
                self.vector_resolver
                    .unwrap_or_else(|| Box::new(IdentityVectorResolver)),
                self.log_sender,
            ),
        };
        Ok(Cpu {
            regs: Registers::new(),
            machine,
            invalid_opcode_policy: self.invalid_opcode_policy,
            wait: Wait::None,
            nmi_armed: false,
            instr_pc: 0,
            tracing: false,
            trace_state: TraceState::new(),
            trace_callback: None,
        })
    }
}

#[cfg(test)]
mod tests;
