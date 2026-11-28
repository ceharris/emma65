//! Execution tracing for any CPU: the record model, the callback a CPU feeds,
//! the binary trace file format, and a writer thread.
//!
//! Everything here is generic over a CPU's register snapshot, which implements
//! [`TraceRegisters`] to supply its family code, record length, and the
//! encoding of its `Registers` record. `Read`, `Write`, and `Cycles` records
//! have the same layout for every CPU.
//!
//! # File format
//!
//! An 8-byte header, then fixed-length records:
//!
//! | Header byte | Meaning |
//! |---|---|
//! | 0–3 | magic, `b"E65T"` |
//! | 4 | format version: 3 (2 is still read) |
//! | 5 | CPU variant, numbered by the CPU family |
//! | 6 | CPU family (v2: always 0, the 6502) |
//! | 7 | record length in bytes (v2: always 16) |
//!
//! Each record, little-endian: bytes 0–7 the instruction id (`u64`), byte 8 the
//! tag (0 `Registers`, 1 `Read`, 2 `Write`, 3 `Cycles`), then a zero-padded
//! payload filling the rest of the record. `Read` and `Write` carry the
//! address (`u16`) and value; `Cycles` carries the cycle count (`u8`);
//! `Registers` carries whatever the CPU's [`TraceRegisters::encode`] writes.
//!
//! A v2 file is a v3 file with family 0 and 16-byte records whose header bytes
//! 6 and 7 are zero, so the 6502's records are byte-for-byte the same in both.

use std::fmt::Debug;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The 4-byte magic that starts every trace file.
pub const MAGIC: [u8; 4] = *b"E65T";
/// The format version this crate writes.
pub const FORMAT_VERSION: u8 = 3;
/// The oldest format version this crate reads.
pub const OLDEST_FORMAT_VERSION: u8 = 2;
/// Size in bytes of the file header.
pub const HEADER_LEN: usize = 8;
/// Offset of the payload within a record: after the id and the tag.
pub const PAYLOAD_OFFSET: usize = 9;
/// The longest record any CPU may declare.
pub const MAX_RECORD_LEN: usize = 32;

/// The family a v2 file implicitly belongs to (it predates the family byte).
const V2_FAMILY: u8 = 0;
/// The record length of a v2 file.
const V2_RECORD_LEN: usize = 16;

const TAG_REGISTERS: u8 = 0;
const TAG_READ: u8 = 1;
const TAG_WRITE: u8 = 2;
const TAG_CYCLES: u8 = 3;

/// A CPU's register snapshot, as carried in a trace's `Registers` records.
///
/// Each CPU family implements this on its register type and picks a family
/// code no other family uses.
pub trait TraceRegisters: Copy + Debug + PartialEq + Eq + Send + 'static {
    /// The CPU variant type the header records.
    type Variant: Copy + Debug + PartialEq + Eq + Send + 'static;

    /// The family code in the header.
    const FAMILY: u8;

    /// The length of every record in this family's files, from
    /// `PAYLOAD_OFFSET + 3` up to [`MAX_RECORD_LEN`].
    const RECORD_LEN: usize;

    /// The header byte for `variant`.
    fn variant_to_byte(variant: Self::Variant) -> u8;

    /// The variant a header byte names, or `None` if it names none.
    fn variant_from_byte(byte: u8) -> Option<Self::Variant>;

    /// Writes the snapshot into `payload`, which is `RECORD_LEN -
    /// PAYLOAD_OFFSET` zeroed bytes.
    fn encode(&self, payload: &mut [u8]);

    /// Reads a snapshot back from a payload that [`encode`](Self::encode)
    /// wrote.
    fn decode(payload: &[u8]) -> Self;
}

/// The kind of event a [`TraceRecord`] carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceKind<R> {
    /// Register snapshot taken immediately before this instruction executed.
    Registers(R),
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
    /// The total clock cycle count for the instruction, known only once it
    /// has finished executing (base cycles plus any addressing-mode or
    /// branch-taken extra cycles). Emitted once per instruction, after its
    /// last `Read`/`Write` record.
    Cycles(u8),
}

