use super::*;
use crate::config::CpuVariantSpec;
use crate::cpu::trace::{BinaryTraceReader, BinaryTraceWriter, TraceCallback};
use crate::cpu::{Cpu, NMI_VECTOR, RESET_VECTOR};
use emma6k_core::exec::StepResult;
use emma6k_core::{AddressRange, Bus, Processor};
use std::io;
use std::sync::{Arc, Mutex};

const START: u16 = 0x1000;
const HANDLER: u16 = 0x2000;

/// The program, and its expected listing.
const PROGRAM: &[u8] = &[
    0x10, 0xCE, 0x80, 0x00, // LDS #$8000 (arms NMI)
    0x8E, 0x30, 0x00, // LDX #$3000
    0xA6, 0x9F, 0x30, 0x00, // LDA [$3000]
    0x34, 0x16, // PSHS A,B,X
    0x10, 0xAE, 0x89, 0x00, 0x02, // LDY $0002,X
    0x26, 0x00, // BNE $1014 (taken or not, the next instruction)
    0x12, // NOP
];
const LISTING: &[&str] = &[
    "LDS #$8000",
    "LDX #$3000",
    "LDA [$3000]",
    "PSHS A,B,X",
    "LDY $0002,X",
    "BNE $1014",
    "NOP",
];

/// Records every trace record into a shared vector.
struct Capture(Arc<Mutex<Vec<TraceRecord>>>);

impl TraceCallback<Registers> for Capture {
    fn record(&mut self, rec: TraceRecord) {
        self.0.lock().unwrap().push(rec);
    }
}

fn traced_cpu() -> (Cpu, Arc<Mutex<Vec<TraceRecord>>>) {
    let mut bus = Bus::config()
        .ram_with_fill(AddressRange::new(0x0000, 0xFFFF), 0)
        .unwrap()
        .build();
    let mut load = |addr: u16, bytes: &[u8]| {
        for (i, &b) in bytes.iter().enumerate() {
            bus.write(addr + i as u16, b).unwrap();
        }
    };
    load(RESET_VECTOR, &START.to_be_bytes());
    load(NMI_VECTOR, &HANDLER.to_be_bytes());
    load(START, PROGRAM);
    load(HANDLER, &[0x3B]); // RTI
    load(0x3000, &[0x30, 0x04, 0x99]); // the pointer [$3000] → $3004
    let mut cpu = Cpu::builder().bus(bus).build().unwrap();
    cpu.reset().unwrap();
    let records = Arc::new(Mutex::new(Vec::new()));
    cpu.set_trace_callback(Some(Box::new(Capture(records.clone()))));
    (cpu, records)
}

fn step(cpu: &mut Cpu) {
    assert!(matches!(cpu.step(None, false), StepResult::Executed(_)));
}

fn rows(records: &[TraceRecord]) -> Vec<TraceRow> {
    let mut assembler = TraceRowAssembler::new(SymbolTable::new());
    let mut rows: Vec<TraceRow> = records.iter().filter_map(|r| assembler.feed(r)).collect();
    rows.extend(assembler.flush());
    rows
}

fn listing(row: &TraceRow) -> String {
    let line = row.line.as_ref().expect("an instruction row");
    format!("{} {}", line.mnemonic, line.operand_text)
        .trim_end()
        .to_string()
}

#[test]
fn rows_match_the_program_listing() {
    let (mut cpu, records) = traced_cpu();
    for _ in 0..LISTING.len() {
        step(&mut cpu);
    }
    let rows = rows(&records.lock().unwrap());
    let listed: Vec<String> = rows.iter().map(listing).collect();
    assert_eq!(listed, LISTING);

    // Ids count steps, and each row starts from the registers before it ran.
    assert!(rows.iter().enumerate().all(|(i, r)| r.instr_id == i as u64));
    assert_eq!(rows[0].regs.pc, START);
    assert_eq!(rows[1].regs.s, 0x8000);
    assert_eq!(rows[0].cycles, Some(4));
    assert_eq!(rows[2].cycles, Some(9)); // LDA [n16]: 4 + 5

    // LDA [$3000]: after the four fetches, the pointer and the data.
    let ops: Vec<TraceBusOp> = rows[2].non_fetch_bus_ops().copied().collect();
    assert_eq!(
        ops,
        [
            TraceBusOp::Read {
                addr: 0x3000,
                value: 0x30
            },
            TraceBusOp::Read {
                addr: 0x3001,
                value: 0x04
            },
            TraceBusOp::Read {
                addr: 0x3004,
                value: 0x00
            },
        ]
    );
    // PSHS A,B,X writes four bytes below S.
    assert_eq!(
        rows[3]
            .non_fetch_bus_ops()
            .filter(|op| matches!(op, TraceBusOp::Write { .. }))
            .count(),
        4
    );
}

