//! The 65C02's execution trace: its `Registers` record codec, and the core's
//! generic trace types specialized to the 65C02's [`Registers`].
//!
//! The file format and writer thread live in [`emma6k_core::trace`]. The
//! 65C02 writes format v3 with family 0 and 16-byte records, so its records
//! are byte-for-byte the same as format v2's, and v2 files still read.
use super::Registers;
use super::status::StatusRegister;
use super::variant::CpuVariant;
use emma6k_core::trace::{self as core_trace, TraceRegisters};

pub use emma6k_core::trace::{OverflowPolicy, TraceCallback, TraceHeader, spawn_trace_writer};

/// The 65C02's trace family code.
pub const FAMILY: u8 = 0;

const VARIANT_CMOS_65C02: u8 = 0;
const VARIANT_WDC_65C02: u8 = 1;

/// The kind of event a [`TraceRecord`] carries, for the 65C02.
pub type TraceKind = core_trace::TraceKind<Registers>;
/// A single recorded 65C02 trace event.
pub type TraceRecord = core_trace::TraceRecord<Registers>;
/// Writes 65C02 trace records to a binary trace file.
pub type BinaryTraceWriter<W> = core_trace::BinaryTraceWriter<W, Registers>;
/// Reads a 65C02 binary trace file (format v2 or v3).
pub type BinaryTraceReader<R> = core_trace::BinaryTraceReader<R, Registers>;
/// A [`TraceCallback`] that hands 65C02 records to a writer thread.
pub type ChannelTraceCallback = core_trace::ChannelTraceCallback<Registers>;
/// The 65C02's per-step trace bookkeeping.
pub(in crate::mos6502) type TraceState = core_trace::TraceState<Registers>;

/// The `Registers` payload: `a`, `x`, `y`, `s` (one byte each), `pc` (u16,
/// little-endian), `p` (u8), in a 16-byte record.
impl TraceRegisters for Registers {
    type Variant = CpuVariant;
    const FAMILY: u8 = FAMILY;
    const RECORD_LEN: usize = 16;

    fn variant_to_byte(variant: CpuVariant) -> u8 {
        match variant {
            CpuVariant::Cmos65C02 => VARIANT_CMOS_65C02,
            CpuVariant::Wdc65C02 => VARIANT_WDC_65C02,
        }
    }

    fn variant_from_byte(byte: u8) -> Option<CpuVariant> {
        match byte {
            VARIANT_CMOS_65C02 => Some(CpuVariant::Cmos65C02),
            VARIANT_WDC_65C02 => Some(CpuVariant::Wdc65C02),
            _ => None,
        }
    }

    fn encode(&self, payload: &mut [u8]) {
        payload[0] = self.a;
        payload[1] = self.x;
        payload[2] = self.y;
        payload[3] = self.s;
        payload[4..6].copy_from_slice(&self.pc.to_le_bytes());
        payload[6] = self.p.to_byte();
    }

    fn decode(payload: &[u8]) -> Self {
        Registers {
            a: payload[0],
            x: payload[1],
            y: payload[2],
            s: payload[3],
            pc: u16::from_le_bytes([payload[4], payload[5]]),
            p: StatusRegister::from_byte(payload[6]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emma6k_core::trace::HEADER_LEN;
    use std::io;

    const RECORD_LEN: usize = 16;

    fn sample_registers() -> Registers {
        Registers {
            a: 0x11,
            x: 0x22,
            y: 0x33,
            s: 0xFD,
            pc: 0xC000,
            p: StatusRegister::UNUSED | StatusRegister::N,
        }
    }

    fn sample_records() -> Vec<TraceRecord> {
        vec![
            TraceRecord {
                instr_id: 7,
                kind: TraceKind::Registers(sample_registers()),
            },
            TraceRecord {
                instr_id: 7,
                kind: TraceKind::Read {
                    addr: 0x1234,
                    value: 0xAB,
                },
            },
            TraceRecord {
                instr_id: 8,
                kind: TraceKind::Write {
                    addr: 0x5678,
                    value: 0xCD,
                },
            },
            TraceRecord {
                instr_id: 8,
                kind: TraceKind::Cycles(4),
            },
        ]
    }

    fn write(records: &[TraceRecord]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut writer = BinaryTraceWriter::new(&mut buf, CpuVariant::Wdc65C02);
        for &rec in records {
            writer.record(rec);
        }
        writer.flush().unwrap();
        drop(writer);
        buf
    }

    /// The exact bytes the 6502 wrote under format v2: v3 changes only
    /// header bytes 4, 6 and 7.
    fn v2_record_bytes() -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut rec = |id: u64, tail: &[u8]| {
            bytes.extend_from_slice(&id.to_le_bytes());
            let mut rest = [0u8; 8];
            rest[..tail.len()].copy_from_slice(tail);
            bytes.extend_from_slice(&rest);
        };
        let p = (StatusRegister::UNUSED | StatusRegister::N).to_byte();
        rec(7, &[0, 0x11, 0x22, 0x33, 0xFD, 0x00, 0xC0, p]);
        rec(7, &[1, 0x34, 0x12, 0xAB]);
        rec(8, &[2, 0x78, 0x56, 0xCD]);
        rec(8, &[3, 4]);
        bytes
    }

    #[test]
    fn writes_v3_with_family_0_and_v2_record_bytes() {
        let buf = write(&sample_records());
        assert_eq!(&buf[..HEADER_LEN], &[b'E', b'6', b'5', b'T', 3, 1, 0, 16]);
        assert_eq!(&buf[HEADER_LEN..], v2_record_bytes().as_slice());
        assert_eq!(buf.len(), HEADER_LEN + 4 * RECORD_LEN);
    }

    #[test]
    fn v3_round_trip() {
        let buf = write(&sample_records());
        let reader = BinaryTraceReader::new(buf.as_slice()).unwrap();
        assert_eq!(reader.variant(), CpuVariant::Wdc65C02);
        let records: Vec<TraceRecord> = reader.collect::<io::Result<_>>().unwrap();
        assert_eq!(records, sample_records());
    }

    #[test]
    fn v2_files_still_decode() {
        let mut buf = vec![b'E', b'6', b'5', b'T', 2, 0, 0, 0];
        buf.extend_from_slice(&v2_record_bytes());
        let reader = BinaryTraceReader::new(buf.as_slice()).unwrap();
        assert_eq!(reader.variant(), CpuVariant::Cmos65C02);
        assert_eq!(reader.header().version, 2);
        let records: Vec<TraceRecord> = reader.collect::<io::Result<_>>().unwrap();
        assert_eq!(records, sample_records());
    }

    #[test]
    fn reader_rejects_unknown_variant_tag() {
        let mut buf = write(&[]);
        buf[5] = 0xFF;
        assert!(BinaryTraceReader::new(buf.as_slice()).is_err());
    }

    #[test]
    fn seek_to_record_jumps_to_arbitrary_record() {
        let buf = write(&sample_records());
        let mut reader = BinaryTraceReader::new(io::Cursor::new(buf)).unwrap();
        reader.seek_to_record(2).unwrap();
        let records: Vec<TraceRecord> = reader.collect::<io::Result<_>>().unwrap();
        assert_eq!(records, sample_records()[2..]);
    }
}
