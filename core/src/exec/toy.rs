//! A test-only toy [`Processor`], used to prove that `exec` runs a CPU it
//! knows nothing about. Nothing here refers to the 6502.
//!
//! The toy's instruction set:
//!
//! | Opcode | Mnemonic | Bytes | Cycles | Effect |
//! |---|---|---|---|---|
//! | `0x00` | NOP | 1 | 1 | nothing |
//! | `0x01` | CALL addr16 | 3 | 3 | push the return address (2 bytes), jump to `addr16` |
//! | `0x02` | RET | 1 | 3 | pop the return address |
//! | `0x03` | JMP addr16 | 3 | 2 | jump to `addr16` |
//! | `0xFE` | WAI | 1 | 1 | park until an interrupt or reset |
//! | `0xFF` | STP | 1 | 1 | park until a reset |
//!
//! Addresses are little-endian. The stack grows down from [`STACK_TOP`], and
//! the reset vector is at [`RESET_VECTOR`]. The toy wakes from WAI on an
//! interrupt but has no interrupt handlers: it just carries on executing.

use super::StepResult;
use crate::LogSender;
use crate::bus::vector::IdentityVectorResolver;
use crate::bus::{AddressRange, Bus, BusOp, CpuRequest};
use crate::error::ExecError;
use crate::exec::ClockSpeed;
use crate::machine::MachineState;
use crate::processor::{HaltState, Processor};
use crate::watch::{Operand, WatchContext};

pub(super) const NOP: u8 = 0x00;
pub(super) const CALL: u8 = 0x01;
pub(super) const RET: u8 = 0x02;
pub(super) const JMP: u8 = 0x03;
pub(super) const WAI: u8 = 0xFE;
pub(super) const STP: u8 = 0xFF;

/// Address the toy reads its 16-bit start address from on reset.
pub(super) const RESET_VECTOR: u16 = 0xFFF0;
/// Initial (empty) stack pointer; the first push writes `STACK_TOP - 1`.
pub(super) const STACK_TOP: u16 = 0x1000;
/// Length of the window a live snapshot captures, ending at `STACK_TOP`.
pub(super) const STACK_WINDOW_LEN: u16 = 0x0100;

const REG_PC: Operand = 0;
const REG_SP: Operand = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ToyRegisters {
    pub(super) pc: u16,
    pub(super) sp: u16,
}

pub(super) struct ToyCpu {
    regs: ToyRegisters,
    machine: MachineState,
    halt: HaltState,
}

impl ToyCpu {
    /// Builds a toy CPU over 64KB of zeroed RAM, with `program` at `start`
    /// and the reset vector pointing there, and resets it.
    pub(super) fn new(start: u16, program: &[u8], clock_speed: ClockSpeed) -> Self {
        let mut bus = Bus::config()
            .ram_with_fill(AddressRange::new(0x0000, 0xFFFF), 0)
            .unwrap()
            .build();
        for (i, &b) in program.iter().enumerate() {
            bus.write(start.wrapping_add(i as u16), b).unwrap();
        }
        let [lo, hi] = start.to_le_bytes();
        bus.write(RESET_VECTOR, lo).unwrap();
        bus.write(RESET_VECTOR + 1, hi).unwrap();
        let mut cpu = Self {
            regs: ToyRegisters {
                pc: 0,
                sp: STACK_TOP,
            },
            machine: MachineState::new(
                bus,
                clock_speed,
                Box::new(IdentityVectorResolver),
                LogSender::default(),
            ),
            halt: HaltState::Running,
        };
        cpu.reset().unwrap();
        cpu
    }

    fn read(&mut self, addr: u16) -> Result<u8, ExecError> {
        self.machine
            .bus
            .read(addr)
            .map_err(|_| ExecError::UnmappedAddress {
                addr,
                op: BusOp::Read,
            })
    }

    fn write(&mut self, addr: u16, value: u8) -> Result<(), ExecError> {
        self.machine
            .bus
            .write(addr, value)
            .map_err(|_| ExecError::UnmappedAddress {
                addr,
                op: BusOp::Write,
            })
    }

    fn read16(&mut self, addr: u16) -> Result<u16, ExecError> {
        let lo = self.read(addr)?;
        let hi = self.read(addr.wrapping_add(1))?;
        Ok(u16::from_le_bytes([lo, hi]))
    }

