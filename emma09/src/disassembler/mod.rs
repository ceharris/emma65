//! The 6809 disassembler: decodes bus memory, or bytes recovered from a
//! trace, into instruction listings in Motorola syntax.
//!
//! Operand forms:
//!
//! | Mode | Example |
//! |---|---|
//! | inherent | `NOP` |
//! | immediate | `LDA #$41`, `LDX #$1234` |
//! | direct | `LDA <$20` (DP supplies the high byte) |
//! | extended | `JMP $E000`, or a label |
//! | relative | `BNE $E010`, or a label: the branch target |
//! | indexed | `LDA 5,X`… see below |
//! | register pair | `TFR X,Y` |
//! | register list | `PSHS CC,A,B,X` |
//!
//! Indexed forms: `$05,X` / `-$05,X` (5- and 8-bit offsets), `$1234,X` or a
//! label (16-bit), `,X+`, `,X++`, `,-X`, `,--X`, `,X`, `A,X`, `B,X`, `D,X`,
//! `target,PCR` (the effective address, or a label), `[$1234]`, and the
//! indirect forms in brackets. An undefined postbyte disassembles as an
//! invalid instruction of opcode-plus-postbyte length, matching how the CPU
//! skips it.

pub mod trace;

pub use trace::{TraceBusOp, TraceDisassembler, TraceRow, TraceRowAssembler};

use crate::cpu::indexed;
use crate::cpu::opcodes::{AddressingMode, DecodedOp, Mnemonic, Page};
use emma6k_core::bus::{Bus, SymbolTable};

/// A single disassembled instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisassembledLine {
    /// Address of the first byte of the instruction.
    pub addr: u16,
    /// The raw instruction bytes (1–5), prefix included.
    pub raw_bytes: Vec<u8>,
    /// Labels for `addr` from the symbol table.
    pub labels: Vec<String>,
    /// The instruction mnemonic.
    pub mnemonic: Mnemonic,
    /// Formatted operand text (empty for inherent instructions).
    pub operand_text: String,
    /// An optional comment (alternate forms of an 8-bit immediate operand).
    pub comment_text: Option<String>,
    /// False for an undocumented opcode or an undefined indexed postbyte.
    pub is_valid: bool,
}

/// Decodes the opcode (and any prefix) at the start of `bytes`.
fn decode(bytes: &[u8]) -> Option<DecodedOp> {
    let first = *bytes.first()?;
    match first {
        0x10 => Some(DecodedOp::new(Page::Page2, *bytes.get(1)?)),
        0x11 => Some(DecodedOp::new(Page::Page3, *bytes.get(1)?)),
        _ => Some(DecodedOp::new(Page::Page1, first)),
    }
}

/// Returns the length of the instruction that `bytes` begins, or `None` if
/// more bytes are needed to tell: the prefix's opcode, or an indexed
/// postbyte, hasn't been seen yet.
pub fn instruction_len(bytes: &[u8]) -> Option<usize> {
    let decoded = decode(bytes)?;
    let opcode_len = decoded.page.prefix_len() as usize + 1;
    if decoded.is_invalid() {
        return Some(opcode_len);
    }
    let base = decoded.base_len() as usize;
    if decoded.mode != AddressingMode::Indexed {
        return Some(base);
    }
    let postbyte = *bytes.get(opcode_len)?;
    if indexed::is_defined(postbyte) {
        Some(base + indexed::extra_bytes(postbyte) as usize)
    } else {
        Some(base)
    }
}

/// Disassembles 6809 instructions from a [`Bus`], using side-effect-free
/// `peek` reads, or from already-collected bytes.
#[derive(Debug, Default, Clone, Copy)]
pub struct Disassembler;

impl Disassembler {
    /// Creates a disassembler.
    pub fn new() -> Self {
        Self
    }

    /// Peeks the bytes of the instruction at `addr` off `bus`.
    fn collect_bytes(&self, bus: &Bus, addr: u16) -> Vec<u8> {
        let mut raw = Vec::with_capacity(5);
        loop {
            if let Some(len) = instruction_len(&raw)
                && raw.len() >= len
            {
                return raw;
            }
            let at = addr.wrapping_add(raw.len() as u16);
            raw.push(bus.peek(at).unwrap_or(0xFF));
        }
    }

    /// Builds a line from an instruction's complete bytes (as
    /// [`instruction_len`] measures them). Pure: no bus access.
    pub fn build_line(
        &self,
        addr: u16,
        raw_bytes: Vec<u8>,
        symbols: &SymbolTable,
    ) -> DisassembledLine {
        let labels = symbols.names_for(addr).map(str::to_string).collect();
        let decoded = decode(&raw_bytes).expect("build_line needs the opcode bytes");
        let operand_at = decoded.page.prefix_len() as usize + 1;
        let next = addr.wrapping_add(raw_bytes.len() as u16);
        let operand = &raw_bytes[operand_at.min(raw_bytes.len())..];
        let byte = |i: usize| operand.get(i).copied().unwrap_or(0);
        let word = |i: usize| u16::from_be_bytes([byte(i), byte(i + 1)]);

        let mut is_valid = !decoded.is_invalid();
        let mut comment_text = None;
        let operand_text = if !is_valid {
            String::new()
        } else {
            match decoded.mode {
                AddressingMode::Inherent => String::new(),
                AddressingMode::Immediate8 => {
                    comment_text = Some(emma6k_core::text::immediate_comment(byte(0)));
                    format!("#${:02X}", byte(0))
                }
                AddressingMode::Immediate16 => format!("#${:04X}", word(0)),
                AddressingMode::Direct => format!("<${:02X}", byte(0)),
                AddressingMode::Extended => address(word(0), symbols),
                AddressingMode::Relative8 => {
                    address(next.wrapping_add(byte(0) as i8 as u16), symbols)
                }
                AddressingMode::Relative16 => address(next.wrapping_add(word(0)), symbols),
                AddressingMode::RegisterPair => format!(
                    "{},{}",
                    pair_register(byte(0) >> 4),
                    pair_register(byte(0) & 0x0F)
                ),
                AddressingMode::RegisterList => register_list(
                    byte(0),
                    matches!(decoded.mnemonic, Mnemonic::Pshu | Mnemonic::Pulu),
                ),
                AddressingMode::Indexed => match format_indexed(operand, next, symbols) {
                    Some(text) => text,
                    None => {
                        is_valid = false;
                        String::new()
                    }
                },
            }
        };
        DisassembledLine {
            addr,
            raw_bytes,
            labels,
            mnemonic: if is_valid {
                decoded.mnemonic
            } else {
                Mnemonic::Invalid
            },
            operand_text,
            comment_text,
            is_valid,
        }
    }