/// A single recorded trace event: either a pre-instruction register snapshot
/// or a bus read/write, tagged with the instruction it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceRecord<R> {
    /// Monotonically increasing id shared by every record (the `Registers`
    /// snapshot plus all bus ops) belonging to the same CPU step.
    pub instr_id: u64,
    /// The event this record carries.
    pub kind: TraceKind<R>,
}

impl<R: TraceRegisters> TraceRecord<R> {
    /// Encodes the record into the first `R::RECORD_LEN` bytes of `buf`, which
    /// must be zeroed.
    fn encode(self, buf: &mut [u8; MAX_RECORD_LEN]) {
        buf[0..8].copy_from_slice(&self.instr_id.to_le_bytes());
        match self.kind {
            TraceKind::Registers(regs) => {
                buf[8] = TAG_REGISTERS;
                regs.encode(&mut buf[PAYLOAD_OFFSET..R::RECORD_LEN]);
            }
            TraceKind::Read { addr, value } => {
                buf[8] = TAG_READ;
                buf[9..11].copy_from_slice(&addr.to_le_bytes());
                buf[11] = value;
            }
            TraceKind::Write { addr, value } => {
                buf[8] = TAG_WRITE;
                buf[9..11].copy_from_slice(&addr.to_le_bytes());
                buf[11] = value;
            }
            TraceKind::Cycles(cycles) => {
                buf[8] = TAG_CYCLES;
                buf[9] = cycles;
            }
        }
    }

    /// Decodes one `R::RECORD_LEN`-byte record.
    fn decode(buf: &[u8]) -> io::Result<Self> {
        let instr_id = u64::from_le_bytes(buf[0..8].try_into().unwrap());
        let kind = match buf[8] {
            TAG_REGISTERS => TraceKind::Registers(R::decode(&buf[PAYLOAD_OFFSET..])),
            TAG_READ => TraceKind::Read {
                addr: u16::from_le_bytes([buf[9], buf[10]]),
                value: buf[11],
            },
            TAG_WRITE => TraceKind::Write {
                addr: u16::from_le_bytes([buf[9], buf[10]]),
                value: buf[11],
            },
            TAG_CYCLES => TraceKind::Cycles(buf[9]),
            tag => {
                return Err(invalid_data(format!("unknown trace record tag: {tag}")));
            }
        };
        Ok(TraceRecord { instr_id, kind })
    }
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Receives trace records as a CPU generates them.
pub trait TraceCallback<R>: Send {
    /// Called once for each `Registers` snapshot, each bus read or write, and
    /// each instruction's `Cycles`. Never called for `peek`.
    fn record(&mut self, rec: TraceRecord<R>);

    /// Makes all records recorded so far visible to an independent reader of
    /// the trace stream (e.g. a live viewer polling the file). Default is a
    /// no-op; implementations backed by buffered or asynchronous I/O should
    /// override this to block until pending writes are durable.
    fn flush(&mut self) {}
}

/// A trace file's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceHeader {
    /// The format version (2 or 3).
    pub version: u8,
    /// The CPU family code ([`TraceRegisters::FAMILY`]).
    pub family: u8,
    /// The CPU variant byte, numbered by the family.
    pub variant: u8,
    /// The length of each record in bytes.
    pub record_len: usize,
}

impl TraceHeader {
    /// Reads and validates a header: the magic, a known version, and a
    /// usable record length. Doesn't check the family or variant.
    pub fn read(reader: &mut impl Read) -> io::Result<Self> {
        let mut bytes = [0u8; HEADER_LEN];
        reader.read_exact(&mut bytes)?;
        Self::parse(&bytes)
    }