    fn push16(&mut self, value: u16) -> Result<(), ExecError> {
        let [lo, hi] = value.to_le_bytes();
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        self.write(self.regs.sp, hi)?;
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        self.write(self.regs.sp, lo)
    }

    fn pop16(&mut self) -> Result<u16, ExecError> {
        let lo = self.read(self.regs.sp)?;
        self.regs.sp = self.regs.sp.wrapping_add(1);
        let hi = self.read(self.regs.sp)?;
        self.regs.sp = self.regs.sp.wrapping_add(1);
        Ok(u16::from_le_bytes([lo, hi]))
    }

    /// Executes the instruction at PC; returns its cycle count.
    fn execute(&mut self, pc: u16, opcode: u8) -> Result<u8, ExecError> {
        match opcode {
            NOP => {
                self.regs.pc = pc.wrapping_add(1);
                Ok(1)
            }
            CALL => {
                let target = self.read16(pc.wrapping_add(1))?;
                self.push16(pc.wrapping_add(3))?;
                self.regs.pc = target;
                Ok(3)
            }
            RET => {
                self.regs.pc = self.pop16()?;
                Ok(3)
            }
            JMP => {
                self.regs.pc = self.read16(pc.wrapping_add(1))?;
                Ok(2)
            }
            WAI => {
                self.regs.pc = pc.wrapping_add(1);
                self.halt = HaltState::Waiting;
                Ok(1)
            }
            STP => {
                self.regs.pc = pc.wrapping_add(1);
                self.halt = HaltState::Stopped;
                Ok(1)
            }
            _ => Err(ExecError::InvalidOpcode { addr: pc, opcode }),
        }
    }
}

/// Watch context over the toy's registers (`PC`, `SP`) and the bus.
struct ToyWatchContext<'a> {
    regs: &'a ToyRegisters,
    bus: &'a Bus,
}

impl WatchContext for ToyWatchContext<'_> {
    fn read_register_u32(&self, id: Operand) -> Operand {
        match id {
            REG_PC => self.regs.pc as Operand,
            REG_SP => self.regs.sp as Operand,
            _ => 0,
        }
    }

    fn read_register_i32(&self, id: Operand) -> Operand {
        (self.read_register_u32(id) as u16 as i16) as u32
    }

    fn read_flag(&self, _flag_id: Operand) -> Operand {
        0
    }

    fn read_mem_u32(&self, addr: u16, width: u8) -> u32 {
        (0..width as u16).fold(0, |val, i| {
            val | (self.bus.peek(addr.wrapping_add(i)).unwrap_or(0) as u32) << (i * 8)
        })
    }

    fn read_mem_i32(&self, addr: u16, width: u8) -> u32 {
        self.read_mem_u32(addr, width)
    }
}

impl Processor for ToyCpu {
    type Registers = ToyRegisters;
    type Op = u8;
    type StackMark = u16;

    fn machine(&self) -> &MachineState {
        &self.machine
    }

    fn machine_mut(&mut self) -> &mut MachineState {
        &mut self.machine
    }

    fn step(&mut self, skip_pc: Option<u16>, check_breakpoints: bool) -> StepResult<u8> {
        match self.halt {
            HaltState::Stopped => {
                self.machine.tick_idle();
                if !self.machine.interrupts.request_pending() {
                    return StepResult::Stopped;
                }
            }
            HaltState::Waiting => {
                self.machine.tick_idle();
                if !self.machine.interrupts.request_pending()
                    && !self.machine.irq_recognized()
                    && !self.machine.interrupts.nmi_pending()
                {
                    return StepResult::Waiting;
                }
                self.halt = HaltState::Running;
            }
            HaltState::Running => {}
        }

        let pc = self.regs.pc;
        if check_breakpoints {
            let regs = &self.regs;
            if let Some(halt) = self
                .machine
                .check_halt(pc, skip_pc, |bus| ToyWatchContext { regs, bus })
            {
                return halt;
            }
        }

        match self.machine.interrupts.take_request() {
            None => {}
            Some(CpuRequest::Reset) => {
                return match self.reset() {
                    Ok(()) => StepResult::Reset,
                    Err(e) => StepResult::Error(e),
                };
            }
            Some(CpuRequest::Exit(status)) => {
                self.halt = HaltState::Stopped;
                self.machine.set_exit_status(Some(status));
                return StepResult::Stopped;
            }
        }

        let result = self
            .read(pc)
            .and_then(|opcode| Ok((opcode, self.execute(pc, opcode)?)));
        match result {
            Ok((opcode, cycles)) => {
                self.machine.finish_cycle(cycles);
                StepResult::Executed(opcode)
            }
            Err(e) => StepResult::Error(e),
        }
    }

