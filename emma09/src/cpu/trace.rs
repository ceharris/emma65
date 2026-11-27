//! The 6809's execution trace: its `Registers` record codec, and the core's
//! generic trace types specialized to the 6809's [`Registers`].
//!
//! The file format and writer thread live in [`emma6k_core::trace`]. The 6809
//! writes family 1 with 24-byte records; the register payload is `a`, `b`,
//! `dp`, `cc` (one byte each), then `x`, `y`, `u`, `s`, `pc` (`u16`,
//! little-endian like the rest of the file).

use super::Registers;
use super::cc::ConditionCodes;
use crate::config::CpuVariantSpec;
use emma6k_core::trace::{self as core_trace, TraceRegisters};

pub use emma6k_core::trace::{OverflowPolicy, TraceCallback, spawn_trace_writer};

/// The 6809's trace family code.
pub const FAMILY: u8 = 1;

const VARIANT_MC6809: u8 = 0;

/// The kind of event a [`TraceRecord`] carries, for the 6809.
pub type TraceKind = core_trace::TraceKind<Registers>;
/// A single recorded 6809 trace event.
pub type TraceRecord = core_trace::TraceRecord<Registers>;
/// Writes 6809 trace records to a binary trace file.
pub type BinaryTraceWriter<W> = core_trace::BinaryTraceWriter<W, Registers>;
/// Reads a 6809 binary trace file.
pub type BinaryTraceReader<R> = core_trace::BinaryTraceReader<R, Registers>;
/// A [`TraceCallback`] that hands 6809 records to a writer thread.
pub type ChannelTraceCallback = core_trace::ChannelTraceCallback<Registers>;

impl TraceRegisters for Registers {
    type Variant = CpuVariantSpec;
    const FAMILY: u8 = FAMILY;
    const RECORD_LEN: usize = 24;

    fn variant_to_byte(variant: CpuVariantSpec) -> u8 {
        match variant {
            CpuVariantSpec::Mc6809 => VARIANT_MC6809,
        }
    }

    fn variant_from_byte(byte: u8) -> Option<CpuVariantSpec> {
        match byte {
            VARIANT_MC6809 => Some(CpuVariantSpec::Mc6809),
            _ => None,
        }
    }

    fn encode(&self, payload: &mut [u8]) {
        payload[0] = self.a;
        payload[1] = self.b;
        payload[2] = self.dp;
        payload[3] = self.cc.to_byte();
        for (i, word) in [self.x, self.y, self.u, self.s, self.pc].iter().enumerate() {
            payload[4 + 2 * i..6 + 2 * i].copy_from_slice(&word.to_le_bytes());
        }
    }

    fn decode(payload: &[u8]) -> Self {
        let word = |i: usize| u16::from_le_bytes([payload[4 + 2 * i], payload[5 + 2 * i]]);
        Registers {
            a: payload[0],
            b: payload[1],
            dp: payload[2],
            cc: ConditionCodes::from_byte(payload[3]),
            x: word(0),
            y: word(1),
            u: word(2),
            s: word(3),
            pc: word(4),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emma6k_core::trace::HEADER_LEN;
    use std::io;

    fn sample_registers() -> Registers {
        Registers {
            a: 0x12,
            b: 0x34,
            dp: 0xDE,
            cc: ConditionCodes::E | ConditionCodes::I | ConditionCodes::Z,
            x: 0x1122,
            y: 0x3344,
            u: 0xDF00,
            s: 0xDEFF,
            pc: 0xF5B0,
        }
    }

    #[test]
    fn v3_round_trip() {
        let records = vec![
            TraceRecord {
                instr_id: 0,
                kind: TraceKind::Registers(sample_registers()),
            },
            TraceRecord {
                instr_id: 0,
                kind: TraceKind::Read {
                    addr: 0xF5B0,
                    value: 0x86,
                },
            },
            TraceRecord {
                instr_id: 0,
                kind: TraceKind::Write {
                    addr: 0xDEFE,
                    value: 0xB0,
                },
            },
            TraceRecord {
                instr_id: 0,
                kind: TraceKind::Cycles(19),
            },
        ];
        let mut buf = Vec::new();
        let mut writer = BinaryTraceWriter::new(&mut buf, CpuVariantSpec::Mc6809);
        for &rec in &records {
            writer.record(rec);
        }
        writer.flush().unwrap();
        drop(writer);

        assert_eq!(&buf[..HEADER_LEN], &[b'E', b'6', b'5', b'T', 3, 0, 1, 24]);
        assert_eq!(buf.len(), HEADER_LEN + 4 * 24);
        let regs = &buf[HEADER_LEN + 9..HEADER_LEN + 24];
        assert_eq!(
            regs,
            &[
                0x12, 0x34, 0xDE, 0x94, 0x22, 0x11, 0x44, 0x33, 0x00, 0xDF, 0xFF, 0xDE, 0xB0, 0xF5,
                0
            ]
        );

        let reader = BinaryTraceReader::new(buf.as_slice()).unwrap();
        assert_eq!(reader.variant(), CpuVariantSpec::Mc6809);
        let read: Vec<TraceRecord> = reader.collect::<io::Result<_>>().unwrap();
        assert_eq!(read, records);
    }

    #[test]
    fn a_6502_trace_is_refused() {
        let buf = [b'E', b'6', b'5', b'T', 3, 1, 0, 16];
        let err = BinaryTraceReader::new(buf.as_slice()).err().unwrap();
        assert!(err.to_string().contains("family 0"), "{err}");
    }
}