    /// Parses and validates header bytes, as [`read`](Self::read) does.
    pub fn parse(bytes: &[u8; HEADER_LEN]) -> io::Result<Self> {
        if bytes[0..4] != MAGIC {
            return Err(invalid_data("bad trace file magic"));
        }
        let version = bytes[4];
        let (family, record_len) = match version {
            2 => (V2_FAMILY, V2_RECORD_LEN),
            3 => (bytes[6], bytes[7] as usize),
            _ => {
                return Err(invalid_data(format!(
                    "unsupported trace file format version: {version}"
                )));
            }
        };
        if !(PAYLOAD_OFFSET + 3..=MAX_RECORD_LEN).contains(&record_len) {
            return Err(invalid_data(format!(
                "invalid trace record length: {record_len}"
            )));
        }
        Ok(Self {
            version,
            family,
            variant: bytes[5],
            record_len,
        })
    }

    /// Encodes a current-version header for family `R` and `variant`.
    fn for_registers<R: TraceRegisters>(variant: R::Variant) -> [u8; HEADER_LEN] {
        let mut header = [0u8; HEADER_LEN];
        header[0..4].copy_from_slice(&MAGIC);
        header[4] = FORMAT_VERSION;
        header[5] = R::variant_to_byte(variant);
        header[6] = R::FAMILY;
        header[7] = R::RECORD_LEN as u8;
        header
    }
}

/// Writes trace records to a binary trace file (see the [module docs](self)).
pub struct BinaryTraceWriter<W: Write, R: TraceRegisters> {
    writer: BufWriter<W>,
    _registers: std::marker::PhantomData<fn(R)>,
}

impl<W: Write, R: TraceRegisters> BinaryTraceWriter<W, R> {
    /// Creates a new writer wrapping `inner`, writing the file header
    /// (including `variant`, the CPU variant that produced this trace)
    /// immediately.
    pub fn new(inner: W, variant: R::Variant) -> Self {
        let mut writer = BufWriter::new(inner);
        // Best-effort write; errors are silently swallowed to avoid disrupting emulation.
        let _ = writer.write_all(&TraceHeader::for_registers::<R>(variant));
        Self {
            writer,
            _registers: std::marker::PhantomData,
        }
    }

    /// Flushes buffered records to the underlying writer.
    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl<W: Write + Send, R: TraceRegisters> TraceCallback<R> for BinaryTraceWriter<W, R> {
    fn record(&mut self, rec: TraceRecord<R>) {
        let mut buf = [0u8; MAX_RECORD_LEN];
        rec.encode(&mut buf);
        // Best-effort write; errors are silently swallowed to avoid disrupting emulation.
        let _ = self.writer.write_all(&buf[..R::RECORD_LEN]);
    }

    fn flush(&mut self) {
        // Best-effort; errors are silently swallowed to avoid disrupting emulation.
        let _ = BinaryTraceWriter::flush(self);
    }
}

/// Reads a binary trace file written by [`BinaryTraceWriter`] for family `R`.
pub struct BinaryTraceReader<In: Read, R: TraceRegisters> {
    reader: BufReader<In>,
    header: TraceHeader,
    variant: R::Variant,
    buf: [u8; MAX_RECORD_LEN],
}

impl<In: Read, R: TraceRegisters> BinaryTraceReader<In, R> {
    /// Creates a new reader over `inner`, validating the file header eagerly.
    /// Returns an error if the magic or format version is unknown, or if the
    /// file was recorded by another CPU family or an unknown variant.
    pub fn new(inner: In) -> io::Result<Self> {
        let mut reader = BufReader::new(inner);
        let header = TraceHeader::read(&mut reader)?;
        if header.family != R::FAMILY {
            return Err(invalid_data(format!(
                "trace was recorded for CPU family {}, not {}",
                header.family,
                R::FAMILY
            )));
        }
        if header.record_len != R::RECORD_LEN {
            return Err(invalid_data(format!(
                "trace record length {} doesn't match the CPU family's {}",
                header.record_len,
                R::RECORD_LEN
            )));
        }
        let variant = R::variant_from_byte(header.variant)
            .ok_or_else(|| invalid_data(format!("unknown CPU variant tag: {}", header.variant)))?;
        Ok(Self {
            reader,
            header,
            variant,
            buf: [0u8; MAX_RECORD_LEN],
        })
    }

