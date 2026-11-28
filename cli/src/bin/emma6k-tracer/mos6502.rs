//! Decoding 65C02 traces: the row loop and its fixed-column format.
//!
//! Column positions are reverse-engineered from the worked examples in
//! issue #268 and are not derivable from any single width constant, so
//! each field is placed at its own absolute column via [`Row::put`].

use std::io::{self, Read, Write};

use crate::columns::{Row, hex_bytes, label_line};
use emma65::disassembler::{
    DisassembledLine, Disassembler, TraceBusOp, TraceRow, TraceRowAssembler,
};
use emma65::emulator::{BinaryTraceReader, Registers, StatusRegister, SymbolTable};

/// Header line labeling each column.
pub const HEADER: &str =
    "   Seq#    Cyc A  X  Y  S  P  NV-BDIZC Addr   Code    Instruction / Operation";
/// Separator line printed immediately below [`HEADER`].
pub const SEPARATOR: &str =
    "---------- --- -- -- -- -- -- -------- ---- -------- -------------------------";

const CYC_COL: usize = 11;
const A_COL: usize = 15;
const X_COL: usize = 18;
const Y_COL: usize = 21;
const S_COL: usize = 24;
const P_COL: usize = 27;
const FLAGS_COL: usize = 30;
const ADDR_COL: usize = 39;
const CODE_COL: usize = 44;
const MNEMONIC_COL: usize = 54;
const OPERAND_COL: usize = 58;
const COMMENT_COL: usize = 66;

/// Streams a 65C02 trace through a [`TraceRowAssembler`], writing one
/// formatted row per reconstructed instruction to `out`.
pub fn run(
    input: impl Read,
    symbols: SymbolTable,
    verbose: bool,
    out: &mut dyn Write,
) -> io::Result<()> {
    let reader = BinaryTraceReader::new(input)?;
    let mut assembler = TraceRowAssembler::new(reader.variant(), symbols);

    writeln!(out, "{HEADER}")?;
    writeln!(out, "{SEPARATOR}")?;

    for item in reader {
        let rec = item?;
        if let Some(row) = assembler.feed(&rec) {
            emit(row, verbose, out)?;
        }
    }

    // Trace file truncated mid-instruction: emit whatever was reconstructed, best-effort.
    if let Some(row) = assembler.flush() {
        emit(row, verbose, out)?;
    }
    Ok(())
}

/// Writes one instruction's labels, its formatted row, and (in verbose mode)
/// its non-fetch bus operations.
fn emit(row: TraceRow, verbose: bool, out: &mut dyn Write) -> io::Result<()> {
    for label in &row.line.labels {
        writeln!(out, "{}", label_line(ADDR_COL, label))?;
    }
    writeln!(
        out,
        "{}",
        instruction_row(row.instr_id + 1, row.cycles, &row.regs, &row.line)
    )?;

    if verbose {
        for op in row.non_fetch_bus_ops() {
            match *op {
                TraceBusOp::Read { addr, value } => {
                    writeln!(out, "{}", bus_op_row(addr, "RD", value))?;
                }
                TraceBusOp::Write { addr, value } => {
                    writeln!(out, "{}", bus_op_row(addr, "WR", value))?;
                }
            }
        }
    }

    Ok(())
}

/// Renders the `NV-BDIZC` flags field: each header character if its bit is
/// set, otherwise a space. The `-` position (the UNUSED bit, always set on
/// real hardware) is never printed — it carries no information.
fn flags_field(p: StatusRegister) -> String {
    const FLAGS: [(char, StatusRegister); 7] = [
        ('N', StatusRegister::N),
        ('V', StatusRegister::V),
        ('B', StatusRegister::B),
        ('D', StatusRegister::D),
        ('I', StatusRegister::I),
        ('Z', StatusRegister::Z),
        ('C', StatusRegister::C),
    ];
    let mut s: String = FLAGS
        .iter()
        .map(|&(c, bit)| if p.contains(bit) { c } else { ' ' })
        .collect();
    s.insert(2, ' ');
    s
}

/// Formats one instruction row: sequence number, cycle count, registers,
/// flags, and the disassembled instruction's address/code/mnemonic/operand/comment.
fn instruction_row(
    seq: u64,
    cycles: Option<u8>,
    regs: &Registers,
    line: &DisassembledLine,
) -> String {
    let mut row = Row::new();
    row.put(0, &format!("{seq:>10}"));
    if let Some(c) = cycles {
        row.put(CYC_COL, &format!("{c:>3}"));
    }
    row.put(A_COL, &format!("{:02X}", regs.a));
    row.put(X_COL, &format!("{:02X}", regs.x));
    row.put(Y_COL, &format!("{:02X}", regs.y));
    row.put(S_COL, &format!("{:02X}", regs.s));
    row.put(P_COL, &format!("{:02X}", regs.p.to_byte()));
    row.put(FLAGS_COL, &flags_field(regs.p));
    row.put(ADDR_COL, &format!("{:04X}", line.addr));
    row.put(CODE_COL, &hex_bytes(&line.raw_bytes));
    // Left unpadded: `Row::put`'s own gap-fill positions the operand at
    // `OPERAND_COL` regardless of the mnemonic's length (3 or 4 letters).
    // Padding it here would leave a trailing space after operand-less
    // (implied-mode) mnemonics like `NOP`.
    row.put(MNEMONIC_COL, &line.mnemonic.to_string());
    match &line.comment_text {
        Some(comment) => {
            row.put(OPERAND_COL, &format!("{:<8}", line.operand_text));
            row.put(COMMENT_COL, comment);
        }
        None if !line.operand_text.is_empty() => row.put(OPERAND_COL, &line.operand_text),
        None => {}
    }
    row.into_string()
}

/// Formats one verbose bus-op sub-line: address, `RD`/`WR`, hex value, and
/// the same alternate-representation comment used for immediate operands.
fn bus_op_row(addr: u16, op: &str, value: u8) -> String {
    // `op` ("RD"/"WR") lands in the Code field's first byte slot; `value`
    // lands in its third byte slot ("XX XX XX" -> byte 3 starts at +6).
    const VALUE_COL: usize = CODE_COL + 6;
    let mut row = Row::new();
    row.put(ADDR_COL, &format!("{addr:04X}"));
    row.put(CODE_COL, op);
    row.put(VALUE_COL, &format!("{value:02X}"));
    row.put(COMMENT_COL, &Disassembler::immediate_mode_comment(value));
    row.into_string()
}
