//! Instruction execution: addressing, the ALU and condition codes, the
//! stacks, and the per-instruction semantics.
//!
//! Flags the datasheet calls undefined are left unchanged: H after SUB, SBC,
//! CMP and NEG, and V after DAA. A transfer between registers of different
//! sizes (TFR, EXG) gives an 8-bit destination the source's low byte and a
//! 16-bit destination `$FF` in its high byte. The undefined register codes
//! (6, 7, $C–$F) read as all ones and ignore writes.

use super::cc::ConditionCodes;
use super::opcodes::{AddressingMode, DecodedOp, Mnemonic};
use super::{Cpu, SWI_VECTOR, SWI2_VECTOR, SWI3_VECTOR, indexed};
use emma6k_core::HaltState;
use emma6k_core::error::ExecError;

const C: u8 = ConditionCodes::C.bits();
const V: u8 = ConditionCodes::V.bits();
const Z: u8 = ConditionCodes::Z.bits();
const N: u8 = ConditionCodes::N.bits();
const H: u8 = ConditionCodes::H.bits();

/// PSH/PUL postbyte bits, in push order (PC first) from bit 7 down.
const PUSH_PC: u8 = 0x80;
/// U for PSHS/PULS, S for PSHU/PULU.
const PUSH_OTHER_STACK: u8 = 0x40;
const PUSH_Y: u8 = 0x20;
const PUSH_X: u8 = 0x10;
const PUSH_DP: u8 = 0x08;
const PUSH_B: u8 = 0x04;
const PUSH_A: u8 = 0x02;
const PUSH_CC: u8 = 0x01;
/// Every register: an interrupt's or SWI's entire-state push.
const PUSH_ALL: u8 = 0xFF;

/// N and Z for an 8-bit result.
#[inline]
fn nz8(r: u8) -> u8 {
    (((r == 0) as u8) << 2) | ((r >> 7) << 3)
}

/// N and Z for a 16-bit result.
#[inline]
fn nz16(r: u16) -> u8 {
    (((r == 0) as u8) << 2) | (((r >> 15) as u8) << 3)
}

/// Which stack a push or pull uses.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stack {
    S,
    U,
}