#[test]
fn an_interrupt_is_a_row_without_a_line() {
    let (mut cpu, records) = traced_cpu();
    step(&mut cpu); // LDS arms NMI
    cpu.machine_mut().interrupts_mut().signal_nmi();
    step(&mut cpu); // services the NMI
    step(&mut cpu); // RTI
    let rows = rows(&records.lock().unwrap());
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1].line, None);
    assert_eq!(rows[1].vector(), Some(NMI_VECTOR));
    assert_eq!(rows[1].vector().and_then(vector_name), Some("NMI"));
    assert_eq!(rows[1].cycles, Some(19));
    assert_eq!(
        rows[1]
            .bus_ops
            .iter()
            .filter(|op| matches!(op, TraceBusOp::Write { .. }))
            .count(),
        12
    );
    assert_eq!(listing(&rows[2]), "RTI");
    assert_eq!(rows[2].vector(), None);
}

#[test]
fn a_step_without_bus_traffic_leaves_no_record() {
    let (mut cpu, records) = traced_cpu();
    cpu.machine_mut().breakpoints_mut().insert(START);
    assert!(matches!(
        cpu.step(None, true),
        StepResult::Breakpoint(START)
    ));
    assert!(records.lock().unwrap().is_empty());
    step(&mut cpu);
    let rows = rows(&records.lock().unwrap());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].instr_id, 1);
}

#[test]
fn a_file_round_trip_reconstructs_the_same_rows() {
    let (mut cpu, records) = traced_cpu();
    for _ in 0..LISTING.len() {
        step(&mut cpu);
    }
    let records = records.lock().unwrap().clone();

    let mut file = Vec::new();
    let mut writer = BinaryTraceWriter::new(&mut file, CpuVariantSpec::Mc6809);
    for &rec in &records {
        writer.record(rec);
    }
    writer.flush().unwrap();
    drop(writer);

    let read: Vec<TraceRecord> = BinaryTraceReader::new(file.as_slice())
        .unwrap()
        .collect::<io::Result<_>>()
        .unwrap();
    assert_eq!(read, records);
    assert_eq!(rows(&read), rows(&records));
}

#[test]
fn removing_the_callback_stops_tracing() {
    let (mut cpu, records) = traced_cpu();
    step(&mut cpu);
    cpu.set_trace_callback(None);
    step(&mut cpu);
    assert_eq!(rows(&records.lock().unwrap()).len(), 1);
}

#[test]
fn every_vector_has_a_name() {
    use crate::cpu::{FIRQ_VECTOR, IRQ_VECTOR, SWI_VECTOR, SWI2_VECTOR, SWI3_VECTOR};
    for (vector, name) in [
        (NMI_VECTOR, "NMI"),
        (FIRQ_VECTOR, "FIRQ"),
        (IRQ_VECTOR, "IRQ"),
        (SWI_VECTOR, "SWI"),
        (SWI2_VECTOR, "SWI2"),
        (SWI3_VECTOR, "SWI3"),
        (RESET_VECTOR, "RESET"),
    ] {
        assert_eq!(vector_name(vector), Some(name));
    }
    assert_eq!(vector_name(0x1234), None);
}

#[test]
fn records_of_another_step_dont_join_the_pending_row() {
    // Step 1 ends without its Cycles record (as after an error), and step
    // 2's records arrive without its Registers record (as in a stream that
    // starts mid-step). Step 2's records don't belong to step 1's row.
    let (cpu, _) = traced_cpu();
    let regs = *cpu.registers();
    let records = [
        TraceRecord {
            instr_id: 1,
            kind: TraceKind::Registers(regs),
        },
        TraceRecord {
            instr_id: 1,
            kind: TraceKind::Read {
                addr: START,
                value: 0x12,
            },
        },
        TraceRecord {
            instr_id: 2,
            kind: TraceKind::Read {
                addr: 0x3000,
                value: 0x01,
            },
        },
        TraceRecord {
            instr_id: 2,
            kind: TraceKind::Write {
                addr: 0x3001,
                value: 0x02,
            },
        },
        TraceRecord {
            instr_id: 2,
            kind: TraceKind::Cycles(4),
        },
    ];
    let mut assembler = TraceRowAssembler::new(SymbolTable::new());
    assert!(records.iter().all(|r| assembler.feed(r).is_none()));
    let row = assembler.flush().expect("step 1's unfinished row");
    assert_eq!(row.instr_id, 1);
    assert_eq!(row.bus_ops.len(), 1);
    assert_eq!(row.cycles, None);
    assert!(assembler.flush().is_none());
}
