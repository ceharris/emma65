use super::*;
use crate::cpu::opcodes::lookup;
use emma6k_core::bus::{AddressRange, BusConfig};

/// Where every table instruction is disassembled.
const ORG: u16 = 0x1000;

/// The golden table: one line per instruction, `bytes | listing`. Every
/// valid line assembles back to its bytes with lwasm (see `lwasm_check.py`).
const EVERY_FORM: &str = include_str!("every_form.txt");

/// Operand bytes that exercise `mode`, for the every-opcode sweep.
fn sample_operand(mode: AddressingMode) -> &'static [u8] {
    match mode {
        AddressingMode::Inherent => &[],
        AddressingMode::Immediate8 => &[0x41],
        AddressingMode::Immediate16 => &[0x12, 0x34],
        AddressingMode::Direct => &[0x20],
        AddressingMode::Extended => &[0x12, 0x34],
        AddressingMode::Indexed => &[0x84],
        AddressingMode::Relative8 => &[0x10],
        AddressingMode::Relative16 => &[0x01, 0x00],
        AddressingMode::RegisterPair => &[0x12],
        AddressingMode::RegisterList => &[0x96],
    }
}

/// Every postbyte form, for each index register, with offsets that need the
/// form's size (so the listing assembles back to the same encoding).
fn indexed_forms() -> Vec<Vec<u8>> {
    let mut forms = Vec::new();
    for reg in 0..4u8 {
        let r = reg << 5;
        for form in [
            vec![r | 0x05],
            vec![r | 0x10],
            vec![r | 0x80],
            vec![r | 0x81],
            vec![r | 0x91],
            vec![r | 0x82],
            vec![r | 0x83],
            vec![r | 0x93],
            vec![r | 0x84],
            vec![r | 0x94],
            vec![r | 0x85],
            vec![r | 0x95],
            vec![r | 0x86],
            vec![r | 0x96],
            vec![r | 0x88, 0x40],
            vec![r | 0x88, 0xC0],
            vec![r | 0x98, 0x40],
            vec![r | 0x89, 0x12, 0x34],
            vec![r | 0x99, 0x12, 0x34],
            vec![r | 0x8B],
            vec![r | 0x9B],
        ] {
            forms.push(form);
        }
    }
    // PCR and [n16] ignore the register bits; assemblers encode them as X.
    for form in [
        vec![0x8C, 0x10],
        vec![0x8C, 0xF0],
        vec![0x9C, 0x10],
        vec![0x8D, 0x10, 0x00],
        vec![0x9D, 0x10, 0x00],
        vec![0x9F, 0x12, 0x34],
    ] {
        forms.push(form);
    }
    // PCR with other register bits still means PCR.
    forms.push(vec![0xEC, 0x10]);
    forms
}

/// The instructions the golden table lists: every valid opcode on every
/// page, every indexed form (with LDA), the other register operands, and
/// some invalid encodings.
fn table_instructions() -> Vec<Vec<u8>> {
    let mut all = Vec::new();
    for (page, prefix) in [
        (Page::Page1, None),
        (Page::Page2, Some(0x10)),
        (Page::Page3, Some(0x11)),
    ] {
        for opcode in 0..=255u8 {
            let info = lookup(page, opcode);
            if info.mnemonic == Mnemonic::Invalid
                || (page == Page::Page1 && matches!(opcode, 0x10 | 0x11))
            {
                continue;
            }
            let mut bytes: Vec<u8> = prefix.into_iter().collect();
            bytes.push(opcode);
            bytes.extend_from_slice(sample_operand(info.mode));
            all.push(bytes);
        }
    }
    for form in indexed_forms() {
        let mut bytes = vec![0xA6];
        bytes.extend(form);
        all.push(bytes);
    }
    // Every TFR register, and PSH/PUL lists naming each register.
    for code in [0x0, 0x1, 0x2, 0x3, 0x4, 0x5, 0x8, 0x9, 0xA, 0xB] {
        all.push(vec![0x1F, code << 4 | code]);
    }
    all.push(vec![0x1E, 0x89]);
    for opcode in [0x34, 0x35, 0x36, 0x37] {
        all.push(vec![opcode, 0xFF]);
        all.push(vec![opcode, 0x01]);
    }
    // Branches backward, and long branches on page 2.
    all.push(vec![0x20, 0xFE]);
    all.push(vec![0x16, 0xFF, 0xFD]);
    all.push(vec![0x10, 0x27, 0xFF, 0xFC]);
    // Invalid: undocumented opcodes on each page, and undefined postbytes.
    all.push(vec![0x01]);
    all.push(vec![0x10, 0x00]);
    all.push(vec![0x11, 0x8E]);
    all.push(vec![0xA6, 0x87]);
    all.push(vec![0x10, 0xAE, 0x9A]);
    all
}

fn render(line: &DisassembledLine) -> String {
    let bytes: Vec<String> = line.raw_bytes.iter().map(|b| format!("{b:02X}")).collect();
    let mut text = format!("{:<14} | {}", bytes.join(" "), line.mnemonic);
    if !line.operand_text.is_empty() {
        text.push(' ');
        text.push_str(&line.operand_text);
    }
    text
}