impl Cpu {
    /// Executes `op`, whose prefix and opcode bytes have been fetched (PC is
    /// past them). Returns the instruction's total cycles.
    pub(super) fn execute(&mut self, op: DecodedOp) -> Result<u8, ExecError> {
        use Mnemonic::*;
        let mode = op.mode;
        let mut cycles = op.base_cycles;
        match op.mnemonic {
            Invalid => unreachable!("step() handles undocumented opcodes"),

            // Memory read-modify-write. The opcode's low nibble selects the
            // operation, as in the accumulator forms. CLR reads before
            // writing, as the 6809 does.
            Neg | Com | Lsr | Ror | Asr | Asl | Rol | Dec | Inc | Clr => {
                let (ea, extra) = self.effective_address(mode)?;
                cycles += extra;
                let value = self.read8(ea)?;
                let result = self.unary(op.opcode & 0x0F, value);
                self.write8(ea, result)?;
            }
            Tst => {
                let (ea, extra) = self.effective_address(mode)?;
                cycles += extra;
                let value = self.read8(ea)?;
                self.unary(op.opcode & 0x0F, value);
            }
            Jmp => {
                let (ea, extra) = self.effective_address(mode)?;
                cycles += extra;
                self.regs.pc = ea;
            }
            Nega | Coma | Lsra | Rora | Asra | Asla | Rola | Deca | Inca | Tsta | Clra => {
                self.regs.a = self.unary(op.opcode & 0x0F, self.regs.a);
            }
            Negb | Comb | Lsrb | Rorb | Asrb | Aslb | Rolb | Decb | Incb | Tstb | Clrb => {
                self.regs.b = self.unary(op.opcode & 0x0F, self.regs.b);
            }

            Nop => {}
            Sync => self.halt = HaltState::Waiting,
            Daa => self.daa(),
            Orcc => {
                let mask = self.fetch8()?;
                self.regs.cc = ConditionCodes::from_byte(self.regs.cc.to_byte() | mask);
            }
            Andcc => {
                let mask = self.fetch8()?;
                self.regs.cc = ConditionCodes::from_byte(self.regs.cc.to_byte() & mask);
            }
            Sex => {
                self.regs.a = if self.regs.b & 0x80 != 0 { 0xFF } else { 0 };
                self.update(N | Z, nz16(self.regs.d()));
            }
            Exg => {
                let postbyte = self.fetch8()?;
                let (r1, r2) = (postbyte >> 4, postbyte & 0x0F);
                let (v1, v2) = (self.register(r1), self.register(r2));
                self.set_register(r1, resize(v2, r2, r1));
                self.set_register(r2, resize(v1, r1, r2));
            }
            Tfr => {
                let postbyte = self.fetch8()?;
                let (src, dst) = (postbyte >> 4, postbyte & 0x0F);
                let value = self.register(src);
                self.set_register(dst, resize(value, src, dst));
            }

            Bra | Brn | Bhi | Bls | Bcc | Bcs | Bne | Beq | Bvc | Bvs | Bpl | Bmi | Bge | Blt
            | Bgt | Ble => {
                let offset = self.fetch8()? as i8 as u16;
                if self.condition(op.opcode) {
                    self.regs.pc = self.regs.pc.wrapping_add(offset);
                }
            }
            Lbrn | Lbhi | Lbls | Lbcc | Lbcs | Lbne | Lbeq | Lbvc | Lbvs | Lbpl | Lbmi | Lbge
            | Lblt | Lbgt | Lble => {
                let offset = self.fetch16()?;
                if self.condition(op.opcode) {
                    self.regs.pc = self.regs.pc.wrapping_add(offset);
                    cycles += 1;
                }
            }
            Lbra => {
                let offset = self.fetch16()?;
                self.regs.pc = self.regs.pc.wrapping_add(offset);
            }
            Bsr => {
                let offset = self.fetch8()? as i8 as u16;
                self.call(self.regs.pc.wrapping_add(offset))?;
            }
            Lbsr => {
                let offset = self.fetch16()?;
                self.call(self.regs.pc.wrapping_add(offset))?;
            }
            Jsr => {
                let (ea, extra) = self.effective_address(mode)?;
                cycles += extra;
                self.call(ea)?;
            }
            Rts => self.regs.pc = self.pull16(Stack::S)?,

            Leax | Leay | Leas | Leau => {
                let (ea, extra) = self.effective_address(mode)?;
                cycles += extra;
                match op.mnemonic {
                    Leax => {
                        self.regs.x = ea;
                        self.update(Z, nz16(ea) & Z);
                    }
                    Leay => {
                        self.regs.y = ea;
                        self.update(Z, nz16(ea) & Z);
                    }
                    Leas => self.load_s(ea),
                    _ => self.regs.u = ea,
                }
            }
            Pshs | Puls | Pshu | Pulu => {
                let list = self.fetch8()?;
                let stack = if matches!(op.mnemonic, Pshs | Puls) {
                    Stack::S
                } else {
                    Stack::U
                };
                cycles += if matches!(op.mnemonic, Pshs | Pshu) {
                    self.push_registers(stack, list)?
                } else {
                    self.pull_registers(stack, list)?
                };
            }
            Abx => self.regs.x = self.regs.x.wrapping_add(self.regs.b as u16),
            Rti => {
                self.pull_registers(Stack::S, PUSH_CC)?;
                if self.regs.cc.contains(ConditionCodes::E) {
                    cycles += self.pull_registers(Stack::S, PUSH_ALL & !PUSH_CC)? - 2;
                } else {
                    self.pull_registers(Stack::S, PUSH_PC)?;
                }
            }
            Cwai => {
                let mask = self.fetch8()?;
                self.regs.cc = ConditionCodes::from_byte(self.regs.cc.to_byte() & mask);
                self.regs.cc.insert(ConditionCodes::E);
                self.push_registers(Stack::S, PUSH_ALL)?;
                self.halt = HaltState::Waiting;
            }
            Mul => {
                let d = self.regs.a as u16 * self.regs.b as u16;
                self.regs.set_d(d);
                self.update(Z | C, (nz16(d) & Z) | ((d >> 7) as u8 & C));
            }
            Swi => self.software_interrupt(SWI_VECTOR, true)?,
            Swi2 => self.software_interrupt(SWI2_VECTOR, false)?,
            Swi3 => self.software_interrupt(SWI3_VECTOR, false)?,

            // 8-bit ALU operations on A or B. The opcode's low nibble selects
            // the operation.
            Suba | Cmpa | Sbca | Anda | Bita | Lda | Eora | Adca | Ora | Adda => {
                let (value, extra) = self.operand8(mode)?;
                cycles += extra;
                self.regs.a = self.alu8(op.opcode & 0x0F, self.regs.a, value);
            }
            Subb | Cmpb | Sbcb | Andb | Bitb | Ldb | Eorb | Adcb | Orb | Addb => {
                let (value, extra) = self.operand8(mode)?;
                cycles += extra;
                self.regs.b = self.alu8(op.opcode & 0x0F, self.regs.b, value);
            }
            Sta | Stb => {
                let (ea, extra) = self.effective_address(mode)?;
                cycles += extra;
                let value = if op.mnemonic == Sta {
                    self.regs.a
                } else {
                    self.regs.b
                };
                self.write8(ea, value)?;
                self.update(N | Z | V, nz8(value));
            }

            // 16-bit arithmetic and compares.
            Subd | Addd | Cmpd | Cmpx | Cmpy | Cmpu | Cmps => {
                let (value, extra) = self.operand16(mode)?;
                cycles += extra;
                match op.mnemonic {
                    Subd => {
                        let d = self.sub16(self.regs.d(), value);
                        self.regs.set_d(d);
                    }
                    Addd => {
                        let d = self.add16(self.regs.d(), value);
                        self.regs.set_d(d);
                    }
                    Cmpd => _ = self.sub16(self.regs.d(), value),
                    Cmpx => _ = self.sub16(self.regs.x, value),
                    Cmpy => _ = self.sub16(self.regs.y, value),
                    Cmpu => _ = self.sub16(self.regs.u, value),
                    _ => _ = self.sub16(self.regs.s, value),
                }
            }
            Ldd | Ldx | Ldy | Ldu | Lds => {
                let (value, extra) = self.operand16(mode)?;
                cycles += extra;
                match op.mnemonic {
                    Ldd => self.regs.set_d(value),
                    Ldx => self.regs.x = value,
                    Ldy => self.regs.y = value,
                    Ldu => self.regs.u = value,
                    _ => self.load_s(value),
                }
                self.update(N | Z | V, nz16(value));
            }
            Std | Stx | Sty | Stu | Sts => {
                let (ea, extra) = self.effective_address(mode)?;
                cycles += extra;
                let value = match op.mnemonic {
                    Std => self.regs.d(),
                    Stx => self.regs.x,
                    Sty => self.regs.y,
                    Stu => self.regs.u,
                    _ => self.regs.s,
                };
                self.write16(ea, value)?;
                self.update(N | Z | V, nz16(value));
            }
        }
        Ok(cycles)
    }

