//! Reconstructs instruction rows from a 6809 trace: groups each step's
//! records and recovers its disassembly from the opcode and operand fetches.

use super::{DisassembledLine, Disassembler, instruction_len};
use crate::cpu::Registers;
use crate::cpu::trace::{TraceKind, TraceRecord};
use emma6k_core::bus::SymbolTable;

/// The instruction being reconstructed from fetch reads.
struct Pending {
    instr_id: u64,
    addr: u16,
    raw_bytes: Vec<u8>,
}

/// Consumes 6809 [`TraceRecord`]s and reconstructs [`DisassembledLine`]s.
///
/// A read contributes to the instruction only when its address is the next
/// byte after those already collected, starting at the PC in the step's
/// `Registers` record. The CPU fetches every instruction byte before any
/// operand access, so pointer and data reads never land there before the
/// instruction is complete. Writes and `Cycles` records are ignored.
pub struct TraceDisassembler {
    disassembler: Disassembler,
    symbols: SymbolTable,
    pending: Option<Pending>,
}

impl TraceDisassembler {
    /// Creates a trace disassembler that resolves labels against `symbols`.
    pub fn new(symbols: SymbolTable) -> Self {
        Self {
            disassembler: Disassembler::new(),
            symbols,
            pending: None,
        }
    }

    /// Feeds one record. Returns a line once the instruction's bytes have all
    /// been fetched.
    pub fn feed(&mut self, rec: &TraceRecord) -> Option<DisassembledLine> {
        match rec.kind {
            TraceKind::Registers(regs) => {
                self.pending = Some(Pending {
                    instr_id: rec.instr_id,
                    addr: regs.pc,
                    raw_bytes: Vec::with_capacity(5),
                });
                None
            }
            TraceKind::Read { addr, value } => self.feed_read(rec.instr_id, addr, value),
            TraceKind::Write { .. } | TraceKind::Cycles(_) => None,
        }
    }

    fn feed_read(&mut self, instr_id: u64, addr: u16, value: u8) -> Option<DisassembledLine> {
        let pending = self.pending.as_mut()?;
        if instr_id != pending.instr_id {
            return None;
        }
        if addr != pending.addr.wrapping_add(pending.raw_bytes.len() as u16) {
            return None;
        }
        pending.raw_bytes.push(value);
        if instruction_len(&pending.raw_bytes) != Some(pending.raw_bytes.len()) {
            return None;
        }
        let pending = self.pending.take().unwrap();
        Some(
            self.disassembler
                .build_line(pending.addr, pending.raw_bytes, &self.symbols),
        )
    }
}

/// A bus operation observed during a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceBusOp {
    /// A bus read of `value` from `addr`.
    Read {
        /// The bus address read.
        addr: u16,
        /// The byte value read.
        value: u8,
    },
    /// A bus write of `value` to `addr`.
    Write {
        /// The bus address written.
        addr: u16,
        /// The byte value written.
        value: u8,
    },
}

/// One reconstructed step: an instruction, or a serviced interrupt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceRow {
    /// The id shared by every record this row was built from.
    pub instr_id: u64,
    /// The registers as they stood before the step.
    pub regs: Registers,
    /// The step's cycle count, if its `Cycles` record was seen.
    pub cycles: Option<u8>,
    /// The instruction's disassembly, or `None` for a step that fetched no
    /// instruction: a serviced interrupt (see [`vector`](Self::vector)).
    pub line: Option<DisassembledLine>,
    /// The step's bus reads and writes, in order.
    pub bus_ops: Vec<TraceBusOp>,
}

impl TraceRow {
    /// Returns `bus_ops` without the instruction's fetch reads, which are
    /// always its first reads.
    pub fn non_fetch_bus_ops(&self) -> impl Iterator<Item = &TraceBusOp> {
        let mut fetch_remaining = self.line.as_ref().map_or(0, |l| l.raw_bytes.len());
        self.bus_ops.iter().filter(move |op| match op {
            TraceBusOp::Read { .. } if fetch_remaining > 0 => {
                fetch_remaining -= 1;
                false
            }
            _ => true,
        })
    }