    /// The CPU variant that produced this trace, as recorded in the file header.
    pub fn variant(&self) -> R::Variant {
        self.variant
    }

    /// The file's header.
    pub fn header(&self) -> TraceHeader {
        self.header
    }
}

impl<In: Read + Seek, R: TraceRegisters> BinaryTraceReader<In, R> {
    /// Seeks so that the next `next()` call yields the record at `record_index`
    /// (0-based, counting from the first record after the header), without
    /// scanning the file from the start.
    pub fn seek_to_record(&mut self, record_index: u64) -> io::Result<()> {
        let offset = HEADER_LEN as u64 + record_index * R::RECORD_LEN as u64;
        self.reader.seek(SeekFrom::Start(offset))?;
        Ok(())
    }
}

impl<In: Read, R: TraceRegisters> Iterator for BinaryTraceReader<In, R> {
    type Item = io::Result<TraceRecord<R>>;

    fn next(&mut self) -> Option<Self::Item> {
        let buf = &mut self.buf[..R::RECORD_LEN];
        let mut read = 0;
        while read < buf.len() {
            match self.reader.read(&mut buf[read..]) {
                Ok(0) if read == 0 => return None,
                Ok(0) => {
                    return Some(Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "partial trace record at EOF",
                    )));
                }
                Ok(n) => read += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Some(Err(e)),
            }
        }
        Some(TraceRecord::decode(buf))
    }
}

/// Overflow behavior when the trace writer thread can't keep up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowPolicy {
    /// Drop the record and count it. Never stalls the CPU thread.
    DropOnFull,
    /// Block until the writer thread makes room. Guarantees no lost records,
    /// at the cost of possibly stalling the CPU thread under sustained write pressure.
    BlockOnFull,
}

/// A message sent from the CPU thread to the trace writer thread: either a
/// record to persist, or a request to flush everything written so far, with
/// an ack channel the writer signals once the flush completes.
enum WriterMsg<R> {
    /// A trace record to write.
    Record(TraceRecord<R>),
    /// A request to flush buffered writes; the writer sends `()` on this
    /// channel once the flush completes, after draining every `Record`
    /// enqueued ahead of it.
    Flush(std::sync::mpsc::Sender<()>),
}

/// A [`TraceCallback`] that hands records off to a writer thread over a bounded channel.
pub struct ChannelTraceCallback<R> {
    /// Sending half of the bounded channel to the writer thread.
    tx: crossbeam_channel::Sender<WriterMsg<R>>,
    /// Behavior to apply when the channel is full.
    policy: OverflowPolicy,
    /// Shared count of records dropped under `OverflowPolicy::DropOnFull`.
    dropped: Arc<AtomicU64>,
}

