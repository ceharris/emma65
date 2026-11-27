//! The 6809's opcode map: one table per opcode page, built at compile time.
//!
//! Every entry gives the instruction's mnemonic, its addressing mode, and its
//! **base** cycle count from the MC6809 datasheet. Execution adds the
//! documented extras on top:
//!
//! - indexed addressing: the postbyte's extra cycles (see
//!   [`indexed`](super::indexed));
//! - a taken long conditional branch (page 2): one cycle;
//! - PSHS/PULS/PSHU/PULU: one cycle per byte moved;
//! - RTI with the E flag set: nine cycles for the entire-state pull.
//!
//! Undocumented opcodes decode as [`Mnemonic::Invalid`], which the CPU's
//! [`InvalidOpcodePolicy`](super::InvalidOpcodePolicy) handles.

/// Which opcode table an instruction comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    /// Unprefixed opcodes.
    Page1,
    /// Opcodes after a `$10` prefix byte.
    Page2,
    /// Opcodes after a `$11` prefix byte.
    Page3,
}

impl Page {
    /// Length of the page's prefix in bytes: 0 for page 1, otherwise 1.
    pub fn prefix_len(self) -> u16 {
        match self {
            Page::Page1 => 0,
            Page::Page2 | Page::Page3 => 1,
        }
    }
}

/// How an instruction finds its operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressingMode {
    /// No operand bytes.
    Inherent,
    /// A one-byte immediate value.
    Immediate8,
    /// A two-byte immediate value, high byte first.
    Immediate16,
    /// A one-byte address in the page selected by DP.
    Direct,
    /// A two-byte address.
    Extended,
    /// An indexed postbyte, plus 0–2 offset or address bytes it calls for.
    Indexed,
    /// A one-byte signed branch offset.
    Relative8,
    /// A two-byte signed branch offset.
    Relative16,
    /// TFR/EXG's postbyte naming two registers.
    RegisterPair,
    /// PSH/PUL's postbyte naming a set of registers.
    RegisterList,
}

impl AddressingMode {
    /// Operand bytes after the opcode, counting an indexed postbyte but not
    /// the offset bytes it may call for.
    pub fn operand_len(self) -> u16 {
        match self {
            AddressingMode::Inherent => 0,
            AddressingMode::Immediate8
            | AddressingMode::Direct
            | AddressingMode::Indexed
            | AddressingMode::Relative8
            | AddressingMode::RegisterPair
            | AddressingMode::RegisterList => 1,
            AddressingMode::Immediate16 | AddressingMode::Extended | AddressingMode::Relative16 => {
                2
            }
        }
    }
}

/// The 6809's instructions. Accumulator forms are distinct mnemonics (`Lda`,
/// `Ldb`), as in the datasheet's tables. `ASL` and `LSL` are one instruction,
/// named `Asl` here; likewise `BCC`/`BHS` (`Bcc`) and `BCS`/`BLO` (`Bcs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Mnemonic {
    Invalid,
    /// Not an instruction: the CPU serviced a hardware interrupt (NMI, FIRQ
    /// or IRQ) in this step. See [`DecodedOp::interrupt`].
    Interrupt,
    // Memory read-modify-write, and JMP.
    Neg,
    Com,
    Lsr,
    Ror,
    Asr,
    Asl,
    Rol,
    Dec,
    Inc,
    Tst,
    Jmp,
    Clr,
    // Accumulator inherent.
    Nega,
    Coma,
    Lsra,
    Rora,
    Asra,
    Asla,
    Rola,
    Deca,
    Inca,
    Tsta,
    Clra,
    Negb,
    Comb,
    Lsrb,
    Rorb,
    Asrb,
    Aslb,
    Rolb,
    Decb,
    Incb,
    Tstb,
    Clrb,
    // Miscellaneous page 1.
    Nop,
    Sync,
    Lbra,
    Lbsr,
    Daa,
    Orcc,
    Andcc,
    Sex,
    Exg,
    Tfr,
    // Short branches, in opcode order ($20–$2F).
    Bra,
    Brn,
    Bhi,
    Bls,
    Bcc,
    Bcs,
    Bne,
    Beq,
    Bvc,
    Bvs,
    Bpl,
    Bmi,
    Bge,
    Blt,
    Bgt,
    Ble,
    // Long conditional branches, in opcode order (page 2, $21–$2F).
    Lbrn,
    Lbhi,
    Lbls,
    Lbcc,
    Lbcs,
    Lbne,
    Lbeq,
    Lbvc,
    Lbvs,
    Lbpl,
    Lbmi,
    Lbge,
    Lblt,
    Lbgt,
    Lble,
    // $30–$3F.
    Leax,
    Leay,
    Leas,
    Leau,
    Pshs,
    Puls,
    Pshu,
    Pulu,
    Rts,
    Abx,
    Rti,
    Cwai,
    Mul,
    Swi,
    Swi2,
    Swi3,
    // A-register column ($80–$BF).
    Suba,
    Cmpa,
    Sbca,
    Subd,
    Anda,
    Bita,
    Lda,
    Sta,
    Eora,
    Adca,
    Ora,
    Adda,
    Cmpx,
    Bsr,
    Jsr,
    Ldx,
    Stx,
    // B-register column ($C0–$FF).
    Subb,
    Cmpb,
    Sbcb,
    Addd,
    Andb,
    Bitb,
    Ldb,
    Stb,
    Eorb,
    Adcb,
    Orb,
    Addb,
    Ldd,
    Std,
    Ldu,
    Stu,
    // Pages 2 and 3.
    Cmpd,
    Cmpy,
    Ldy,
    Sty,
    Lds,
    Sts,
    Cmpu,
    Cmps,
}