    /// For an interrupt row, the address of the vector the CPU read: its
    /// first read, after any stacking writes.
    pub fn vector(&self) -> Option<u16> {
        if self.line.is_some() {
            return None;
        }
        self.bus_ops.iter().find_map(|op| match *op {
            TraceBusOp::Read { addr, .. } => Some(addr),
            TraceBusOp::Write { .. } => None,
        })
    }
}

/// Names the interrupt whose vector is at `vector`: `NMI`, `FIRQ` or `IRQ`
/// (or `SWI`, `SWI2`, `SWI3`, `RESET` for the other vectors).
pub fn vector_name(vector: u16) -> Option<&'static str> {
    use crate::cpu::{
        FIRQ_VECTOR, IRQ_VECTOR, NMI_VECTOR, RESET_VECTOR, SWI_VECTOR, SWI2_VECTOR, SWI3_VECTOR,
    };
    match vector {
        NMI_VECTOR => Some("NMI"),
        FIRQ_VECTOR => Some("FIRQ"),
        IRQ_VECTOR => Some("IRQ"),
        SWI_VECTOR => Some("SWI"),
        SWI2_VECTOR => Some("SWI2"),
        SWI3_VECTOR => Some("SWI3"),
        RESET_VECTOR => Some("RESET"),
        _ => None,
    }
}

/// The records collected so far for one row.
struct PendingRow {
    instr_id: u64,
    regs: Registers,
    bus_ops: Vec<TraceBusOp>,
    line: Option<DisassembledLine>,
    cycles: Option<u8>,
}

/// Groups a 6809 record stream into [`TraceRow`]s, one per step. A row is
/// complete when its `Cycles` record, always a step's last, arrives.
pub struct TraceRowAssembler {
    disassembler: TraceDisassembler,
    pending: Option<PendingRow>,
}

impl TraceRowAssembler {
    /// Creates an assembler that resolves labels against `symbols`.
    pub fn new(symbols: SymbolTable) -> Self {
        Self {
            disassembler: TraceDisassembler::new(symbols),
            pending: None,
        }
    }

    /// Feeds one record. Returns a row once its `Cycles` record arrives.
    pub fn feed(&mut self, rec: &TraceRecord) -> Option<TraceRow> {
        let current =
            |p: &Option<PendingRow>| p.as_ref().is_some_and(|p| p.instr_id == rec.instr_id);
        match rec.kind {
            TraceKind::Registers(regs) => {
                // A new step; drop any unfinished one (it ended in an error).
                self.pending = Some(PendingRow {
                    instr_id: rec.instr_id,
                    regs,
                    bus_ops: Vec::new(),
                    line: None,
                    cycles: None,
                });
            }
            TraceKind::Read { addr, value } if current(&self.pending) => {
                if let Some(p) = self.pending.as_mut() {
                    p.bus_ops.push(TraceBusOp::Read { addr, value });
                }
            }
            TraceKind::Write { addr, value } if current(&self.pending) => {
                if let Some(p) = self.pending.as_mut() {
                    p.bus_ops.push(TraceBusOp::Write { addr, value });
                }
            }
            TraceKind::Cycles(cycles) if current(&self.pending) => {
                if let Some(p) = self.pending.as_mut() {
                    p.cycles = Some(cycles);
                }
            }
            _ => {}
        }

        if let Some(line) = self.disassembler.feed(rec)
            && let Some(p) = self.pending.as_mut()
            && p.instr_id == rec.instr_id
        {
            p.line = Some(line);
        }

        if matches!(rec.kind, TraceKind::Cycles(_)) && current(&self.pending) {
            return self.pending.take().map(Self::complete);
        }
        None
    }

    /// Returns the unfinished row, if any (a trace that ended mid-step).
    pub fn flush(&mut self) -> Option<TraceRow> {
        self.pending.take().map(Self::complete)
    }

    fn complete(p: PendingRow) -> TraceRow {
        TraceRow {
            instr_id: p.instr_id,
            regs: p.regs,
            cycles: p.cycles,
            line: p.line,
            bus_ops: p.bus_ops,
        }
    }
}

#[cfg(test)]
mod tests;