    // --- condition codes ---

    /// Replaces the CC bits in `mask` with those in `bits`.
    #[inline]
    fn update(&mut self, mask: u8, bits: u8) {
        self.regs.cc = ConditionCodes::from_byte((self.regs.cc.to_byte() & !mask) | bits);
    }

    #[inline]
    fn carry(&self) -> u8 {
        self.regs.cc.to_byte() & C
    }

    /// Evaluates a branch condition: the low nibble of a short ($20–$2F) or
    /// long (page 2 $21–$2F) branch opcode.
    #[inline]
    fn condition(&self, opcode: u8) -> bool {
        let cc = self.regs.cc;
        let n = cc.contains(ConditionCodes::N);
        let z = cc.contains(ConditionCodes::Z);
        let v = cc.contains(ConditionCodes::V);
        let c = cc.contains(ConditionCodes::C);
        match opcode & 0x0F {
            0x0 => true,         // BRA
            0x1 => false,        // BRN
            0x2 => !(c || z),    // BHI
            0x3 => c || z,       // BLS
            0x4 => !c,           // BCC/BHS
            0x5 => c,            // BCS/BLO
            0x6 => !z,           // BNE
            0x7 => z,            // BEQ
            0x8 => !v,           // BVC
            0x9 => v,            // BVS
            0xA => !n,           // BPL
            0xB => n,            // BMI
            0xC => n == v,       // BGE
            0xD => n != v,       // BLT
            0xE => !z && n == v, // BGT
            _ => z || n != v,    // BLE
        }
    }

    // --- ALU ---