    fn reset(&mut self) -> Result<(), ExecError> {
        self.machine.bus.reset_devices();
        self.machine.exit_status = None;
        let vector = self
            .machine
            .vector_resolver
            .resolve(RESET_VECTOR, &self.machine.interrupts);
        self.regs.pc = self.read16(vector)?;
        self.regs.sp = STACK_TOP;
        self.machine.cycles = 0;
        self.halt = HaltState::Running;
        Ok(())
    }

    fn registers(&self) -> &ToyRegisters {
        &self.regs
    }

    fn pc(&self) -> u16 {
        self.regs.pc
    }

    fn halt_state(&self) -> HaltState {
        self.halt
    }

    fn flush_trace(&mut self) {}

    fn subroutine_call_len(&self, pc: u16) -> Option<u16> {
        (self.machine.bus.peek(pc).unwrap_or(0) == CALL).then_some(3)
    }

    fn stack_mark(&self) -> u16 {
        self.regs.sp
    }

    fn has_returned_from(&self, mark: u16) -> bool {
        self.regs.sp > mark
    }

    fn stack_window(&self) -> (u16, u16) {
        (STACK_TOP - STACK_WINDOW_LEN, STACK_WINDOW_LEN)
    }

    fn map_register_name(name: &str) -> Option<Operand> {
        match name {
            "PC" | "pc" => Some(REG_PC),
            "SP" | "sp" => Some(REG_SP),
            _ => None,
        }
    }

    fn map_flag_name(_name: &str) -> Option<Operand> {
        None
    }
}

mod tests {
    use super::*;
    use crate::exec::{RunStopper, run, run_from, step_into, step_over_subroutine, step_return};
    use std::sync::Arc;
    use std::sync::atomic::AtomicU16;
    use std::time::{Duration, Instant};

    const START: u16 = 0x0200;
    const SUB: u16 = 0x0300;

    /// NOP, then JMP back to the NOP: an endless two-instruction loop.
    fn nop_loop() -> ToyCpu {
        let [lo, hi] = START.to_le_bytes();
        ToyCpu::new(START, &[NOP, JMP, lo, hi], ClockSpeed::unlimited())
    }

    /// `CALL SUB; NOP` at `START`, and `NOP; NOP; RET` at `SUB`.
    fn call_program() -> ToyCpu {
        let [lo, hi] = SUB.to_le_bytes();
        let mut cpu = ToyCpu::new(START, &[CALL, lo, hi, NOP], ClockSpeed::unlimited());
        for (i, &b) in [NOP, NOP, RET].iter().enumerate() {
            cpu.bus_mut().write(SUB + i as u16, b).unwrap();
        }
        cpu
    }

    #[tokio::test]
    async fn toy_throttled_run_takes_at_least_wall_time() {
        let [lo, hi] = START.to_le_bytes();
        let cpu = ToyCpu::new(START, &[NOP, JMP, lo, hi], ClockSpeed::mhz(2.0));
        let handle = run(cpu);
        let wall_start = Instant::now();
        tokio::time::sleep(Duration::from_millis(15)).await;
        let cpu = handle.take_cpu().await;
        let wall_elapsed = wall_start.elapsed();
        // 2 cycles per µs at 2 MHz, with 2× slack for CI jitter.
        let max_expected_cycles = wall_elapsed.as_micros() as u64 * 4;
        assert!(cpu.cycles() > 0);
        assert!(
            cpu.cycles() <= max_expected_cycles,
            "throttled toy CPU ran too fast: {} cycles in {wall_elapsed:?}",
            cpu.cycles()
        );
    }

    #[tokio::test]
    async fn toy_free_run_halts_at_breakpoint() {
        let mut cpu = nop_loop();
        cpu.add_breakpoint(START + 1);
        let result = run(cpu).wait().await;
        assert!(matches!(result, StepResult::Breakpoint(addr) if addr == START + 1));
    }