fn table() -> String {
    let symbols = SymbolTable::new();
    let mut out = String::new();
    for bytes in table_instructions() {
        assert_eq!(instruction_len(&bytes), Some(bytes.len()), "{bytes:02X?}");
        let line = Disassembler::new().build_line(ORG, bytes, &symbols);
        out.push_str(&render(&line));
        out.push('\n');
    }
    out
}

/// Compares against `every_form.txt`. Run with `EMMA09_BLESS=1` to rewrite
/// it after an intended change, then check the result with
/// `lwasm_check.py`.
#[test]
fn every_opcode_and_addressing_form_matches_the_table() {
    let actual = table();
    if std::env::var_os("EMMA09_BLESS").is_some() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/disassembler/every_form.txt"
        );
        std::fs::write(path, &actual).unwrap();
        return;
    }
    for (i, (want, got)) in EVERY_FORM.lines().zip(actual.lines()).enumerate() {
        assert_eq!(got, want, "line {}", i + 1);
    }
    assert_eq!(actual.lines().count(), EVERY_FORM.lines().count());
}

#[test]
fn sweep_covers_every_valid_opcode() {
    // 221 + 38 + 9 valid opcodes, less the two page-1 prefixes, which
    // aren't table entries anyway.
    let sweep = table_instructions().len();
    assert!(sweep >= 221 + 38 + 9, "{sweep}");
}

#[test]
fn instruction_len_waits_for_the_bytes_it_needs() {
    assert_eq!(instruction_len(&[]), None);
    assert_eq!(instruction_len(&[0x10]), None);
    assert_eq!(instruction_len(&[0x10, 0x8E]), Some(4));
    assert_eq!(instruction_len(&[0xA6]), None);
    assert_eq!(instruction_len(&[0xA6, 0x89]), Some(4));
    assert_eq!(instruction_len(&[0x10, 0xAE]), None);
    assert_eq!(instruction_len(&[0x10, 0xAE, 0x9F]), Some(5));
    assert_eq!(instruction_len(&[0xA6, 0x87]), Some(2));
    assert_eq!(instruction_len(&[0x01]), Some(1));
    assert_eq!(instruction_len(&[0x11, 0x00]), Some(2));
}

#[test]
fn immediate8_operands_get_a_comment() {
    let line = Disassembler::new().build_line(ORG, vec![0x86, 0x41], &SymbolTable::new());
    assert_eq!(line.comment_text.as_deref(), Some("; 65 'A'"));
    let line = Disassembler::new().build_line(ORG, vec![0x8E, 0x00, 0x41], &SymbolTable::new());
    assert_eq!(line.comment_text, None);
}

#[test]
fn invalid_encodings_are_marked() {
    let symbols = SymbolTable::new();
    for bytes in [vec![0x01], vec![0x11, 0x8E], vec![0xA6, 0x87]] {
        let line = Disassembler::new().build_line(ORG, bytes.clone(), &symbols);
        assert!(!line.is_valid, "{bytes:02X?}");
        assert_eq!(line.mnemonic, Mnemonic::Invalid);
        assert_eq!(line.mnemonic.to_string(), "<ILL>");
    }
}

fn bus_with(start: u16, bytes: &[u8], labels: &[(u16, &str)]) -> Bus {
    let mut bus = BusConfig::new()
        .ram(AddressRange::new(0x0000, 0xFFFF))
        .unwrap()
        .build();
    for (i, &b) in bytes.iter().enumerate() {
        bus.write(start.wrapping_add(i as u16), b).unwrap();
    }
    for &(addr, name) in labels {
        bus.symbol_table_mut().insert(name.to_string(), addr);
    }
    bus
}

#[test]
fn labels_name_addresses_and_targets() {
    let bus = bus_with(
        0xE000,
        &[
            0xBD, 0xE0, 0x10, 0x26, 0xFB, 0x30, 0x8C, 0x08, 0xA6, 0x9F, 0xE0, 0x10,
        ],
        &[(0xE000, "START"), (0xE010, "EMIT")],
    );
    let lines = Disassembler::new().disassemble_range(&bus, 0xE000, 0xE00C, 10);
    let text: Vec<String> = lines
        .iter()
        .map(|l| format!("{} {}", l.mnemonic, l.operand_text))
        .collect();
    assert_eq!(
        text,
        ["JSR EMIT", "BNE START", "LEAX EMIT,PCR", "LDA [EMIT]"]
    );
    assert_eq!(lines[0].labels, ["START"]);
}

#[test]
fn disassemble_uses_peek_and_reads_whole_instructions() {
    let bus = bus_with(0x2000, &[0x10, 0xAE, 0x9F, 0x12, 0x34, 0x12], &[]);
    let lines = Disassembler::new().disassemble_range(&bus, 0x2000, 0, 2);
    assert_eq!(lines[0].raw_bytes, [0x10, 0xAE, 0x9F, 0x12, 0x34]);
    assert_eq!(lines[0].operand_text, "[$1234]");
    assert_eq!(lines[1].addr, 0x2005);
    assert_eq!(lines[1].mnemonic, Mnemonic::Nop);
}

#[test]
fn disassemble_range_stops_at_the_top_of_memory() {
    let bus = bus_with(0xFFFE, &[0x12, 0x12], &[]);
    let lines = Disassembler::new().disassemble_range(&bus, 0xFFFE, 0, 10);
    assert_eq!(lines.len(), 2);
}