    /// A + B + carry, setting H, N, Z, V and C.
    fn add8(&mut self, a: u8, b: u8, carry: u8) -> u8 {
        let sum = a as u16 + b as u16 + carry as u16;
        let r = sum as u8;
        let half = ((a ^ b ^ r) & 0x10) << 1;
        let overflow = (((a ^ r) & (b ^ r)) >> 6) & V;
        self.update(
            H | N | Z | V | C,
            half | nz8(r) | overflow | (sum >> 8) as u8,
        );
        r
    }

    /// A − B − borrow, setting N, Z, V and C (C is the borrow).
    fn sub8(&mut self, a: u8, b: u8, borrow: u8) -> u8 {
        let diff = (a as u16)
            .wrapping_sub(b as u16)
            .wrapping_sub(borrow as u16);
        let r = diff as u8;
        let overflow = (((a ^ b) & (a ^ r)) >> 6) & V;
        self.update(N | Z | V | C, nz8(r) | overflow | ((diff >> 8) as u8 & C));
        r
    }

    /// A + B, setting N, Z, V and C.
    fn add16(&mut self, a: u16, b: u16) -> u16 {
        let sum = a as u32 + b as u32;
        let r = sum as u16;
        let overflow = ((((a ^ r) & (b ^ r)) >> 14) as u8) & V;
        self.update(N | Z | V | C, nz16(r) | overflow | (sum >> 16) as u8);
        r
    }

    /// A − B, setting N, Z, V and C (C is the borrow).
    fn sub16(&mut self, a: u16, b: u16) -> u16 {
        let diff = (a as u32).wrapping_sub(b as u32);
        let r = diff as u16;
        let overflow = ((((a ^ b) & (a ^ r)) >> 14) as u8) & V;
        self.update(N | Z | V | C, nz16(r) | overflow | ((diff >> 16) as u8 & C));
        r
    }

    /// The single-operand operations shared by the memory ($0x, $6x, $7x)
    /// and accumulator ($4x, $5x) forms, selected by the opcode's low
    /// nibble. Returns the result; TST returns its operand unchanged.
    #[inline]
    fn unary(&mut self, operation: u8, value: u8) -> u8 {
        match operation {
            0x0 => self.sub8(0, value, 0), // NEG
            0x3 => {
                // COM
                let r = !value;
                self.update(N | Z | V | C, nz8(r) | C);
                r
            }
            0x4 => {
                // LSR
                let r = value >> 1;
                self.update(N | Z | C, nz8(r) | (value & C));
                r
            }
            0x6 => {
                // ROR
                let r = (value >> 1) | (self.carry() << 7);
                self.update(N | Z | C, nz8(r) | (value & C));
                r
            }
            0x7 => {
                // ASR
                let r = (value >> 1) | (value & 0x80);
                self.update(N | Z | C, nz8(r) | (value & C));
                r
            }
            0x8 | 0x9 => {
                // ASL/LSL, ROL
                let carry_in = if operation == 0x9 { self.carry() } else { 0 };
                let r = (value << 1) | carry_in;
                let overflow = ((value ^ (value << 1)) >> 6) & V;
                self.update(N | Z | V | C, nz8(r) | overflow | (value >> 7));
                r
            }
            0xA => {
                // DEC
                let r = value.wrapping_sub(1);
                self.update(N | Z | V, nz8(r) | if value == 0x80 { V } else { 0 });
                r
            }
            0xC => {
                // INC
                let r = value.wrapping_add(1);
                self.update(N | Z | V, nz8(r) | if value == 0x7F { V } else { 0 });
                r
            }
            0xD => {
                // TST
                self.update(N | Z | V, nz8(value));
                value
            }
            _ => {
                // CLR
                self.update(N | Z | V | C, Z);
                0
            }
        }
    }

    /// The 8-bit accumulator operations of the $80–$FF columns, selected by
    /// the opcode's low nibble. Returns the accumulator's new value: CMP and
    /// BIT return it unchanged.
    #[inline]
    fn alu8(&mut self, operation: u8, acc: u8, value: u8) -> u8 {
        match operation {
            0x0 => self.sub8(acc, value, 0), // SUB
            0x1 => {
                // CMP
                self.sub8(acc, value, 0);
                acc
            }
            0x2 => {
                // SBC
                let borrow = self.carry();
                self.sub8(acc, value, borrow)
            }
            0x4 | 0x5 => {
                // AND, BIT
                let r = acc & value;
                self.update(N | Z | V, nz8(r));
                if operation == 0x4 { r } else { acc }
            }
            0x6 => {
                // LD
                self.update(N | Z | V, nz8(value));
                value
            }
            0x8 => {
                // EOR
                let r = acc ^ value;
                self.update(N | Z | V, nz8(r));
                r
            }
            0x9 => {
                // ADC
                let carry = self.carry();
                self.add8(acc, value, carry)
            }
            0xA => {
                // OR
                let r = acc | value;
                self.update(N | Z | V, nz8(r));
                r
            }
            _ => self.add8(acc, value, 0), // ADD
        }
    }

