use super::*;

/// A toy CPU's registers: two 16-bit registers and a flag byte, in 24-byte
/// records, so the tests exercise a record longer than v2's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToyRegisters {
    pc: u16,
    sp: u16,
    flags: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToyVariant {
    Plain,
    Fancy,
}

impl TraceRegisters for ToyRegisters {
    type Variant = ToyVariant;
    const FAMILY: u8 = 7;
    const RECORD_LEN: usize = 24;

    fn variant_to_byte(variant: ToyVariant) -> u8 {
        match variant {
            ToyVariant::Plain => 0,
            ToyVariant::Fancy => 1,
        }
    }

    fn variant_from_byte(byte: u8) -> Option<ToyVariant> {
        match byte {
            0 => Some(ToyVariant::Plain),
            1 => Some(ToyVariant::Fancy),
            _ => None,
        }
    }

    fn encode(&self, payload: &mut [u8]) {
        assert_eq!(payload.len(), 15);
        payload[0..2].copy_from_slice(&self.pc.to_le_bytes());
        payload[2..4].copy_from_slice(&self.sp.to_le_bytes());
        payload[14] = self.flags;
    }

    fn decode(payload: &[u8]) -> Self {
        Self {
            pc: u16::from_le_bytes([payload[0], payload[1]]),
            sp: u16::from_le_bytes([payload[2], payload[3]]),
            flags: payload[14],
        }
    }
}

/// A 16-byte-record family numbered 0, to read v2 files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct V2Registers(u8);

impl TraceRegisters for V2Registers {
    type Variant = u8;
    const FAMILY: u8 = 0;
    const RECORD_LEN: usize = 16;

    fn variant_to_byte(variant: u8) -> u8 {
        variant
    }

    fn variant_from_byte(byte: u8) -> Option<u8> {
        Some(byte)
    }

    fn encode(&self, payload: &mut [u8]) {
        payload[0] = self.0;
    }

    fn decode(payload: &[u8]) -> Self {
        Self(payload[0])
    }
}

const REGS: ToyRegisters = ToyRegisters {
    pc: 0xC000,
    sp: 0x01FD,
    flags: 0xA5,
};

fn record(instr_id: u64, kind: TraceKind<ToyRegisters>) -> TraceRecord<ToyRegisters> {
    TraceRecord { instr_id, kind }
}

fn write_all(records: &[TraceRecord<ToyRegisters>]) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut writer = BinaryTraceWriter::<_, ToyRegisters>::new(&mut buf, ToyVariant::Fancy);
    for &rec in records {
        writer.record(rec);
    }
    writer.flush().unwrap();
    drop(writer);
    buf
}

fn read_all(buf: &[u8]) -> Vec<TraceRecord<ToyRegisters>> {
    BinaryTraceReader::<_, ToyRegisters>::new(buf)
        .unwrap()
        .collect::<io::Result<Vec<_>>>()
        .unwrap()
}

#[test]
fn header_records_version_variant_family_and_record_length() {
    let buf = write_all(&[]);
    assert_eq!(buf, [b'E', b'6', b'5', b'T', 3, 1, 7, 24]);
}

#[test]
fn record_layout() {
    let buf = write_all(&[
        record(7, TraceKind::Registers(REGS)),
        record(
            7,
            TraceKind::Read {
                addr: 0x1234,
                value: 0xAB,
            },
        ),
        record(
            8,
            TraceKind::Write {
                addr: 0x5678,
                value: 0xCD,
            },
        ),
        record(8, TraceKind::Cycles(4)),
    ]);
    assert_eq!(buf.len(), HEADER_LEN + 4 * 24);
    let rec = |i: usize| &buf[HEADER_LEN + i * 24..HEADER_LEN + (i + 1) * 24];

    assert_eq!(&rec(0)[0..8], &7u64.to_le_bytes());
    assert_eq!(rec(0)[8], TAG_REGISTERS);
    assert_eq!(&rec(0)[9..13], &[0x00, 0xC0, 0xFD, 0x01]);
    assert_eq!(rec(0)[23], 0xA5);

    assert_eq!(rec(1)[8], TAG_READ);
    assert_eq!(&rec(1)[9..12], &[0x34, 0x12, 0xAB]);
    assert!(rec(1)[12..].iter().all(|&b| b == 0));

    assert_eq!(&rec(2)[0..8], &8u64.to_le_bytes());
    assert_eq!(rec(2)[8], TAG_WRITE);
    assert_eq!(&rec(2)[9..12], &[0x78, 0x56, 0xCD]);

    assert_eq!(rec(3)[8], TAG_CYCLES);
    assert_eq!(rec(3)[9], 4);
    assert!(rec(3)[10..].iter().all(|&b| b == 0));
}

#[test]
fn records_round_trip() {
    let records = vec![
        record(3, TraceKind::Registers(REGS)),
        record(
            3,
            TraceKind::Read {
                addr: 0x0300,
                value: 0x99,
            },
        ),
        record(
            3,
            TraceKind::Write {
                addr: 0x0301,
                value: 0x77,
            },
        ),
        record(3, TraceKind::Cycles(7)),
    ];
    assert_eq!(read_all(&write_all(&records)), records);
}

#[test]
fn reader_reports_the_header() {
    let buf = write_all(&[]);
    let reader = BinaryTraceReader::<_, ToyRegisters>::new(buf.as_slice()).unwrap();
    assert_eq!(reader.variant(), ToyVariant::Fancy);
    assert_eq!(
        reader.header(),
        TraceHeader {
            version: 3,
            family: 7,
            variant: 1,
            record_len: 24,
        }
    );
}