    /// Disassembles the instruction at `addr`.
    pub fn disassemble_one(&self, bus: &Bus, addr: u16) -> DisassembledLine {
        let raw_bytes = self.collect_bytes(bus, addr);
        self.build_line(addr, raw_bytes, bus.symbol_table())
    }

    /// Disassembles up to `max` instructions starting at `start`, stopping
    /// before `end` (when `end` is above `start`) or on wrapping past `$FFFF`.
    pub fn disassemble_range(
        &self,
        bus: &Bus,
        start: u16,
        end: u16,
        max: usize,
    ) -> Vec<DisassembledLine> {
        let mut result = Vec::new();
        let mut addr = start;
        while result.len() < max {
            if end > start && addr >= end {
                break;
            }
            let line = self.disassemble_one(bus, addr);
            let next = addr.wrapping_add(line.raw_bytes.len() as u16);
            result.push(line);
            if next <= addr {
                break;
            }
            addr = next;
        }
        result
    }
}

/// Formats an address as its first label, or `$XXXX`.
fn address(addr: u16, symbols: &SymbolTable) -> String {
    match symbols.names_for(addr).next() {
        Some(name) => name.to_string(),
        None => format!("${addr:04X}"),
    }
}

/// Formats a signed offset in hex: `$05`, `-$05`.
fn signed(offset: i32) -> String {
    if offset < 0 {
        format!("-${:02X}", -offset)
    } else {
        format!("${offset:02X}")
    }
}

/// Names a TFR/EXG register code.
fn pair_register(code: u8) -> &'static str {
    match code {
        0x0 => "D",
        0x1 => "X",
        0x2 => "Y",
        0x3 => "U",
        0x4 => "S",
        0x5 => "PC",
        0x8 => "A",
        0x9 => "B",
        0xA => "CC",
        0xB => "DP",
        _ => "?",
    }
}

/// Formats a PSH/PUL register list, in stacking order from the lowest bit.
/// Bit 6 names the other stack: U for PSHS/PULS, S for PSHU/PULU.
fn register_list(postbyte: u8, user_stack: bool) -> String {
    let names = [
        "CC",
        "A",
        "B",
        "DP",
        "X",
        "Y",
        if user_stack { "S" } else { "U" },
        "PC",
    ];
    if postbyte == 0 {
        return "#$00".to_string();
    }
    names
        .iter()
        .enumerate()
        .filter(|&(bit, _)| postbyte & (1 << bit) != 0)
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join(",")
}

/// Formats an indexed operand: `operand` starts at the postbyte, and `next`
/// is the address after the instruction (the base for PCR offsets). Returns
/// `None` for an undefined postbyte.
fn format_indexed(operand: &[u8], next: u16, symbols: &SymbolTable) -> Option<String> {
    let postbyte = *operand.first()?;
    if !indexed::is_defined(postbyte) {
        return None;
    }
    let reg = ["X", "Y", "U", "S"][(postbyte >> 5 & 0x03) as usize];
    let byte = operand.get(1).copied().unwrap_or(0);
    let word = u16::from_be_bytes([byte, operand.get(2).copied().unwrap_or(0)]);
    if postbyte & indexed::OFFSET5 == 0 {
        // Sign-extend the low five bits.
        let offset = ((postbyte & 0x1F) as i8) << 3 >> 3;
        return Some(format!("{},{reg}", signed(offset as i32)));
    }
    let text = match postbyte & 0x0F {
        0x0 => format!(",{reg}+"),
        0x1 => format!(",{reg}++"),
        0x2 => format!(",-{reg}"),
        0x3 => format!(",--{reg}"),
        0x4 => format!(",{reg}"),
        0x5 => format!("B,{reg}"),
        0x6 => format!("A,{reg}"),
        0x8 => format!("{},{reg}", signed(byte as i8 as i32)),
        0x9 => format!("{},{reg}", address(word, symbols)),
        0xB => format!("D,{reg}"),
        0xC => format!(
            "{},PCR",
            address(next.wrapping_add(byte as i8 as u16), symbols)
        ),
        0xD => format!("{},PCR", address(next.wrapping_add(word), symbols)),
        0xF => address(word, symbols),
        _ => unreachable!("is_defined rejects the other forms"),
    };
    Some(if postbyte & indexed::INDIRECT != 0 {
        format!("[{text}]")
    } else {
        text
    })
}

#[cfg(test)]
mod tests;