    /// Decimal-adjusts A after a BCD addition.
    fn daa(&mut self) {
        let a = self.regs.a;
        let cc = self.regs.cc.to_byte();
        let low = a & 0x0F;
        let high = a & 0xF0;
        let mut correction = 0u16;
        if cc & H != 0 || low > 9 {
            correction |= 0x06;
        }
        if cc & C != 0 || high > 0x90 || (high > 0x80 && low > 9) {
            correction |= 0x60;
        }
        let sum = a as u16 + correction;
        let r = sum as u8;
        self.regs.a = r;
        self.update(N | Z | C, nz8(r) | (cc & C) | (sum >> 8) as u8);
    }

    // --- addressing ---

    /// Returns the effective address of a direct, extended or indexed
    /// operand, and the extra cycles an indexed postbyte adds.
    #[inline]
    fn effective_address(&mut self, mode: AddressingMode) -> Result<(u16, u8), ExecError> {
        match mode {
            AddressingMode::Direct => {
                let low = self.fetch8()?;
                Ok((u16::from_be_bytes([self.regs.dp, low]), 0))
            }
            AddressingMode::Extended => Ok((self.fetch16()?, 0)),
            AddressingMode::Indexed => self.indexed(),
            _ => unreachable!("{mode:?} has no effective address"),
        }
    }

    /// Fetches an 8-bit operand: immediate, or read from its effective
    /// address. Returns the value and any indexed extra cycles.
    #[inline]
    fn operand8(&mut self, mode: AddressingMode) -> Result<(u8, u8), ExecError> {
        if mode == AddressingMode::Immediate8 {
            return Ok((self.fetch8()?, 0));
        }
        let (ea, extra) = self.effective_address(mode)?;
        Ok((self.read8(ea)?, extra))
    }

    /// Fetches a 16-bit operand: immediate, or read from its effective
    /// address. Returns the value and any indexed extra cycles.
    #[inline]
    fn operand16(&mut self, mode: AddressingMode) -> Result<(u16, u8), ExecError> {
        if mode == AddressingMode::Immediate16 {
            return Ok((self.fetch16()?, 0));
        }
        let (ea, extra) = self.effective_address(mode)?;
        Ok((self.read16(ea)?, extra))
    }

    /// Decodes an indexed postbyte and its offset bytes, updating the index
    /// register for the auto-increment and auto-decrement forms. Returns the
    /// effective address and the postbyte's extra cycles (see
    /// [`indexed`]). An undefined postbyte is reported as
    /// [`ExecError::InvalidOpcode`] before anything changes but PC.
    fn indexed(&mut self) -> Result<(u16, u8), ExecError> {
        let postbyte = self.fetch8()?;
        let register = (postbyte >> 5) & 0x03;
        let base = self.index_register(register);
        if postbyte & indexed::OFFSET5 == 0 {
            // 5-bit signed offset.
            let offset = (((postbyte & 0x1F) << 3) as i8 >> 3) as u16;
            return Ok((base.wrapping_add(offset), 1));
        }
        if !indexed::is_defined(postbyte) {
            // `step` reports this as the instruction's opcode.
            return Err(ExecError::InvalidOpcode {
                addr: self.instr_pc,
                opcode: postbyte,
            });
        }
        let (ea, extra) = match postbyte & 0x0F {
            0x0 => {
                self.set_index_register(register, base.wrapping_add(1));
                (base, 2)
            }
            0x1 => {
                self.set_index_register(register, base.wrapping_add(2));
                (base, 3)
            }
            0x2 => {
                let ea = base.wrapping_sub(1);
                self.set_index_register(register, ea);
                (ea, 2)
            }
            0x3 => {
                let ea = base.wrapping_sub(2);
                self.set_index_register(register, ea);
                (ea, 3)
            }
            0x4 => (base, 0),
            0x5 => (base.wrapping_add(self.regs.b as i8 as u16), 1),
            0x6 => (base.wrapping_add(self.regs.a as i8 as u16), 1),
            0x8 => {
                let offset = self.fetch8()? as i8 as u16;
                (base.wrapping_add(offset), 1)
            }
            0x9 => {
                let offset = self.fetch16()?;
                (base.wrapping_add(offset), 4)
            }
            0xB => (base.wrapping_add(self.regs.d()), 4),
            0xC => {
                let offset = self.fetch8()? as i8 as u16;
                (self.regs.pc.wrapping_add(offset), 1)
            }
            0xD => {
                let offset = self.fetch16()?;
                (self.regs.pc.wrapping_add(offset), 5)
            }
            // 0xF: extended indirect, [n16]; the indirection adds 3 more.
            _ => (self.fetch16()?, 2),
        };
        if postbyte & indexed::INDIRECT != 0 {
            Ok((self.read16(ea)?, extra + 3))
        } else {
            Ok((ea, extra))
        }
    }