#[test]
fn reader_accepts_v2_as_family_zero_with_16_byte_records() {
    let mut buf = vec![b'E', b'6', b'5', b'T', 2, 1, 0, 0];
    let mut rec = [0u8; 16];
    rec[0] = 5;
    rec[8] = TAG_REGISTERS;
    rec[9] = 0x42;
    buf.extend_from_slice(&rec);

    let header = TraceHeader::parse(buf[..8].try_into().unwrap()).unwrap();
    assert_eq!(
        (header.version, header.family, header.record_len),
        (2, 0, 16)
    );
    let reader = BinaryTraceReader::<_, V2Registers>::new(buf.as_slice()).unwrap();
    assert_eq!(reader.variant(), 1);
    let records: Vec<_> = reader.collect::<io::Result<_>>().unwrap();
    assert_eq!(
        records,
        vec![TraceRecord {
            instr_id: 5,
            kind: TraceKind::Registers(V2Registers(0x42)),
        }]
    );
}

#[test]
fn reader_rejects_bad_magic() {
    let mut buf = write_all(&[]);
    buf[0..4].copy_from_slice(b"NOPE");
    assert!(BinaryTraceReader::<_, ToyRegisters>::new(buf.as_slice()).is_err());
}

#[test]
fn reader_rejects_unknown_versions() {
    for version in [1, 4] {
        let mut buf = write_all(&[]);
        buf[4] = version;
        let err = BinaryTraceReader::<_, ToyRegisters>::new(buf.as_slice())
            .err()
            .unwrap();
        assert!(err.to_string().contains("version"), "{err}");
    }
}

#[test]
fn reader_rejects_another_family() {
    let buf = write_all(&[]);
    let err = BinaryTraceReader::<_, V2Registers>::new(buf.as_slice())
        .err()
        .unwrap();
    assert!(err.to_string().contains("family 7"), "{err}");
}

#[test]
fn reader_rejects_unknown_variant_tag() {
    let mut buf = write_all(&[]);
    buf[5] = 0xFF;
    assert!(BinaryTraceReader::<_, ToyRegisters>::new(buf.as_slice()).is_err());
}

#[test]
fn header_rejects_unusable_record_lengths() {
    for len in [0, 11, 33] {
        let mut buf = write_all(&[]);
        buf[7] = len;
        assert!(TraceHeader::parse(buf[..8].try_into().unwrap()).is_err());
    }
}

#[test]
fn partial_record_at_eof_is_an_error() {
    let mut buf = write_all(&[record(1, TraceKind::Cycles(2))]);
    buf.truncate(buf.len() - 1);
    let mut reader = BinaryTraceReader::<_, ToyRegisters>::new(buf.as_slice()).unwrap();
    assert_eq!(
        reader.next().unwrap().unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
}

#[test]
fn unknown_tag_is_an_error() {
    let mut buf = write_all(&[record(1, TraceKind::Cycles(2))]);
    buf[HEADER_LEN + 8] = 9;
    let mut reader = BinaryTraceReader::<_, ToyRegisters>::new(buf.as_slice()).unwrap();
    assert!(reader.next().unwrap().is_err());
}

#[test]
fn seek_to_record_uses_the_family_record_length() {
    let records: Vec<_> = (0..5u8)
        .map(|i| record(i as u64, TraceKind::Cycles(i)))
        .collect();
    let mut reader =
        BinaryTraceReader::<_, ToyRegisters>::new(io::Cursor::new(write_all(&records))).unwrap();
    reader.seek_to_record(3).unwrap();
    let rest: Vec<_> = reader.collect::<io::Result<_>>().unwrap();
    assert_eq!(rest, records[3..]);

    let mut reader =
        BinaryTraceReader::<_, ToyRegisters>::new(io::Cursor::new(write_all(&records))).unwrap();
    reader.seek_to_record(0).unwrap();
    assert_eq!(reader.count(), 5);
}

#[test]
fn channel_writer_writes_and_flushes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.trace");
    let writer = BinaryTraceWriter::<_, ToyRegisters>::new(
        std::fs::File::create(&path).unwrap(),
        ToyVariant::Plain,
    );
    let (mut callback, handle, dropped) =
        spawn_trace_writer(writer, 16, OverflowPolicy::BlockOnFull);
    callback.record(record(0, TraceKind::Registers(REGS)));
    callback.record(record(0, TraceKind::Cycles(3)));
    callback.flush();
    assert_eq!(std::fs::read(&path).unwrap().len(), HEADER_LEN + 2 * 24);
    drop(callback);
    handle.join().unwrap();
    assert_eq!(dropped.load(Ordering::Relaxed), 0);
    assert_eq!(read_all(&std::fs::read(&path).unwrap()).len(), 2);
}

#[test]
fn begin_instruction_advances_id_and_sets_pending() {
    let mut state = TraceState::new();
    state.begin_instruction(REGS);
    let id1 = state.current_instr_id();
    state.begin_instruction(REGS);
    let id2 = state.current_instr_id();
    assert_eq!(id2, id1 + 1);
    assert_eq!(state.take_pending_registers(id2), Some(REGS));
}

#[test]
fn take_pending_registers_returns_none_for_stale_id() {
    let mut state = TraceState::new();
    state.begin_instruction(REGS);
    let id = state.current_instr_id();
    // Wrong id: pending snapshot for `id` must remain untouched.
    assert_eq!(state.take_pending_registers(id.wrapping_add(1)), None);
    assert_eq!(state.take_pending_registers(id), Some(REGS));
    // Already emitted: taking again returns None.
    assert_eq!(state.take_pending_registers(id), None);
}