    #[tokio::test]
    async fn toy_stopped_run_reports_stop_requested() {
        let handle = run(nop_loop());
        let stopper = handle.stopper();
        tokio::time::sleep(Duration::from_millis(5)).await;
        stopper.stop();
        let (result, cpu) = handle.take_cpu_with_result().await;
        assert!(matches!(result, StepResult::StopRequested));
        assert!(cpu.cycles() > 0);
    }

    #[tokio::test]
    async fn toy_live_snapshot_uses_its_own_stack_window() {
        let mut cpu = nop_loop();
        cpu.bus_mut().write(STACK_TOP - 1, 0x5A).unwrap();
        let handle = run(cpu);
        tokio::time::sleep(Duration::from_millis(5)).await;
        let snapshot = handle
            .live_snapshot()
            .expect("expected a published snapshot");
        assert_eq!(snapshot.stack_base, STACK_TOP - STACK_WINDOW_LEN);
        assert_eq!(snapshot.stack.len(), STACK_WINDOW_LEN as usize);
        assert_eq!(snapshot.stack[STACK_WINDOW_LEN as usize - 1], 0x5A);
        assert!(snapshot.registers.pc == START || snapshot.registers.pc == START + 1);
        handle.take_cpu().await;
    }

    #[test]
    fn toy_step_over_call_runs_the_whole_subroutine() {
        let mut cpu = call_program();
        let (_stopper, stop_rx, mut cmd_rx) = RunStopper::channel();
        let result = step_over_subroutine(
            &mut cpu,
            &stop_rx,
            &mut cmd_rx,
            None,
            &Arc::new(AtomicU16::new(0)),
        );
        assert!(matches!(result, Some(StepResult::Executed(RET))));
        assert_eq!(cpu.pc(), START + 3);
        assert_eq!(cpu.registers().sp, STACK_TOP);
        // CALL (3) + NOP (1) + NOP (1) + RET (3).
        assert_eq!(cpu.cycles(), 8);
        // The temporary breakpoint is gone.
        assert!(cpu.breakpoints().is_empty());
    }

    #[test]
    fn toy_step_return_halts_after_ret() {
        let mut cpu = call_program();
        assert!(matches!(step_into(&mut cpu), StepResult::Executed(CALL)));
        assert_eq!(cpu.pc(), SUB);
        let (_stopper, stop_rx, mut cmd_rx) = RunStopper::channel();
        let result = step_return(
            &mut cpu,
            &stop_rx,
            &mut cmd_rx,
            None,
            &Arc::new(AtomicU16::new(0)),
        );
        assert!(matches!(result, Some(StepResult::Executed(RET))));
        assert_eq!(cpu.pc(), START + 3);
        assert_eq!(cpu.registers().sp, STACK_TOP);
    }

    #[tokio::test]
    async fn toy_run_halts_on_stp_by_default() {
        let cpu = ToyCpu::new(START, &[STP], ClockSpeed::unlimited());
        let (result, cpu) = run(cpu).take_cpu_with_result().await;
        assert!(matches!(result, StepResult::Stopped));
        assert!(cpu.is_stopped());
    }

    #[tokio::test]
    async fn toy_parked_run_survives_stp_until_reset() {
        let cpu = ToyCpu::new(START, &[STP], ClockSpeed::unlimited());
        let handle = run_from(cpu, None, Arc::new(AtomicU16::new(0)), Vec::new(), true);
        let stopper = handle.stopper();
        tokio::time::sleep(Duration::from_millis(10)).await;
        stopper.trigger_reset();
        let (result, cpu) = handle.take_cpu_with_result().await;
        assert!(matches!(result, StepResult::Reset));
        assert!(!cpu.is_stopped());
        assert_eq!(cpu.pc(), START);
    }

    #[tokio::test]
    async fn toy_parked_run_wakes_from_wai_on_reset() {
        let cpu = ToyCpu::new(START, &[WAI], ClockSpeed::unlimited());
        let handle = run_from(cpu, None, Arc::new(AtomicU16::new(0)), Vec::new(), true);
        let stopper = handle.stopper();
        tokio::time::sleep(Duration::from_millis(10)).await;
        stopper.trigger_reset();
        let (result, cpu) = handle.take_cpu_with_result().await;
        assert!(matches!(result, StepResult::Reset));
        assert!(!cpu.is_waiting());
    }
}