    #[inline]
    fn index_register(&self, register: u8) -> u16 {
        match register {
            0 => self.regs.x,
            1 => self.regs.y,
            2 => self.regs.u,
            _ => self.regs.s,
        }
    }

    #[inline]
    fn set_index_register(&mut self, register: u8, value: u16) {
        match register {
            0 => self.regs.x = value,
            1 => self.regs.y = value,
            2 => self.regs.u = value,
            _ => self.regs.s = value,
        }
    }

    /// Loads S by an explicit instruction (LDS, LEAS, TFR/EXG, PULU),
    /// unlike the stack pointer's own pushes and pulls.
    #[inline]
    fn load_s(&mut self, value: u16) {
        self.regs.s = value;
    }

    // --- TFR/EXG registers ---

    /// Reads the register a TFR/EXG code names. 8-bit registers are in the
    /// low byte; undefined codes read as all ones.
    fn register(&self, code: u8) -> u16 {
        match code {
            0x0 => self.regs.d(),
            0x1 => self.regs.x,
            0x2 => self.regs.y,
            0x3 => self.regs.u,
            0x4 => self.regs.s,
            0x5 => self.regs.pc,
            0x8 => self.regs.a as u16,
            0x9 => self.regs.b as u16,
            0xA => self.regs.cc.to_byte() as u16,
            0xB => self.regs.dp as u16,
            0x6 | 0x7 => 0xFFFF,
            _ => 0x00FF,
        }
    }

    /// Writes the register a TFR/EXG code names; an 8-bit register takes the
    /// low byte. Writes to undefined codes are ignored.
    fn set_register(&mut self, code: u8, value: u16) {
        match code {
            0x0 => self.regs.set_d(value),
            0x1 => self.regs.x = value,
            0x2 => self.regs.y = value,
            0x3 => self.regs.u = value,
            0x4 => self.load_s(value),
            0x5 => self.regs.pc = value,
            0x8 => self.regs.a = value as u8,
            0x9 => self.regs.b = value as u8,
            0xA => self.regs.cc = ConditionCodes::from_byte(value as u8),
            0xB => self.regs.dp = value as u8,
            _ => {}
        }
    }

    // --- stacks ---

    #[inline]
    fn stack_pointer(&self, stack: Stack) -> u16 {
        match stack {
            Stack::S => self.regs.s,
            Stack::U => self.regs.u,
        }
    }

    #[inline]
    fn set_stack_pointer(&mut self, stack: Stack, value: u16) {
        match stack {
            Stack::S => self.regs.s = value,
            Stack::U => self.regs.u = value,
        }
    }

    fn push8(&mut self, stack: Stack, value: u8) -> Result<(), ExecError> {
        let sp = self.stack_pointer(stack).wrapping_sub(1);
        self.set_stack_pointer(stack, sp);
        self.write8(sp, value)
    }

    /// Pushes a word so it sits big-endian on the stack.
    fn push16(&mut self, stack: Stack, value: u16) -> Result<(), ExecError> {
        let [hi, lo] = value.to_be_bytes();
        self.push8(stack, lo)?;
        self.push8(stack, hi)
    }

    fn pull8(&mut self, stack: Stack) -> Result<u8, ExecError> {
        let sp = self.stack_pointer(stack);
        let value = self.read8(sp)?;
        self.set_stack_pointer(stack, sp.wrapping_add(1));
        Ok(value)
    }

