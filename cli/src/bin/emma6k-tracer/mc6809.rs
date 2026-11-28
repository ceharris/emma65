//! Decoding 6809 traces: the row loop and its fixed-column format.

use std::io::{self, Read, Write};

use crate::columns::{Row, flags_field, hex_bytes, label_line};
use emma6k_core::SymbolTable;
use emma09::Registers;
use emma09::cpu::trace::BinaryTraceReader;
use emma09::disassembler::trace::vector_name;
use emma09::disassembler::{DisassembledLine, TraceBusOp, TraceRow, TraceRowAssembler};

/// Header line labeling each column.
pub const HEADER: &str = "   Seq#    Cyc A  B  X    Y    U    S    DP CC EFHINZVC Addr Code           Instruction / Operation";
/// Separator line printed immediately below [`HEADER`].
pub const SEPARATOR: &str = "---------- --- -- -- ---- ---- ---- ---- -- -- -------- ---- -------------- -------------------------";

const CYC_COL: usize = 11;
const A_COL: usize = 15;
const B_COL: usize = 18;
const X_COL: usize = 21;
const Y_COL: usize = 26;
const U_COL: usize = 31;
const S_COL: usize = 36;
const DP_COL: usize = 41;
const CC_COL: usize = 44;
const FLAGS_COL: usize = 47;
const ADDR_COL: usize = 56;
const CODE_COL: usize = 61;
const MNEMONIC_COL: usize = 76;
const OPERAND_COL: usize = 82;
const COMMENT_COL: usize = 92;

/// Streams a 6809 trace through a [`TraceRowAssembler`], writing one
/// formatted row per step to `out`.
pub fn run(
    input: impl Read,
    symbols: SymbolTable,
    verbose: bool,
    out: &mut dyn Write,
) -> io::Result<()> {
    let reader = BinaryTraceReader::new(input)?;
    let mut assembler = TraceRowAssembler::new(symbols);

    writeln!(out, "{HEADER}")?;
    writeln!(out, "{SEPARATOR}")?;

    for item in reader {
        let rec = item?;
        if let Some(row) = assembler.feed(&rec) {
            emit(&row, verbose, out)?;
        }
    }
    // A trace that ended mid-step: emit what was reconstructed.
    if let Some(row) = assembler.flush() {
        emit(&row, verbose, out)?;
    }
    Ok(())
}

/// Writes one step's labels, its row, and (in verbose mode) its non-fetch
/// bus operations.
fn emit(row: &TraceRow, verbose: bool, out: &mut dyn Write) -> io::Result<()> {
    if let Some(line) = &row.line {
        for label in &line.labels {
            writeln!(out, "{}", label_line(ADDR_COL, label))?;
        }
    }
    writeln!(out, "{}", step_row(row))?;
    if verbose {
        for op in row.non_fetch_bus_ops() {
            let (addr, op, value) = match *op {
                TraceBusOp::Read { addr, value } => (addr, "RD", value),
                TraceBusOp::Write { addr, value } => (addr, "WR", value),
            };
            writeln!(out, "{}", bus_op_row(addr, op, value))?;
        }
    }
    Ok(())
}

/// Formats one step: sequence number, cycles, registers, flags, then the
/// instruction, or for a serviced interrupt `<NAME>` and its vector.
fn step_row(row: &TraceRow) -> String {
    let mut out = Row::new();
    registers(&mut out, row.instr_id + 1, row.cycles, &row.regs);
    match &row.line {
        Some(line) => instruction(&mut out, line),
        None => {
            out.put(ADDR_COL, &format!("{:04X}", row.regs.pc));
            let vector = row.vector();
            let name = vector.and_then(vector_name).unwrap_or("INT");
            out.put(MNEMONIC_COL, &format!("<{name}>"));
            if let Some(vector) = vector {
                out.put(OPERAND_COL, &format!("[${vector:04X}]"));
            }
        }
    }
    out.into_string()
}