impl<R: Send> TraceCallback<R> for ChannelTraceCallback<R> {
    fn record(&mut self, rec: TraceRecord<R>) {
        match self.policy {
            OverflowPolicy::DropOnFull => {
                if self.tx.try_send(WriterMsg::Record(rec)).is_err() {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
            OverflowPolicy::BlockOnFull => {
                // Blocks until the writer drains space, or returns immediately
                // if the writer thread has exited (receiver disconnected) —
                // never blocks forever on a dead consumer.
                let _ = self.tx.send(WriterMsg::Record(rec));
            }
        }
    }

    fn flush(&mut self) {
        // Always sent with a blocking `send` (regardless of `policy`, which only
        // governs record drops): a flush request must reach the writer thread in
        // order, after every record enqueued ahead of it, for callers to be able
        // to rely on "everything written before this call is now visible".
        let (ack_tx, ack_rx) = std::sync::mpsc::channel();
        if self.tx.send(WriterMsg::Flush(ack_tx)).is_ok() {
            let _ = ack_rx.recv();
        }
    }
}

/// Spawns a writer thread that drains trace records from a bounded channel and
/// writes them via `writer`. Returns a callback for the CPU thread, a join
/// handle to await flush-on-exit, and a shared counter of dropped records
/// (only incremented under [`OverflowPolicy::DropOnFull`]).
///
/// Writes are only made visible to an independent reader of the same file
/// (e.g. a live viewer that reopens the file on each poll) when the caller
/// explicitly calls [`ChannelTraceCallback::flush`] — the underlying
/// `BinaryTraceWriter` otherwise only flushes when the channel closes.
pub fn spawn_trace_writer<W: Write + Send + 'static, R: TraceRegisters>(
    writer: BinaryTraceWriter<W, R>,
    capacity: usize,
    policy: OverflowPolicy,
) -> (
    ChannelTraceCallback<R>,
    std::thread::JoinHandle<()>,
    Arc<AtomicU64>,
) {
    let (tx, rx) = crossbeam_channel::bounded::<WriterMsg<R>>(capacity);
    let dropped = Arc::new(AtomicU64::new(0));

    let mut writer = writer;
    let handle = std::thread::spawn(move || {
        // rx.recv() parks the thread until a message arrives, and returns Err
        // once the sender drops and the channel is drained — no polling loop needed.
        while let Ok(msg) = rx.recv() {
            match msg {
                WriterMsg::Record(rec) => writer.record(rec),
                WriterMsg::Flush(ack) => {
                    let _ = writer.flush();
                    let _ = ack.send(());
                }
            }
        }
        let _ = writer.flush();
    });

    (
        ChannelTraceCallback {
            tx,
            policy,
            dropped: dropped.clone(),
        },
        handle,
        dropped,
    )
}

/// The per-instruction correlation id and the not-yet-emitted register
/// snapshot for the instruction a CPU is stepping.
///
/// A CPU calls [`begin_instruction`](Self::begin_instruction) at the start of
/// each traced step and emits the snapshot (via
/// [`take_pending_registers`](Self::take_pending_registers)) just before the
/// step's first bus access, so a step that ends without touching the bus (a
/// breakpoint, say) leaves no record.
#[derive(Debug, Default)]
pub struct TraceState<R> {
    /// Id to assign to the next instruction that begins.
    next_instr_id: u64,
    /// The `(instr_id, snapshot)` for the current instruction, if its
    /// `Registers` record hasn't been emitted yet.
    pending: Option<(u64, R)>,
}

impl<R: Copy> TraceState<R> {
    /// Creates the state for a trace whose first instruction gets id 0.
    pub fn new() -> Self {
        Self {
            next_instr_id: 0,
            pending: None,
        }
    }

    /// Called once per step, before any register mutation for that step,
    /// with the register state as it stood at the start of the step.
    #[inline]
    pub fn begin_instruction(&mut self, regs: R) {
        let id = self.next_instr_id;
        self.next_instr_id = self.next_instr_id.wrapping_add(1);
        self.pending = Some((id, regs));
    }

    /// Returns the id of the most recently begun instruction.
    #[inline]
    pub fn current_instr_id(&self) -> u64 {
        self.next_instr_id.wrapping_sub(1)
    }

    /// Takes and returns the pending `Registers` snapshot for `instr_id`, if
    /// it hasn't already been emitted. Returns `None` (leaving any pending
    /// snapshot for a different id untouched) once already emitted.
    #[inline]
    pub fn take_pending_registers(&mut self, instr_id: u64) -> Option<R> {
        match self.pending.take() {
            Some((id, regs)) if id == instr_id => Some(regs),
            other => {
                self.pending = other;
                None
            }
        }
    }
}

#[cfg(test)]
mod tests;