    fn pull16(&mut self, stack: Stack) -> Result<u16, ExecError> {
        let hi = self.pull8(stack)?;
        let lo = self.pull8(stack)?;
        Ok(u16::from_be_bytes([hi, lo]))
    }

    /// Pushes the registers in `list` (a PSH postbyte) in the 6809's order:
    /// PC, U or S, Y, X, DP, B, A, CC. Returns the number of bytes pushed.
    fn push_registers(&mut self, stack: Stack, list: u8) -> Result<u8, ExecError> {
        let mut bytes = 0;
        if list & PUSH_PC != 0 {
            self.push16(stack, self.regs.pc)?;
            bytes += 2;
        }
        if list & PUSH_OTHER_STACK != 0 {
            let other = match stack {
                Stack::S => self.regs.u,
                Stack::U => self.regs.s,
            };
            self.push16(stack, other)?;
            bytes += 2;
        }
        if list & PUSH_Y != 0 {
            self.push16(stack, self.regs.y)?;
            bytes += 2;
        }
        if list & PUSH_X != 0 {
            self.push16(stack, self.regs.x)?;
            bytes += 2;
        }
        if list & PUSH_DP != 0 {
            self.push8(stack, self.regs.dp)?;
            bytes += 1;
        }
        if list & PUSH_B != 0 {
            self.push8(stack, self.regs.b)?;
            bytes += 1;
        }
        if list & PUSH_A != 0 {
            self.push8(stack, self.regs.a)?;
            bytes += 1;
        }
        if list & PUSH_CC != 0 {
            self.push8(stack, self.regs.cc.to_byte())?;
            bytes += 1;
        }
        Ok(bytes)
    }

    /// Pulls the registers in `list` (a PUL postbyte) in the reverse of the
    /// push order. Returns the number of bytes pulled.
    fn pull_registers(&mut self, stack: Stack, list: u8) -> Result<u8, ExecError> {
        let mut bytes = 0;
        if list & PUSH_CC != 0 {
            self.regs.cc = ConditionCodes::from_byte(self.pull8(stack)?);
            bytes += 1;
        }
        if list & PUSH_A != 0 {
            self.regs.a = self.pull8(stack)?;
            bytes += 1;
        }
        if list & PUSH_B != 0 {
            self.regs.b = self.pull8(stack)?;
            bytes += 1;
        }
        if list & PUSH_DP != 0 {
            self.regs.dp = self.pull8(stack)?;
            bytes += 1;
        }
        if list & PUSH_X != 0 {
            self.regs.x = self.pull16(stack)?;
            bytes += 2;
        }
        if list & PUSH_Y != 0 {
            self.regs.y = self.pull16(stack)?;
            bytes += 2;
        }
        if list & PUSH_OTHER_STACK != 0 {
            let value = self.pull16(stack)?;
            match stack {
                Stack::S => self.regs.u = value,
                Stack::U => self.load_s(value),
            }
            bytes += 2;
        }
        if list & PUSH_PC != 0 {
            self.regs.pc = self.pull16(stack)?;
            bytes += 2;
        }
        Ok(bytes)
    }

    /// Pushes the return address (PC) on S and jumps to `target`.
    fn call(&mut self, target: u16) -> Result<(), ExecError> {
        self.push16(Stack::S, self.regs.pc)?;
        self.regs.pc = target;
        Ok(())
    }

    /// SWI, SWI2 or SWI3: sets E, pushes the entire state on S, masks IRQ and
    /// FIRQ if `mask` (SWI only), and jumps through `vector`.
    fn software_interrupt(&mut self, vector: u16, mask: bool) -> Result<(), ExecError> {
        self.regs.cc.insert(ConditionCodes::E);
        self.push_registers(Stack::S, PUSH_ALL)?;
        if mask {
            self.regs.cc.insert(ConditionCodes::I | ConditionCodes::F);
        }
        let resolved = self.machine.resolve_vector(vector);
        self.regs.pc = self.read16(resolved)?;
        Ok(())
    }
}

/// Converts a TFR/EXG value from register `from`'s size to register `to`'s:
/// 8-bit to 16-bit puts `$FF` in the high byte. (16-bit to 8-bit keeps the
/// low byte, which [`Cpu::set_register`] does.)
#[inline]
fn resize(value: u16, from: u8, to: u8) -> u16 {
    let wide = |code: u8| code < 8;
    if !wide(from) && wide(to) {
        0xFF00 | value
    } else {
        value
    }
}