/// One opcode-table entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpInfo {
    /// The instruction.
    pub mnemonic: Mnemonic,
    /// Its addressing mode.
    pub mode: AddressingMode,
    /// Its base cycle count (see the module docs for the extras).
    pub cycles: u8,
}

/// Cycles an undocumented opcode takes under
/// [`InvalidOpcodePolicy::Nop`](super::InvalidOpcodePolicy::Nop).
pub const INVALID_OPCODE_CYCLES: u8 = 2;

const INVALID: OpInfo = OpInfo {
    mnemonic: Mnemonic::Invalid,
    mode: AddressingMode::Inherent,
    cycles: INVALID_OPCODE_CYCLES,
};

/// A decoded instruction: where it came from in the opcode map, and its
/// table entry. This is the 6809's [`Processor::Op`](emma6k_core::Processor::Op).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedOp {
    /// The opcode page.
    pub page: Page,
    /// The opcode byte (after any prefix).
    pub opcode: u8,
    /// The instruction.
    pub mnemonic: Mnemonic,
    /// Its addressing mode.
    pub mode: AddressingMode,
    /// Its base cycle count.
    pub base_cycles: u8,
}

impl DecodedOp {
    /// Builds the decoded form of `opcode` on `page`. Inlined: `step` calls
    /// it for every instruction.
    #[inline]
    pub fn new(page: Page, opcode: u8) -> Self {
        let info = lookup(page, opcode);
        Self {
            page,
            opcode,
            mnemonic: info.mnemonic,
            mode: info.mode,
            base_cycles: info.cycles,
        }
    }

    /// The step result's op for a serviced hardware interrupt, which isn't
    /// an instruction: mnemonic [`Mnemonic::Interrupt`], opcode 0, and the
    /// service's cycles.
    pub fn interrupt(cycles: u8) -> Self {
        Self {
            page: Page::Page1,
            opcode: 0,
            mnemonic: Mnemonic::Interrupt,
            mode: AddressingMode::Inherent,
            base_cycles: cycles,
        }
    }

    /// Instruction length in bytes, not counting the offset bytes an indexed
    /// postbyte may call for (see [`indexed::extra_bytes`](super::indexed::extra_bytes)).
    pub fn base_len(&self) -> u16 {
        self.page.prefix_len() + 1 + self.mode.operand_len()
    }

    /// Returns `true` for an undocumented opcode.
    pub fn is_invalid(&self) -> bool {
        self.mnemonic == Mnemonic::Invalid
    }
}