fn registers(out: &mut Row, seq: u64, cycles: Option<u8>, regs: &Registers) {
    out.put(0, &format!("{seq:>10}"));
    if let Some(c) = cycles {
        out.put(CYC_COL, &format!("{c:>3}"));
    }
    out.put(A_COL, &format!("{:02X}", regs.a));
    out.put(B_COL, &format!("{:02X}", regs.b));
    out.put(X_COL, &format!("{:04X}", regs.x));
    out.put(Y_COL, &format!("{:04X}", regs.y));
    out.put(U_COL, &format!("{:04X}", regs.u));
    out.put(S_COL, &format!("{:04X}", regs.s));
    out.put(DP_COL, &format!("{:02X}", regs.dp));
    out.put(CC_COL, &format!("{:02X}", regs.cc.to_byte()));
    out.put(FLAGS_COL, &flags_field("EFHINZVC", regs.cc.to_byte()));
}

fn instruction(out: &mut Row, line: &DisassembledLine) {
    out.put(ADDR_COL, &format!("{:04X}", line.addr));
    out.put(CODE_COL, &hex_bytes(&line.raw_bytes));
    out.put(MNEMONIC_COL, &line.mnemonic.to_string());
    match &line.comment_text {
        Some(comment) => {
            out.put(OPERAND_COL, &format!("{:<9}", line.operand_text));
            out.put(COMMENT_COL, comment);
        }
        None if !line.operand_text.is_empty() => out.put(OPERAND_COL, &line.operand_text),
        None => {}
    }
}

/// Formats a verbose bus-op line: address, `RD`/`WR`, value, and comment.
fn bus_op_row(addr: u16, op: &str, value: u8) -> String {
    const VALUE_COL: usize = CODE_COL + 3;
    let mut row = Row::new();
    row.put(ADDR_COL, &format!("{addr:04X}"));
    row.put(CODE_COL, op);
    row.put(VALUE_COL, &format!("{value:02X}"));
    row.put(COMMENT_COL, &emma6k_core::text::immediate_comment(value));
    row.into_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interrupt_row_names_the_interrupt_and_its_vector() {
        let regs = Registers {
            a: 0,
            b: 0,
            dp: 0,
            cc: emma09::cpu::cc::ConditionCodes::from_byte(0),
            x: 0,
            y: 0,
            u: 0,
            s: 0xDF00,
            pc: 0xE012,
        };
        let row = TraceRow {
            instr_id: 41,
            regs,
            cycles: Some(19),
            line: None,
            bus_ops: vec![
                TraceBusOp::Write {
                    addr: 0xDEFF,
                    value: 0x12,
                },
                TraceBusOp::Read {
                    addr: 0xFFF8,
                    value: 0xF5,
                },
                TraceBusOp::Read {
                    addr: 0xFFF9,
                    value: 0x9D,
                },
            ],
        };
        let text = step_row(&row);
        assert!(text.starts_with("        42  19 00 00"), "{text}");
        assert_eq!(&text[ADDR_COL..ADDR_COL + 4], "E012");
        assert_eq!(&text[MNEMONIC_COL..], "<IRQ> [$FFF8]");
    }

    #[test]
    fn header_columns_line_up() {
        for (name, col) in [
            ("Cyc", CYC_COL),
            ("A ", A_COL),
            ("B ", B_COL),
            ("X ", X_COL),
            ("Y ", Y_COL),
            ("U ", U_COL),
            ("S ", S_COL),
            ("DP", DP_COL),
            ("CC", CC_COL),
            ("EFHINZVC", FLAGS_COL),
            ("Addr", ADDR_COL),
            ("Code", CODE_COL),
            ("Instruction", MNEMONIC_COL),
        ] {
            assert_eq!(&HEADER[col..col + name.len()], name, "{name}");
            assert_eq!(&SEPARATOR[col..col + 1], "-", "{name}");
        }
    }
}