/// Returns the table entry for `opcode` on `page`.
#[inline]
pub fn lookup(page: Page, opcode: u8) -> &'static OpInfo {
    match page {
        Page::Page1 => &PAGE1[opcode as usize],
        Page::Page2 => &PAGE2[opcode as usize],
        Page::Page3 => &PAGE3[opcode as usize],
    }
}

/// Page 1: the unprefixed opcodes.
pub static PAGE1: [OpInfo; 256] = build_page1();
/// Page 2: the opcodes after a `$10` prefix.
pub static PAGE2: [OpInfo; 256] = build_page2();
/// Page 3: the opcodes after a `$11` prefix.
pub static PAGE3: [OpInfo; 256] = build_page3();

const fn op(mnemonic: Mnemonic, mode: AddressingMode, cycles: u8) -> OpInfo {
    OpInfo {
        mnemonic,
        mode,
        cycles,
    }
}

const fn build_page1() -> [OpInfo; 256] {
    use AddressingMode::*;
    use Mnemonic::*;
    let mut t = [INVALID; 256];

    // $00–$0F direct, $60–$6F indexed, $70–$7F extended: memory
    // read-modify-write (6/6/7 cycles) and JMP (3/3/4).
    let rmw = [
        Neg, Invalid, Invalid, Com, Lsr, Invalid, Ror, Asr, Asl, Rol, Dec, Invalid, Inc, Tst, Jmp,
        Clr,
    ];
    // $40–$4F and $50–$5F: accumulator inherent, 2 cycles.
    let acc_a = [
        Nega, Invalid, Invalid, Coma, Lsra, Invalid, Rora, Asra, Asla, Rola, Deca, Invalid, Inca,
        Tsta, Invalid, Clra,
    ];
    let acc_b = [
        Negb, Invalid, Invalid, Comb, Lsrb, Invalid, Rorb, Asrb, Aslb, Rolb, Decb, Invalid, Incb,
        Tstb, Invalid, Clrb,
    ];
    let mut i = 0;
    while i < 16 {
        let m = rmw[i];
        if !matches!(m, Invalid) {
            let is_jmp = matches!(m, Jmp);
            t[i] = op(m, Direct, if is_jmp { 3 } else { 6 });
            t[0x60 + i] = op(m, Indexed, if is_jmp { 3 } else { 6 });
            t[0x70 + i] = op(m, Extended, if is_jmp { 4 } else { 7 });
        }
        if !matches!(acc_a[i], Invalid) {
            t[0x40 + i] = op(acc_a[i], Inherent, 2);
        }
        if !matches!(acc_b[i], Invalid) {
            t[0x50 + i] = op(acc_b[i], Inherent, 2);
        }
        i += 1;
    }

    t[0x12] = op(Nop, Inherent, 2);
    t[0x13] = op(Sync, Inherent, 4);
    t[0x16] = op(Lbra, Relative16, 5);
    t[0x17] = op(Lbsr, Relative16, 9);
    t[0x19] = op(Daa, Inherent, 2);
    t[0x1A] = op(Orcc, Immediate8, 3);
    t[0x1C] = op(Andcc, Immediate8, 3);
    t[0x1D] = op(Sex, Inherent, 2);
    t[0x1E] = op(Exg, RegisterPair, 8);
    t[0x1F] = op(Tfr, RegisterPair, 6);

    // $20–$2F: short branches, 3 cycles taken or not.
    let branches = [
        Bra, Brn, Bhi, Bls, Bcc, Bcs, Bne, Beq, Bvc, Bvs, Bpl, Bmi, Bge, Blt, Bgt, Ble,
    ];
    let mut i = 0;
    while i < 16 {
        t[0x20 + i] = op(branches[i], Relative8, 3);
        i += 1;
    }

    t[0x30] = op(Leax, Indexed, 4);
    t[0x31] = op(Leay, Indexed, 4);
    t[0x32] = op(Leas, Indexed, 4);
    t[0x33] = op(Leau, Indexed, 4);
    t[0x34] = op(Pshs, RegisterList, 5);
    t[0x35] = op(Puls, RegisterList, 5);
    t[0x36] = op(Pshu, RegisterList, 5);
    t[0x37] = op(Pulu, RegisterList, 5);
    t[0x39] = op(Rts, Inherent, 5);
    t[0x3A] = op(Abx, Inherent, 3);
    t[0x3B] = op(Rti, Inherent, 6);
    t[0x3C] = op(Cwai, Immediate8, 20);
    t[0x3D] = op(Mul, Inherent, 11);
    t[0x3F] = op(Swi, Inherent, 19);

    // $80–$BF (A column) and $C0–$FF (B column). Rows: immediate, direct,
    // indexed, extended. Direct and indexed share base cycles; extended
    // takes one more.
    let col_a = [
        Suba, Cmpa, Sbca, Subd, Anda, Bita, Lda, Sta, Eora, Adca, Ora, Adda, Cmpx, Jsr, Ldx, Stx,
    ];
    let col_b = [
        Subb, Cmpb, Sbcb, Addd, Andb, Bitb, Ldb, Stb, Eorb, Adcb, Orb, Addb, Ldd, Std, Ldu, Stu,
    ];
    let mut i = 0;
    while i < 16 {
        let ma = col_a[i];
        let mb = col_b[i];
        // (immediate cycles or 0 for none, immediate is 16-bit, direct cycles)
        let (imm_a, wide_a, dir_a) = column_cycles(ma);
        let (imm_b, wide_b, dir_b) = column_cycles(mb);
        if imm_a != 0 {
            t[0x80 + i] = op(ma, if wide_a { Immediate16 } else { Immediate8 }, imm_a);
        }
        t[0x90 + i] = op(ma, Direct, dir_a);
        t[0xA0 + i] = op(ma, Indexed, dir_a);
        t[0xB0 + i] = op(ma, Extended, dir_a + 1);
        if imm_b != 0 {
            t[0xC0 + i] = op(mb, if wide_b { Immediate16 } else { Immediate8 }, imm_b);
        }
        t[0xD0 + i] = op(mb, Direct, dir_b);
        t[0xE0 + i] = op(mb, Indexed, dir_b);
        t[0xF0 + i] = op(mb, Extended, dir_b + 1);
        i += 1;
    }
    // BSR sits where an immediate JSR would be.
    t[0x8D] = op(Bsr, Relative8, 7);
    t
}

/// Base cycles of an $80–$FF column instruction: `(immediate, immediate is
/// 16-bit, direct)`. An immediate of 0 means the instruction has no
/// immediate form.
const fn column_cycles(m: Mnemonic) -> (u8, bool, u8) {
    use Mnemonic::*;
    match m {
        Sta | Stb => (0, false, 4),
        Stx | Std | Stu => (0, false, 5),
        Subd | Addd | Cmpx => (4, true, 6),
        Ldx | Ldd | Ldu => (3, true, 5),
        Jsr => (0, false, 7),
        _ => (2, false, 4),
    }
}

const fn build_page2() -> [OpInfo; 256] {
    use AddressingMode::*;
    use Mnemonic::*;
    let mut t = [INVALID; 256];
    // $21–$2F: long conditional branches, 5 cycles (6 when taken).
    let branches = [
        Lbrn, Lbhi, Lbls, Lbcc, Lbcs, Lbne, Lbeq, Lbvc, Lbvs, Lbpl, Lbmi, Lbge, Lblt, Lbgt, Lble,
    ];
    let mut i = 0;
    while i < 15 {
        t[0x21 + i] = op(branches[i], Relative16, 5);
        i += 1;
    }
    t[0x3F] = op(Swi2, Inherent, 20);
    compare16(&mut t, 0x83, Cmpd);
    compare16(&mut t, 0x8C, Cmpy);
    // LDY/STY in the A column, LDS/STS in the B column.
    t[0x8E] = op(Ldy, Immediate16, 4);
    t[0x9E] = op(Ldy, Direct, 6);
    t[0xAE] = op(Ldy, Indexed, 6);
    t[0xBE] = op(Ldy, Extended, 7);
    t[0x9F] = op(Sty, Direct, 6);
    t[0xAF] = op(Sty, Indexed, 6);
    t[0xBF] = op(Sty, Extended, 7);
    t[0xCE] = op(Lds, Immediate16, 4);
    t[0xDE] = op(Lds, Direct, 6);
    t[0xEE] = op(Lds, Indexed, 6);
    t[0xFE] = op(Lds, Extended, 7);
    t[0xDF] = op(Sts, Direct, 6);
    t[0xEF] = op(Sts, Indexed, 6);
    t[0xFF] = op(Sts, Extended, 7);
    t
}

const fn build_page3() -> [OpInfo; 256] {
    use AddressingMode::*;
    use Mnemonic::*;
    let mut t = [INVALID; 256];
    t[0x3F] = op(Swi3, Inherent, 20);
    compare16(&mut t, 0x83, Cmpu);
    compare16(&mut t, 0x8C, Cmps);
    t
}

/// A page 2/3 16-bit compare: immediate 5, direct and indexed 7, extended 8.
const fn compare16(t: &mut [OpInfo; 256], base: usize, m: Mnemonic) {
    use AddressingMode::*;
    t[base] = op(m, Immediate16, 5);
    t[base + 0x10] = op(m, Direct, 7);
    t[base + 0x20] = op(m, Indexed, 7);
    t[base + 0x30] = op(m, Extended, 8);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_valid(t: &[OpInfo; 256]) -> usize {
        t.iter().filter(|e| e.mnemonic != Mnemonic::Invalid).count()
    }

    #[test]
    fn documented_opcode_counts() {
        // 36 memory read-modify-write/JMP, 22 accumulator inherent, 10
        // miscellaneous ($12–$1F), 16 short branches, 14 at $30–$3F, and 123
        // in the A and B columns (128 less five store/JSR immediates, plus
        // BSR).
        assert_eq!(count_valid(&PAGE1), 221);
        assert_eq!(count_valid(&PAGE2), 38);
        assert_eq!(count_valid(&PAGE3), 9);
    }

    #[test]
    fn undocumented_page1_opcodes_are_invalid() {
        for opcode in [
            0x01, 0x02, 0x05, 0x0B, 0x10, 0x11, 0x14, 0x15, 0x18, 0x1B, 0x38, 0x3E, 0x41, 0x42,
            0x45, 0x4B, 0x4E, 0x51, 0x52, 0x55, 0x5B, 0x5E, 0x61, 0x62, 0x65, 0x6B, 0x71, 0x72,
            0x75, 0x7B, 0x87, 0x8F, 0xC7, 0xCD, 0xCF,
        ] {
            assert_eq!(
                PAGE1[opcode].mnemonic,
                Mnemonic::Invalid,
                "opcode ${opcode:02X}"
            );
        }
    }

    #[test]
    fn spot_check_entries() {
        let e = |p, o| {
            let d = DecodedOp::new(p, o);
            (d.mnemonic, d.mode, d.base_cycles, d.base_len())
        };
        use AddressingMode::*;
        use Mnemonic::*;
        use Page::*;
        assert_eq!(e(Page1, 0x86), (Lda, Immediate8, 2, 2));
        assert_eq!(e(Page1, 0xCC), (Ldd, Immediate16, 3, 3));
        assert_eq!(e(Page1, 0x83), (Subd, Immediate16, 4, 3));
        assert_eq!(e(Page1, 0x8D), (Bsr, Relative8, 7, 2));
        assert_eq!(e(Page1, 0x9D), (Jsr, Direct, 7, 2));
        assert_eq!(e(Page1, 0xBD), (Jsr, Extended, 8, 3));
        assert_eq!(e(Page1, 0x0E), (Jmp, Direct, 3, 2));
        assert_eq!(e(Page1, 0x7F), (Clr, Extended, 7, 3));
        assert_eq!(e(Page1, 0xFD), (Std, Extended, 6, 3));
        assert_eq!(e(Page2, 0x83), (Cmpd, Immediate16, 5, 4));
        assert_eq!(e(Page2, 0x8E), (Ldy, Immediate16, 4, 4));
        assert_eq!(e(Page2, 0xFF), (Sts, Extended, 7, 4));
        assert_eq!(e(Page2, 0x27), (Lbeq, Relative16, 5, 4));
        assert_eq!(e(Page3, 0xBC), (Cmps, Extended, 8, 4));
        assert_eq!(e(Page3, 0x3F), (Swi3, Inherent, 20, 2));
    }
}
