//! Unit tests of the 6809 CPU, built from machine-code byte arrays (there is
//! no in-tree 6809 assembler). Cycle counts are the MC6809 datasheet's.
//!
//! W. Schwotzer's test suite (`tests/schwotzer_6809.rs`) checks results and
//! condition codes across the instruction set and every addressing mode.
//! These tests cover what it doesn't: cycle counts, the stacks, SWI and RTI,
//! CWAI and SYNC, reset, the invalid-opcode policy, and the hooks the
//! debugger and `exec` use.

use super::*;
use emma6k_core::AddressRange;

/// Where test programs start.
const START: u16 = 0x1000;
/// Where S starts.
const STACK: u16 = 0x8000;

fn cpu_with(program: &[u8], policy: InvalidOpcodePolicy) -> Cpu {
    let bus = Bus::config()
        .ram_with_fill(AddressRange::new(0x0000, 0xFFFF), 0)
        .unwrap()
        .build();
    let mut cpu = Cpu::builder()
        .invalid_opcode_policy(policy)
        .bus(bus)
        .build()
        .unwrap();
    write(&mut cpu, RESET_VECTOR, &START.to_be_bytes());
    write(&mut cpu, START, program);
    cpu.reset().unwrap();
    cpu.regs.s = STACK;
    cpu
}

fn cpu(program: &[u8]) -> Cpu {
    cpu_with(program, InvalidOpcodePolicy::Nop)
}

fn write(cpu: &mut Cpu, addr: u16, bytes: &[u8]) {
    for (i, &b) in bytes.iter().enumerate() {
        cpu.bus_mut().write(addr.wrapping_add(i as u16), b).unwrap();
    }
}

fn peek(cpu: &Cpu, addr: u16) -> u8 {
    cpu.bus().peek(addr).unwrap()
}

/// Steps once, expecting an executed instruction; returns its cycles.
fn step(cpu: &mut Cpu) -> u64 {
    let before = cpu.cycles();
    match cpu.step(None, true) {
        StepResult::Executed(_) => cpu.cycles() - before,
        StepResult::Error(e) => panic!("unexpected error: {e}"),
        _ => panic!("expected an executed instruction"),
    }
}

/// Steps through `count` instructions.
fn steps(cpu: &mut Cpu, count: usize) {
    for _ in 0..count {
        step(cpu);
    }
}

fn cc(cpu: &Cpu) -> ConditionCodes {
    cpu.registers().cc
}

// --- reset ---

#[test]
fn reset_reads_the_big_endian_vector_at_fffe() {
    let cpu = cpu(&[]);
    assert_eq!(cpu.pc(), START);
    assert_eq!(cpu.registers().dp, 0);
    assert!(cc(&cpu).contains(ConditionCodes::I | ConditionCodes::F));
    assert_eq!(cpu.cycles(), 0);
    assert_eq!(cpu.halt_state(), HaltState::Running);
}

#[test]
fn reset_clears_dp_and_masks_interrupts() {
    let mut cpu = cpu(&[]);
    cpu.regs.dp = 0x20;
    cpu.regs.cc = ConditionCodes::empty();
    cpu.reset().unwrap();
    assert_eq!(cpu.registers().dp, 0);
    assert!(cc(&cpu).contains(ConditionCodes::I | ConditionCodes::F));
}

#[test]
fn a_pending_reset_is_serviced_by_step() {
    let mut cpu = cpu(&[0x12]);
    step(&mut cpu);
    cpu.interrupts_mut().signal_reset();
    assert!(matches!(cpu.step(None, true), StepResult::Reset));
    assert_eq!(cpu.pc(), START);
}

// --- cycle counts ---

/// Each instruction's cycles, run from reset with X, Y, U and S pointing at
/// RAM. CC after reset is I|F: Z is clear, so BNE is taken and BEQ isn't.
#[test]
fn cycle_counts_match_the_datasheet() {
    let cases: &[(&str, &[u8], u64)] = &[
        ("NOP", &[0x12], 2),
        ("LDA #", &[0x86, 0x01], 2),
        ("LDA <", &[0x96, 0x10], 4),
        ("LDA >", &[0xB6, 0x20, 0x00], 5),
        ("STA <", &[0x97, 0x10], 4),
        ("STA >", &[0xB7, 0x20, 0x00], 5),
        ("LDD #", &[0xCC, 0x12, 0x34], 3),
        ("LDD <", &[0xDC, 0x10], 5),
        ("LDD >", &[0xFC, 0x20, 0x00], 6),
        ("STD >", &[0xFD, 0x20, 0x00], 6),
        ("LDX #", &[0x8E, 0x20, 0x00], 3),
        ("CMPX #", &[0x8C, 0x20, 0x00], 4),
        ("SUBD <", &[0x93, 0x10], 6),
        ("ADDD >", &[0xF3, 0x20, 0x00], 7),
        ("LDY #", &[0x10, 0x8E, 0x20, 0x00], 4),
        ("STY <", &[0x10, 0x9F, 0x10], 6),
        ("LDS #", &[0x10, 0xCE, 0x80, 0x00], 4),
        ("STS >", &[0x10, 0xFF, 0x20, 0x00], 7),
        ("CMPD #", &[0x10, 0x83, 0x00, 0x00], 5),
        ("CMPY <", &[0x10, 0x9C, 0x10], 7),
        ("CMPU >", &[0x11, 0xB3, 0x20, 0x00], 8),
        ("CMPS #", &[0x11, 0x8C, 0x00, 0x00], 5),
        // Indexed: 4 cycles for LDA plus the postbyte's extras.
        ("LDA ,X", &[0xA6, 0x84], 4),
        ("LDA 5,X", &[0xA6, 0x05], 5),
        ("LDA ,X+", &[0xA6, 0x80], 6),
        ("LDA ,X++", &[0xA6, 0x81], 7),
        ("LDA ,-X", &[0xA6, 0x82], 6),
        ("LDA ,--X", &[0xA6, 0x83], 7),
        ("LDA B,X", &[0xA6, 0x85], 5),
        ("LDA A,X", &[0xA6, 0x86], 5),
        ("LDA n8,X", &[0xA6, 0x88, 0x10], 5),
        ("LDA n16,X", &[0xA6, 0x89, 0x01, 0x00], 8),
        ("LDA D,X", &[0xA6, 0x8B], 8),
        ("LDA n8,PCR", &[0xA6, 0x8C, 0x10], 5),
        ("LDA n16,PCR", &[0xA6, 0x8D, 0x01, 0x00], 9),
        ("LDA [,X]", &[0xA6, 0x94], 7),
        ("LDA [,X++]", &[0xA6, 0x91], 10),
        ("LDA [,--X]", &[0xA6, 0x93], 10),
        ("LDA [B,X]", &[0xA6, 0x95], 8),
        ("LDA [n8,X]", &[0xA6, 0x98, 0x10], 8),
        ("LDA [n16,X]", &[0xA6, 0x99, 0x01, 0x00], 11),
        ("LDA [D,X]", &[0xA6, 0x9B], 11),
        ("LDA [n8,PCR]", &[0xA6, 0x9C, 0x10], 8),
        ("LDA [n16,PCR]", &[0xA6, 0x9D, 0x01, 0x00], 12),
        ("LDA [n16]", &[0xA6, 0x9F, 0x20, 0x00], 9),
        ("LDY ,X", &[0x10, 0xAE, 0x84], 6),
        ("CMPU [n16]", &[0x11, 0xA3, 0x9F, 0x20, 0x00], 12),
        // Read-modify-write and JMP.
        ("NEG <", &[0x00, 0x10], 6),
        ("CLR ,X", &[0x6F, 0x84], 6),
        ("INC >", &[0x7C, 0x20, 0x00], 7),
        ("TST >", &[0x7D, 0x20, 0x00], 7),
        ("JMP <", &[0x0E, 0x10], 3),
        ("JMP ,X", &[0x6E, 0x84], 3),
        ("JMP >", &[0x7E, 0x20, 0x00], 4),
        ("NEGA", &[0x40], 2),
        ("CLRB", &[0x5F], 2),
        // Branches and calls.
        ("BRA", &[0x20, 0x00], 3),
        ("BEQ (not taken)", &[0x27, 0x10], 3),
        ("BNE (taken)", &[0x26, 0x10], 3),
        ("LBRA", &[0x16, 0x00, 0x10], 5),
        ("LBRN", &[0x10, 0x21, 0x00, 0x10], 5),
        ("LBEQ (not taken)", &[0x10, 0x27, 0x00, 0x10], 5),
        ("LBNE (taken)", &[0x10, 0x26, 0x00, 0x10], 6),
        ("BSR", &[0x8D, 0x10], 7),
        ("LBSR", &[0x17, 0x00, 0x10], 9),
        ("JSR <", &[0x9D, 0x10], 7),
        ("JSR ,X", &[0xAD, 0x84], 7),
        ("JSR >", &[0xBD, 0x20, 0x00], 8),
        // Everything else.
        ("LEAX ,X", &[0x30, 0x84], 4),
        ("LEAX n16,X", &[0x30, 0x89, 0x01, 0x00], 8),
        ("LEAS -2,S", &[0x32, 0x7E], 5),
        ("TFR", &[0x1F, 0x89], 6),
        ("EXG", &[0x1E, 0x89], 8),
        ("PSHS A,B", &[0x34, 0x06], 7),
        ("PSHS all", &[0x34, 0xFF], 17),
        ("PSHU X", &[0x36, 0x10], 7),
        ("PULS CC", &[0x35, 0x01], 6),
        ("PULS PC", &[0x35, 0x80], 7),
        ("PULU all", &[0x37, 0xFF], 17),
        ("RTS", &[0x39], 5),
        ("ABX", &[0x3A], 3),
        ("MUL", &[0x3D], 11),
        ("SEX", &[0x1D], 2),
        ("DAA", &[0x19], 2),
        ("ORCC", &[0x1A, 0x00], 3),
        ("ANDCC", &[0x1C, 0xFF], 3),
        ("SWI", &[0x3F], 19),
        ("SWI2", &[0x10, 0x3F], 20),
        ("SWI3", &[0x11, 0x3F], 20),
        ("CWAI", &[0x3C, 0xFF], 20),
        ("SYNC", &[0x13], 4),
    ];
    for &(name, program, expected) in cases {
        let mut cpu = cpu(program);
        cpu.regs.x = 0x3000;
        cpu.regs.y = 0x3100;
        cpu.regs.u = 0x4000;
        assert_eq!(step(&mut cpu), expected, "{name}");
    }
}

#[test]
fn rti_takes_6_cycles_for_fast_state_and_15_for_entire() {
    // CC (E clear) and PC on the stack.
    let mut fast = cpu(&[0x3B]);
    write(&mut fast, STACK - 3, &[0x00, 0x20, 0x00]);
    fast.regs.s = STACK - 3;
    assert_eq!(step(&mut fast), 6);
    assert_eq!(fast.pc(), 0x2000);
    assert_eq!(fast.registers().s, STACK);

    // CC (E set), A, B, DP, X, Y, U, PC.
    let mut entire = cpu(&[0x3B]);
    let frame = [
        0x80, 0x11, 0x22, 0x33, 0x44, 0x44, 0x55, 0x55, 0x66, 0x66, 0x20, 0x00,
    ];
    write(&mut entire, STACK - 12, &frame);
    entire.regs.s = STACK - 12;
    assert_eq!(step(&mut entire), 15);
    let r = entire.registers();
    assert_eq!((r.a, r.b, r.dp), (0x11, 0x22, 0x33));
    assert_eq!(
        (r.x, r.y, r.u, r.pc, r.s),
        (0x4444, 0x5555, 0x6666, 0x2000, STACK)
    );
    assert_eq!(r.cc.to_byte(), 0x80);
}

// --- addressing ---

#[test]
fn direct_mode_uses_dp_as_the_high_byte() {
    // LDA <$10 with DP = $20 reads $2010.
    let mut cpu = cpu(&[0x96, 0x10]);
    cpu.regs.dp = 0x20;
    write(&mut cpu, 0x2010, &[0x5A]);
    step(&mut cpu);
    assert_eq!(cpu.registers().a, 0x5A);
}

#[test]
fn words_are_big_endian_in_memory() {
    // LDD #$1234; STD $2000; LDX $2000
    let mut cpu = cpu(&[0xCC, 0x12, 0x34, 0xFD, 0x20, 0x00, 0xBE, 0x20, 0x00]);
    steps(&mut cpu, 3);
    assert_eq!((peek(&cpu, 0x2000), peek(&cpu, 0x2001)), (0x12, 0x34));
    assert_eq!(cpu.registers().x, 0x1234);
}

#[test]
fn lea_with_post_increment_keeps_the_original_address() {
    // LEAX ,X+ leaves X as it was; LEAY ,X+ takes X's old value and bumps X.
    let mut cpu = cpu(&[0x30, 0x80, 0x31, 0x80]);
    cpu.regs.x = 0x2000;
    step(&mut cpu);
    assert_eq!(cpu.registers().x, 0x2000);
    step(&mut cpu);
    assert_eq!((cpu.registers().y, cpu.registers().x), (0x2000, 0x2001));
}

#[test]
fn leax_sets_z_but_leas_leaves_cc_alone() {
    // LEAX ,Y with Y = 0 sets Z; LEAS 1,S leaves Z set though S isn't 0.
    let mut cpu = cpu(&[0x30, 0xA4, 0x32, 0x61]);
    step(&mut cpu);
    assert!(cc(&cpu).contains(ConditionCodes::Z));
    step(&mut cpu);
    assert!(cc(&cpu).contains(ConditionCodes::Z));
    assert_eq!(cpu.registers().s, STACK + 1);
}

#[test]
fn pc_relative_indexing_counts_from_the_next_instruction() {
    // LDA $10,PCR at START: the next instruction is START + 3.
    let mut cpu = cpu(&[0xA6, 0x8C, 0x10]);
    write(&mut cpu, START + 3 + 0x10, &[0x77]);
    step(&mut cpu);
    assert_eq!(cpu.registers().a, 0x77);
}

// --- stacks ---

#[test]
fn pshs_pushes_in_datasheet_order_and_puls_restores() {
    // PSHS PC,U,Y,X,DP,B,A,CC; then clear everything and PULS all but PC.
    let mut cpu = cpu(&[0x34, 0xFF, 0x35, 0x7F]);
    let r = cpu.registers_mut();
    (r.a, r.b, r.dp) = (0xA1, 0xB2, 0xD3);
    (r.x, r.y, r.u) = (0x1111, 0x2222, 0x3333);
    r.cc = ConditionCodes::from_byte(0x0F);
    step(&mut cpu);
    let s = cpu.registers().s;
    assert_eq!(s, STACK - 12);
    let stacked: Vec<u8> = (0..12).map(|i| peek(&cpu, s + i)).collect();
    assert_eq!(
        stacked,
        [
            0x0F, 0xA1, 0xB2, 0xD3, 0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x10, 0x02
        ]
    );

    let r = cpu.registers_mut();
    (r.a, r.b, r.dp, r.x, r.y, r.u) = (0, 0, 0, 0, 0, 0);
    step(&mut cpu);
    let r = cpu.registers();
    assert_eq!((r.a, r.b, r.dp), (0xA1, 0xB2, 0xD3));
    assert_eq!((r.x, r.y, r.u), (0x1111, 0x2222, 0x3333));
    assert_eq!(r.cc.to_byte(), 0x0F);
    assert_eq!(r.s, STACK - 2);
}

#[test]
fn pshu_pushes_s_and_pulu_loads_it() {
    // PSHU S; LDS #$1234; PULU S
    let mut cpu = cpu(&[0x36, 0x40, 0x10, 0xCE, 0x12, 0x34, 0x37, 0x40]);
    cpu.regs.u = 0x4000;
    step(&mut cpu);
    assert_eq!(cpu.registers().u, 0x3FFE);
    assert_eq!((peek(&cpu, 0x3FFE), peek(&cpu, 0x3FFF)), (0x80, 0x00));
    steps(&mut cpu, 2);
    assert_eq!(cpu.registers().s, STACK);
    assert_eq!(cpu.registers().u, 0x4000);
}

#[test]
fn bsr_and_rts_round_trip() {
    // BSR +2; NOP; NOP; (sub) RTS
    let mut cpu = cpu(&[0x8D, 0x02, 0x12, 0x12, 0x39]);
    step(&mut cpu);
    assert_eq!(cpu.pc(), START + 4);
    assert_eq!((peek(&cpu, STACK - 2), peek(&cpu, STACK - 1)), (0x10, 0x02));
    step(&mut cpu);
    assert_eq!(cpu.pc(), START + 2);
    assert_eq!(cpu.registers().s, STACK);
}

// --- SWI, RTI, CWAI, SYNC ---

#[test]
fn swi_stacks_the_entire_state_and_masks_interrupts() {
    let mut cpu = cpu(&[0x3F]);
    write(&mut cpu, SWI_VECTOR, &[0x20, 0x00]);
    write(&mut cpu, 0x2000, &[0x3B]); // RTI
    cpu.regs.cc = ConditionCodes::C;
    cpu.regs.a = 0x42;
    step(&mut cpu);
    assert_eq!(cpu.pc(), 0x2000);
    assert_eq!(cpu.registers().s, STACK - 12);
    // The stacked CC has E set; the live CC has E, I and F.
    assert_eq!(peek(&cpu, STACK - 12), 0x81);
    assert_eq!(cc(&cpu).to_byte(), 0xD1);
    // The stacked PC is the instruction after SWI.
    assert_eq!((peek(&cpu, STACK - 2), peek(&cpu, STACK - 1)), (0x10, 0x01));

    cpu.regs.a = 0;
    step(&mut cpu);
    assert_eq!(cpu.pc(), START + 1);
    assert_eq!(cpu.registers().a, 0x42);
    assert_eq!(cc(&cpu).to_byte(), 0x81);
    assert_eq!(cpu.registers().s, STACK);
}

#[test]
fn swi2_and_swi3_leave_the_masks_alone() {
    for (program, vector) in [([0x10, 0x3F], SWI2_VECTOR), ([0x11, 0x3F], SWI3_VECTOR)] {
        let mut cpu = cpu(&program);
        write(&mut cpu, vector, &[0x24, 0x68]);
        cpu.regs.cc = ConditionCodes::empty();
        step(&mut cpu);
        assert_eq!(cpu.pc(), 0x2468);
        assert_eq!(cc(&cpu).to_byte(), 0x80, "only E is set");
    }
}

#[test]
fn cwai_clears_cc_bits_stacks_the_state_and_waits() {
    // CWAI #$EF clears I; E is set and the entire state is stacked.
    let mut cpu = cpu(&[0x3C, 0xEF, 0x12]);
    step(&mut cpu);
    assert_eq!(cpu.halt_state(), HaltState::Waiting);
    assert_eq!(cpu.registers().s, STACK - 12);
    assert_eq!(peek(&cpu, STACK - 12), 0xC0, "stacked CC: E and F");
    assert_eq!((peek(&cpu, STACK - 2), peek(&cpu, STACK - 1)), (0x10, 0x02));
    assert!(matches!(cpu.step(None, true), StepResult::Waiting));
    assert_eq!(cpu.pc(), START + 2);

    // A reset ends the wait too.
    cpu.interrupts_mut().signal_reset();
    assert!(matches!(cpu.step(None, true), StepResult::Reset));
    assert_eq!(cpu.halt_state(), HaltState::Running);
}

#[test]
fn sync_waits() {
    let mut cpu = cpu(&[0x13]);
    step(&mut cpu);
    assert_eq!(cpu.halt_state(), HaltState::Waiting);
    assert!(matches!(cpu.step(None, true), StepResult::Waiting));
}

// --- ALU details the suite doesn't check ---

#[test]
fn branches_follow_signed_and_unsigned_comparisons() {
    // LDA #$80; CMPA #$01: -128 < 1 signed, $80 > $01 unsigned.
    for (branch, taken) in [
        (0x2D, true),  // BLT
        (0x2C, false), // BGE
        (0x2F, true),  // BLE
        (0x2E, false), // BGT
        (0x22, true),  // BHI
        (0x23, false), // BLS
        (0x24, true),  // BCC
        (0x25, false), // BCS
    ] {
        let mut cpu = cpu(&[0x86, 0x80, 0x81, 0x01, branch, 0x10]);
        steps(&mut cpu, 3);
        let expected = if taken { START + 6 + 0x10 } else { START + 6 };
        assert_eq!(cpu.pc(), expected, "branch ${branch:02X}");
    }
}

#[test]
fn long_branches_reach_backward() {
    // LBRA -3 jumps back to itself.
    let mut cpu = cpu(&[0x16, 0xFF, 0xFD]);
    step(&mut cpu);
    assert_eq!(cpu.pc(), START);
}

#[test]
fn tfr_between_sizes() {
    // TFR A,X fills X's high byte with $FF; TFR Y,B keeps Y's low byte;
    // TFR X,PC jumps.
    let mut cpu = cpu(&[0x1F, 0x81, 0x1F, 0x29, 0x1F, 0x15]);
    cpu.regs.a = 0x12;
    cpu.regs.y = 0xABCD;
    step(&mut cpu);
    assert_eq!(cpu.registers().x, 0xFF12);
    step(&mut cpu);
    assert_eq!(cpu.registers().b, 0xCD);
    step(&mut cpu);
    assert_eq!(cpu.pc(), 0xFF12);
}

#[test]
fn abx_adds_b_unsigned_without_flags() {
    let mut cpu = cpu(&[0x3A]);
    cpu.regs.x = 0x1000;
    cpu.regs.b = 0xFF;
    cpu.regs.cc = ConditionCodes::empty();
    step(&mut cpu);
    assert_eq!(cpu.registers().x, 0x10FF);
    assert_eq!(cc(&cpu), ConditionCodes::empty());
}

#[test]
fn daa_adjusts_bcd_and_carries() {
    // LDA #$09; ADDA #$01; DAA → $10.
    let mut cpu = cpu(&[0x86, 0x09, 0x8B, 0x01, 0x19]);
    steps(&mut cpu, 3);
    assert_eq!(cpu.registers().a, 0x10);
    assert!(!cc(&cpu).contains(ConditionCodes::C));
    // LDA #$99; ADDA #$01; DAA → $00 with C.
    let mut cpu = self::cpu(&[0x86, 0x99, 0x8B, 0x01, 0x19]);
    steps(&mut cpu, 3);
    assert_eq!(cpu.registers().a, 0x00);
    assert!(cc(&cpu).contains(ConditionCodes::C | ConditionCodes::Z));
}

#[test]
fn add_sets_half_carry() {
    // LDA #$0F; ADDA #$01
    let mut cpu = cpu(&[0x86, 0x0F, 0x8B, 0x01]);
    steps(&mut cpu, 2);
    assert!(cc(&cpu).contains(ConditionCodes::H));
}

// --- invalid opcodes ---

#[test]
fn nop_policy_skips_an_undocumented_opcode() {
    let mut cpu = cpu(&[0x01, 0x10, 0x00, 0x12]);
    assert_eq!(step(&mut cpu), opcodes::INVALID_OPCODE_CYCLES as u64);
    assert_eq!(cpu.pc(), START + 1);
    // A page-2 one skips the prefix too.
    step(&mut cpu);
    assert_eq!(cpu.pc(), START + 3);
}

#[test]
fn error_policy_reports_an_undocumented_opcode() {
    let mut cpu = cpu_with(&[0x10, 0x00], InvalidOpcodePolicy::Error);
    assert!(matches!(
        cpu.step(None, true),
        StepResult::Error(ExecError::InvalidOpcode {
            addr: START,
            opcode: 0x00
        })
    ));
    assert_eq!(cpu.pc(), START);
}

#[test]
fn an_undefined_postbyte_follows_the_policy() {
    // LDA with postbyte $87 (undefined).
    let mut cpu = cpu_with(&[0xA6, 0x87], InvalidOpcodePolicy::Error);
    cpu.regs.x = 0x2000;
    assert!(matches!(
        cpu.step(None, true),
        StepResult::Error(ExecError::InvalidOpcode {
            addr: START,
            opcode: 0xA6
        })
    ));
    assert_eq!(cpu.pc(), START);

    let mut cpu = self::cpu(&[0xA6, 0x87]);
    assert_eq!(step(&mut cpu), 4);
    assert_eq!(cpu.pc(), START + 2);
}

#[test]
fn indirect_auto_increment_by_one_is_undefined() {
    // LDA [,X+]: the indirect form of ,X+ doesn't exist. X is untouched.
    let mut cpu = cpu_with(&[0xA6, 0x90], InvalidOpcodePolicy::Error);
    cpu.regs.x = 0x2000;
    assert!(matches!(cpu.step(None, true), StepResult::Error(_)));
    assert_eq!(cpu.registers().x, 0x2000);
}

// --- debugger and exec hooks ---

#[test]
fn subroutine_call_lengths() {
    let cases: &[(&[u8], Option<u16>)] = &[
        (&[0x8D, 0x00], Some(2)),             // BSR
        (&[0x17, 0x00, 0x00], Some(3)),       // LBSR
        (&[0x9D, 0x10], Some(2)),             // JSR <
        (&[0xBD, 0x20, 0x00], Some(3)),       // JSR >
        (&[0xAD, 0x84], Some(2)),             // JSR ,X
        (&[0xAD, 0x88, 0x10], Some(3)),       // JSR n8,X
        (&[0xAD, 0x9D, 0x01, 0x00], Some(4)), // JSR [n16,PCR]
        (&[0x7E, 0x20, 0x00], None),          // JMP
        (&[0x3F], None),                      // SWI
    ];
    for &(program, expected) in cases {
        let cpu = cpu(program);
        assert_eq!(cpu.subroutine_call_len(START), expected, "{program:02X?}");
    }
}

#[test]
fn returned_once_s_rises_above_the_mark() {
    let mut cpu = cpu(&[]);
    let mark = cpu.stack_mark();
    assert!(!cpu.has_returned_from(mark));
    cpu.regs.s = STACK - 2;
    assert!(!cpu.has_returned_from(mark));
    cpu.regs.s = STACK + 2;
    assert!(cpu.has_returned_from(mark));
    // Wrapping: a mark near the top of memory and S just past zero.
    cpu.regs.s = 0x0001;
    assert!(cpu.has_returned_from(0xFFFE));
}

#[test]
fn stack_window_is_clamped_to_the_address_space() {
    let mut cpu = cpu(&[]);
    assert_eq!(cpu.stack_window(), (STACK - 32, 128));
    cpu.regs.s = 0x0010;
    assert_eq!(cpu.stack_window(), (0x0000, 0x10 + 96));
    cpu.regs.s = 0xFFF0;
    assert_eq!(cpu.stack_window(), (0xFFD0, 0x30));
}

#[test]
fn register_and_flag_names() {
    for (name, id) in [
        ("a", REG_A),
        ("B", REG_B),
        ("d", REG_D),
        ("X", REG_X),
        ("y", REG_Y),
        ("U", REG_U),
        ("s", REG_S),
        ("Pc", REG_PC),
        ("dp", REG_DP),
        ("CC", REG_CC),
    ] {
        assert_eq!(map_register_name(name), Some(id), "{name}");
    }
    assert_eq!(map_register_name("P"), None);
    assert_eq!(map_flag_name("e"), Some(0x80));
    assert_eq!(map_flag_name("F"), Some(0x40));
    assert_eq!(map_flag_name("c"), Some(0x01));
    assert_eq!(map_flag_name("D"), None);
}

#[test]
fn watch_context_reads_registers_and_memory_big_endian() {
    let mut cpu = cpu(&[]);
    write(&mut cpu, 0x2000, &[0x12, 0x34, 0x56, 0x78]);
    cpu.regs.set_d(0x80FF);
    cpu.regs.cc = ConditionCodes::N | ConditionCodes::C;
    let ctx = CpuWatchContext {
        regs: &cpu.regs,
        bus: cpu.machine.bus(),
    };
    assert_eq!(ctx.read_mem_u32(0x2000, 1), 0x12);
    assert_eq!(ctx.read_mem_u32(0x2000, 2), 0x1234);
    assert_eq!(ctx.read_mem_u32(0x2000, 4), 0x1234_5678);
    assert_eq!(ctx.read_mem_i32(0x2002, 2), 0x5678);
    assert_eq!(ctx.read_register_u32(REG_D), 0x80FF);
    assert_eq!(ctx.read_register_u32(REG_A), 0x80);
    assert_eq!(ctx.read_register_i32(REG_D), (-0x7F01i32) as u32);
    assert_eq!(ctx.read_register_i32(REG_B), (-1i32) as u32);
    assert_eq!(ctx.read_flag(0x08), 1);
    assert_eq!(ctx.read_flag(0x04), 0);
}

// --- hardware interrupts ---

mod interrupts {
    use super::*;
    use emma6k_core::IrqSource;

    const HANDLER: u16 = 0x2000;
    const IRQ_SOURCE: IrqSource = IrqSource(1);
    const FIRQ_SOURCE: IrqSource = IrqSource(2);

    /// A CPU with `program` at `START`, NMI armed, no interrupts masked, an
    /// `RTI` at `HANDLER`, and every hardware vector pointing there. Source 2
    /// is routed to FIRQ.
    fn interruptible(program: &[u8]) -> Cpu {
        let mut cpu = cpu(program);
        for vector in [NMI_VECTOR, FIRQ_VECTOR, IRQ_VECTOR] {
            write(&mut cpu, vector, &HANDLER.to_be_bytes());
        }
        write(&mut cpu, HANDLER, &[0x3B]);
        cpu.nmi_armed = true;
        cpu.regs.cc = ConditionCodes::empty();
        cpu.interrupts_mut().set_firq_mask(1 << FIRQ_SOURCE.0);
        cpu
    }

    /// Steps once, expecting an interrupt service; returns its cycles.
    fn service(cpu: &mut Cpu) -> u64 {
        let before = cpu.cycles();
        match cpu.step(None, true) {
            StepResult::Executed(op) if op.mnemonic == opcodes::Mnemonic::Interrupt => {
                cpu.cycles() - before
            }
            StepResult::Executed(op) => panic!("executed {:?} instead", op.mnemonic),
            _ => panic!("expected an interrupt service"),
        }
    }

    fn stacked(cpu: &Cpu, len: u16) -> Vec<u8> {
        let s = cpu.registers().s;
        (0..len).map(|i| peek(cpu, s + i)).collect()
    }

    #[test]
    fn irq_stacks_the_entire_state_and_sets_i() {
        let mut cpu = interruptible(&[0x12]);
        cpu.regs.a = 0x42;
        cpu.interrupts_mut().assert_irq(IRQ_SOURCE);
        assert_eq!(service(&mut cpu), 19);
        assert_eq!(cpu.pc(), HANDLER);
        assert_eq!(cpu.registers().s, STACK - 12);
        let frame = stacked(&cpu, 12);
        assert_eq!(frame[0], 0x80, "stacked CC: E");
        assert_eq!(frame[1], 0x42, "stacked A");
        assert_eq!(&frame[10..], &START.to_be_bytes(), "stacked PC");
        assert_eq!(cc(&cpu).to_byte(), 0x90, "live CC: E and I, F unchanged");

        cpu.interrupts_mut().release_irq(IRQ_SOURCE);
        assert_eq!(step(&mut cpu), 15, "RTI pulls the entire state");
        assert_eq!(cpu.pc(), START);
        assert_eq!(cpu.registers().s, STACK);
    }

    #[test]
    fn firq_stacks_only_pc_and_cc_and_sets_i_and_f() {
        let mut cpu = interruptible(&[0x12]);
        cpu.regs.cc = ConditionCodes::E | ConditionCodes::C;
        cpu.interrupts_mut().assert_irq(FIRQ_SOURCE);
        assert_eq!(service(&mut cpu), 10);
        assert_eq!(cpu.pc(), HANDLER);
        assert_eq!(cpu.registers().s, STACK - 3);
        let frame = stacked(&cpu, 3);
        assert_eq!(frame[0], 0x01, "stacked CC: E cleared, C kept");
        assert_eq!(&frame[1..], &START.to_be_bytes());
        assert_eq!(cc(&cpu).to_byte(), 0x51, "live CC: F, I and C");

        cpu.interrupts_mut().release_irq(FIRQ_SOURCE);
        assert_eq!(step(&mut cpu), 6, "RTI pulls CC and PC");
        assert_eq!(cpu.pc(), START);
        assert_eq!(cc(&cpu).to_byte(), 0x01);
    }

    #[test]
    fn nmi_stacks_the_entire_state_and_sets_i_and_f() {
        let mut cpu = interruptible(&[0x12]);
        cpu.interrupts_mut().signal_nmi();
        assert_eq!(service(&mut cpu), 19);
        assert_eq!(cpu.pc(), HANDLER);
        assert_eq!(cpu.registers().s, STACK - 12);
        assert_eq!(cc(&cpu).to_byte(), 0xD0, "live CC: E, F and I");
        assert!(!cpu.interrupts().nmi_pending(), "the edge is consumed");
    }

    #[test]
    fn interrupts_use_their_own_vectors() {
        for (vector, trigger) in [(NMI_VECTOR, 0), (FIRQ_VECTOR, 1), (IRQ_VECTOR, 2)] {
            let mut cpu = interruptible(&[0x12]);
            write(&mut cpu, vector, &[0x24, 0x68]);
            match trigger {
                0 => cpu.interrupts_mut().signal_nmi(),
                1 => cpu.interrupts_mut().assert_irq(FIRQ_SOURCE),
                _ => cpu.interrupts_mut().assert_irq(IRQ_SOURCE),
            }
            service(&mut cpu);
            assert_eq!(cpu.pc(), 0x2468, "vector ${vector:04X}");
        }
    }

    #[test]
    fn nmi_waits_until_a_program_loads_s() {
        // NOP; LDS #STACK; NOP. Disarm NMI as a reset does.
        let [hi, lo] = STACK.to_be_bytes();
        let mut cpu = interruptible(&[0x12, 0x10, 0xCE, hi, lo, 0x12]);
        cpu.nmi_armed = false;
        cpu.interrupts_mut().signal_nmi();
        step(&mut cpu); // NOP: the NMI stays pending
        assert!(cpu.interrupts().nmi_pending());
        step(&mut cpu); // LDS arms NMI
        assert_eq!(service(&mut cpu), 19);
        assert_eq!(cpu.pc(), HANDLER);
    }

    #[test]
    fn every_explicit_load_of_s_arms_nmi() {
        let [hi, lo] = STACK.to_be_bytes();
        let programs: &[(&str, &[u8])] = &[
            ("LDS", &[0x10, 0xCE, hi, lo]),
            ("LEAS", &[0x32, 0x62]), // LEAS 2,S
            ("TFR X,S", &[0x1F, 0x14]),
            ("EXG X,S", &[0x1E, 0x14]),
            ("PULU S", &[0x37, 0x40]),
        ];
        for &(name, program) in programs {
            let mut cpu = interruptible(program);
            cpu.nmi_armed = false;
            cpu.regs.u = 0x4000;
            step(&mut cpu);
            assert!(cpu.nmi_armed, "{name}");
        }
        // A push or pull through S doesn't.
        let mut cpu = interruptible(&[0x34, 0x02, 0x35, 0x02]);
        cpu.nmi_armed = false;
        steps(&mut cpu, 2);
        assert!(!cpu.nmi_armed, "PSHS/PULS");
    }

    #[test]
    fn reset_disarms_nmi() {
        let mut cpu = interruptible(&[0x12]);
        cpu.reset().unwrap();
        assert!(!cpu.nmi_armed);
    }

    #[test]
    fn nmi_comes_before_firq_and_firq_before_irq() {
        let mut cpu = interruptible(&[0x12]);
        write(&mut cpu, NMI_VECTOR, &[0x30, 0x00]);
        write(&mut cpu, FIRQ_VECTOR, &[0x31, 0x00]);
        write(&mut cpu, IRQ_VECTOR, &[0x32, 0x00]);
        cpu.interrupts_mut().signal_nmi();
        cpu.interrupts_mut().assert_irq(FIRQ_SOURCE);
        cpu.interrupts_mut().assert_irq(IRQ_SOURCE);
        service(&mut cpu);
        assert_eq!(cpu.pc(), 0x3000, "NMI first");

        let mut cpu = interruptible(&[0x12]);
        write(&mut cpu, FIRQ_VECTOR, &[0x31, 0x00]);
        write(&mut cpu, IRQ_VECTOR, &[0x32, 0x00]);
        cpu.interrupts_mut().assert_irq(FIRQ_SOURCE);
        cpu.interrupts_mut().assert_irq(IRQ_SOURCE);
        service(&mut cpu);
        assert_eq!(cpu.pc(), 0x3100, "FIRQ before IRQ");
    }

    #[test]
    fn firq_preempts_an_irq_handler() {
        // IRQ sets only I, so a FIRQ can interrupt the IRQ handler.
        let mut cpu = interruptible(&[0x12]);
        write(&mut cpu, IRQ_VECTOR, &[0x30, 0x00]);
        write(&mut cpu, 0x3000, &[0x12]);
        cpu.interrupts_mut().assert_irq(IRQ_SOURCE);
        service(&mut cpu);
        assert_eq!(cpu.pc(), 0x3000);
        cpu.interrupts_mut().assert_irq(FIRQ_SOURCE);
        assert_eq!(service(&mut cpu), 10);
        assert_eq!(cpu.pc(), HANDLER);
        assert_eq!(cpu.registers().s, STACK - 15);
    }

    #[test]
    fn masked_interrupts_are_not_serviced() {
        let mut cpu = interruptible(&[0x12, 0x12]);
        cpu.regs.cc = ConditionCodes::I | ConditionCodes::F;
        cpu.interrupts_mut().assert_irq(IRQ_SOURCE);
        cpu.interrupts_mut().assert_irq(FIRQ_SOURCE);
        assert_eq!(step(&mut cpu), 2);
        assert_eq!(cpu.pc(), START + 1);
    }

    #[test]
    fn a_pulsed_firq_source_is_serviced_once() {
        let mut cpu = interruptible(&[0x12, 0x12]);
        cpu.interrupts_mut().assert_irq_pulse(FIRQ_SOURCE);
        service(&mut cpu);
        step(&mut cpu); // RTI
        assert_eq!(cpu.pc(), START);
        assert!(!cpu.interrupts().firq_active());
        step(&mut cpu);
        assert_eq!(cpu.pc(), START + 1, "the NOP runs; no second FIRQ");
    }

    // --- CWAI and SYNC ---

    #[test]
    fn cwai_wakes_on_an_unmasked_interrupt_without_stacking_again() {
        // CWAI #$EF clears I; then NOP.
        let mut cpu = interruptible(&[0x3C, 0xEF, 0x12]);
        cpu.regs.cc = ConditionCodes::I | ConditionCodes::F;
        step(&mut cpu);
        assert!(matches!(cpu.step(None, true), StepResult::Waiting));
        cpu.interrupts_mut().assert_irq(IRQ_SOURCE);
        assert_eq!(service(&mut cpu), CWAI_WAKE_CYCLES as u64);
        assert_eq!(cpu.halt_state(), HaltState::Running);
        assert_eq!(cpu.pc(), HANDLER);
        assert_eq!(cpu.registers().s, STACK - 12, "stacked once, by CWAI");

        cpu.interrupts_mut().release_irq(IRQ_SOURCE);
        assert_eq!(step(&mut cpu), 15);
        assert_eq!(cpu.pc(), START + 2);
    }

    #[test]
    fn firq_after_cwai_returns_with_the_entire_state() {
        // CWAI #$BF clears F. E stays set, so RTI pulls everything.
        let mut cpu = interruptible(&[0x3C, 0xBF, 0x12]);
        cpu.regs.cc = ConditionCodes::I | ConditionCodes::F;
        cpu.regs.a = 0x42;
        step(&mut cpu);
        cpu.regs.a = 0;
        cpu.interrupts_mut().assert_irq(FIRQ_SOURCE);
        assert_eq!(service(&mut cpu), CWAI_WAKE_CYCLES as u64);
        cpu.interrupts_mut().release_irq(FIRQ_SOURCE);
        assert_eq!(step(&mut cpu), 15);
        assert_eq!(cpu.registers().a, 0x42);
        assert_eq!(cpu.pc(), START + 2);
    }

    #[test]
    fn cwai_keeps_waiting_through_a_masked_interrupt() {
        // CWAI #$FF leaves I and F set.
        let mut cpu = interruptible(&[0x3C, 0xFF]);
        cpu.regs.cc = ConditionCodes::I | ConditionCodes::F;
        step(&mut cpu);
        cpu.interrupts_mut().assert_irq(IRQ_SOURCE);
        cpu.interrupts_mut().assert_irq(FIRQ_SOURCE);
        assert!(matches!(cpu.step(None, true), StepResult::Waiting));
        // NMI isn't maskable.
        cpu.interrupts_mut().signal_nmi();
        assert_eq!(service(&mut cpu), CWAI_WAKE_CYCLES as u64);
    }

    #[test]
    fn sync_resumes_without_servicing_a_masked_interrupt() {
        // SYNC; NOP with I set.
        let mut cpu = interruptible(&[0x13, 0x12]);
        cpu.regs.cc = ConditionCodes::I;
        step(&mut cpu);
        assert!(matches!(cpu.step(None, true), StepResult::Waiting));
        cpu.interrupts_mut().assert_irq(IRQ_SOURCE);
        assert_eq!(step(&mut cpu), 2, "the NOP after SYNC runs");
        assert_eq!(cpu.pc(), START + 2);
        assert_eq!(cpu.registers().s, STACK, "nothing stacked");
    }

    #[test]
    fn sync_services_an_unmasked_interrupt() {
        let mut cpu = interruptible(&[0x13, 0x12]);
        step(&mut cpu);
        cpu.interrupts_mut().assert_irq(FIRQ_SOURCE);
        assert_eq!(service(&mut cpu), 10);
        assert_eq!(cpu.pc(), HANDLER);
        assert_eq!(&stacked(&cpu, 3)[1..], &(START + 1).to_be_bytes());
    }

    #[test]
    fn a_disarmed_nmi_does_not_end_sync() {
        let mut cpu = interruptible(&[0x13, 0x12]);
        cpu.nmi_armed = false;
        step(&mut cpu);
        cpu.interrupts_mut().signal_nmi();
        assert!(matches!(cpu.step(None, true), StepResult::Waiting));
    }
}

// --- the exit device ---

/// A 6809 with an `exit` device at $9000 and a program that writes `EXIT`
/// and `status` to it.
fn cpu_that_exits(status: u8) -> Cpu {
    use emma6k_core::device::{DeviceId, ExitDevice};
    let bus = Bus::config()
        .ram_with_fill(AddressRange::new(0x0000, 0xFFFF), 0)
        .unwrap()
        .device(
            AddressRange::new(0x9000, 0x9000),
            DeviceId(1),
            Box::new(ExitDevice::new(0x9000)),
        )
        .unwrap()
        .build();
    let mut cpu = Cpu::builder().bus(bus).build().unwrap();
    write(&mut cpu, RESET_VECTOR, &START.to_be_bytes());
    let mut program = Vec::new();
    for b in [b'E', b'X', b'I', b'T', status] {
        program.extend_from_slice(&[0x86, b, 0xB7, 0x90, 0x00]); // LDA #b; STA $9000
    }
    program.push(0x12); // NOP, never reached
    write(&mut cpu, START, &program);
    cpu.reset().unwrap();
    cpu
}

#[test]
fn an_exit_request_stops_the_cpu_with_its_status() {
    let mut cpu = cpu_that_exits(7);
    steps(&mut cpu, 10);
    assert_eq!(cpu.machine().exit_status(), None);
    assert!(matches!(cpu.step(None, true), StepResult::Stopped));
    assert_eq!(cpu.halt_state(), HaltState::Stopped);
    assert_eq!(cpu.machine().exit_status(), Some(7));
    assert_eq!(cpu.regs.pc, START + 25); // stopped before the NOP
    // It stays stopped.
    assert!(matches!(cpu.step(None, true), StepResult::Stopped));
}

#[test]
fn after_an_exit_only_a_reset_wakes_the_cpu() {
    let mut cpu = cpu_that_exits(0);
    while !matches!(cpu.step(None, true), StepResult::Stopped) {}
    cpu.regs.cc = ConditionCodes::from_byte(0); // unmask IRQ and FIRQ
    cpu.interrupts_mut().assert_irq(emma6k_core::IrqSource(2));
    cpu.interrupts_mut().signal_nmi();
    assert!(matches!(cpu.step(None, true), StepResult::Stopped));
    cpu.interrupts_mut().signal_reset();
    assert!(matches!(cpu.step(None, true), StepResult::Reset));
    assert_eq!(cpu.halt_state(), HaltState::Running);
    assert_eq!(cpu.machine().exit_status(), None);
}

// --- gaps found by mutation testing ---
//
// `cargo mutants -p emma09` showed which changes to the CPU no test caught;
// Schwotzer's suite checks results but rarely that an instruction clears a
// flag that was already set. These tests close those gaps.

use emma6k_core::bus::{InterruptController, RomWritePolicy};
use std::sync::{Arc, Mutex};

/// A flag case: name, program, A, B, CC in, A out, CC out.
type FlagCase = (&'static str, &'static [u8], u8, u8, u8, u8, u8);

/// Each case runs one instruction from `cc_in` with A = `a` and B = `b`, then
/// checks A and the whole CC. Starting with the affected flags set checks
/// that the instruction clears them.
#[test]
fn instructions_set_and_clear_exactly_their_flags() {
    const N: u8 = 0x08;
    const Z: u8 = 0x04;
    const V: u8 = 0x02;
    const C: u8 = 0x01;
    #[rustfmt::skip]
    let cases: &[FlagCase] = &[
        // name, program, A, B, CC in, A out, CC out
        ("ANDA", &[0x84, 0x0F], 0x01, 0, N | Z | V, 0x01, 0),
        ("BITA", &[0x85, 0x0F], 0x01, 0, N | Z | V, 0x01, 0),
        ("EORA", &[0x88, 0xFF], 0x0F, 0, Z | V | C, 0xF0, N | C),
        ("ORA", &[0x8A, 0x02], 0x01, 0, N | Z | V, 0x03, 0),
        ("COMA", &[0x43], 0x0F, 0, V, 0xF0, N | C),
        ("ASRA", &[0x47], 0x02, 0, N | Z | C, 0x01, 0),
        ("ASLA $01", &[0x48], 0x01, 0, N | Z | V | C, 0x02, 0),
        ("ASLA $40", &[0x48], 0x40, 0, 0, 0x80, N | V),
        ("ASLA $80", &[0x48], 0x80, 0, 0, 0x00, Z | V | C),
        ("ASLA $C0", &[0x48], 0xC0, 0, 0, 0x80, N | C),
        ("TSTA", &[0x4D], 0x01, 0, Z | V | C, 0x01, C),
        ("CLRA", &[0x4F], 0x55, 0, N | V | C, 0x00, Z),
        ("SEX", &[0x1D], 0xAA, 0x01, N | Z, 0x00, 0),
        ("ORCC", &[0x1A, 0x05], 0, 0, 0x11, 0, 0x15),
        ("DAA $09", &[0x19], 0x09, 0, 0, 0x09, 0),
        ("DAA $8A", &[0x19], 0x8A, 0, 0, 0x90, N),
        ("DAA $90", &[0x19], 0x90, 0, 0, 0x90, N),
        ("DAA $A0 with C", &[0x19], 0xA0, 0, C, 0x00, Z | C),
        ("STA", &[0xB7, 0x20, 0x00], 0x01, 0, N | Z | V, 0x01, 0),
        ("STB", &[0xF7, 0x20, 0x00], 0, 0x01, N | Z | V, 0, 0),
        ("LDD #", &[0xCC, 0x00, 0x01], 0, 0, N | Z | V, 0x00, 0),
        ("STD", &[0xFD, 0x20, 0x00], 0x00, 0x01, N | Z | V, 0x00, 0),
    ];
    for &(name, program, a, b, cc_in, a_out, cc_out) in cases {
        let mut cpu = cpu(program);
        cpu.regs.a = a;
        cpu.regs.b = b;
        cpu.regs.cc = ConditionCodes::from_byte(cc_in);
        step(&mut cpu);
        assert_eq!(cpu.registers().a, a_out, "{name}: A");
        assert_eq!(cc(&cpu).to_byte(), cc_out, "{name}: CC");
    }
}

#[test]
fn tst_memory_sets_n_z_and_clears_v_but_keeps_c() {
    // TST $2000, where $2000 holds $80.
    let mut cpu = cpu(&[0x7D, 0x20, 0x00]);
    write(&mut cpu, 0x2000, &[0x80]);
    cpu.regs.cc = ConditionCodes::V | ConditionCodes::C;
    step(&mut cpu);
    assert_eq!(cc(&cpu), ConditionCodes::N | ConditionCodes::C);
    assert_eq!(peek(&cpu, 0x2000), 0x80);
}

#[test]
fn stu_stores_u_and_clears_v() {
    // STU $2000
    let mut cpu = cpu(&[0xFF, 0x20, 0x00]);
    cpu.regs.u = 0x1234;
    cpu.regs.cc = ConditionCodes::N | ConditionCodes::Z | ConditionCodes::V;
    step(&mut cpu);
    assert_eq!([peek(&cpu, 0x2000), peek(&cpu, 0x2001)], [0x12, 0x34]);
    assert_eq!(cc(&cpu), ConditionCodes::empty());
}

/// Every branch condition after CMPA, against what the comparison means:
/// signed and unsigned order, equality, the sign of the difference, and
/// signed overflow. Short and long branches share the conditions.
#[test]
fn every_branch_condition_follows_its_comparison() {
    let values = [0x00u8, 0x01, 0x7F, 0x80, 0x81, 0xFF];
    for a in values {
        for b in values {
            let (sa, sb) = (a as i8, b as i8);
            let difference = a.wrapping_sub(b) as i8;
            let overflow = sa.checked_sub(sb).is_none();
            let expected = [
                true,            // BRA
                false,           // BRN
                a > b,           // BHI
                a <= b,          // BLS
                a >= b,          // BHS
                a < b,           // BLO
                a != b,          // BNE
                a == b,          // BEQ
                !overflow,       // BVC
                overflow,        // BVS
                difference >= 0, // BPL
                difference < 0,  // BMI
                sa >= sb,        // BGE
                sa < sb,         // BLT
                sa > sb,         // BGT
                sa <= sb,        // BLE
            ];
            for (condition, &taken) in expected.iter().enumerate() {
                // LDA #a; CMPA #b; Bcc +$10
                let branch = 0x20 | condition as u8;
                let mut cpu = cpu(&[0x86, a, 0x81, b, branch, 0x10]);
                steps(&mut cpu, 3);
                let next = START + 6;
                let target = if taken { next + 0x10 } else { next };
                assert_eq!(
                    cpu.pc(),
                    target,
                    "${a:02X} vs ${b:02X}, branch ${branch:02X}"
                );
            }
        }
    }
}

#[test]
fn tfr_reads_s_pc_and_undefined_registers() {
    // TFR S,X; TFR PC,Y; TFR 6,U (undefined 16-bit code: all ones).
    let mut cpu = cpu(&[0x1F, 0x41, 0x1F, 0x52, 0x1F, 0x63]);
    cpu.regs.s = 0x7000;
    steps(&mut cpu, 3);
    assert_eq!(cpu.registers().x, 0x7000);
    assert_eq!(cpu.registers().y, START + 4); // PC after the TFR
    assert_eq!(cpu.registers().u, 0xFFFF);
}

#[test]
fn indexed_auto_increment_updates_u_alone() {
    // LDA ,U+
    let mut cpu = cpu(&[0xA6, 0xC0]);
    write(&mut cpu, 0x3000, &[0x5A]);
    cpu.regs.u = 0x3000;
    step(&mut cpu);
    assert_eq!(cpu.registers().a, 0x5A);
    assert_eq!(cpu.registers().u, 0x3001);
    assert_eq!(cpu.registers().s, STACK);
}

/// Indexed forms add the postbyte's cycles (here 1, for a 5-bit offset) to
/// every instruction group's base.
#[test]
fn indexed_operands_add_their_cycles_to_every_group() {
    let cases: &[(&str, &[u8], u64)] = &[
        ("NEG 5,X", &[0x60, 0x05], 7),
        ("TST 5,X", &[0x6D, 0x05], 7),
        ("JMP 5,X", &[0x6E, 0x05], 4),
        ("JSR 5,X", &[0xAD, 0x05], 8),
        ("LDA 5,X", &[0xA6, 0x05], 5),
        ("LDB 5,X", &[0xE6, 0x05], 5),
        ("STA 5,X", &[0xA7, 0x05], 5),
        ("ADDD 5,X", &[0xE3, 0x05], 7),
        ("LDD 5,X", &[0xEC, 0x05], 6),
        ("STD 5,X", &[0xED, 0x05], 6),
        ("LEAX 5,X", &[0x30, 0x05], 5),
        // PSHS adds one cycle a byte: U is the first register pushed.
        ("PSHS U", &[0x34, 0x40], 7),
    ];
    for &(name, program, expected) in cases {
        let mut cpu = cpu(program);
        cpu.regs.x = 0x3000;
        assert_eq!(step(&mut cpu), expected, "{name}");
    }
}

#[test]
fn every_flag_and_register_has_a_watch_name() {
    for (name, mask) in [
        ("E", 0x80),
        ("f", 0x40),
        ("H", 0x20),
        ("i", 0x10),
        ("N", 0x08),
        ("z", 0x04),
        ("V", 0x02),
        ("c", 0x01),
    ] {
        assert_eq!(map_flag_name(name), Some(mask), "{name}");
        assert_eq!(
            <Cpu as Processor>::map_flag_name(name),
            Some(mask),
            "{name}"
        );
    }
    assert_eq!(<Cpu as Processor>::map_register_name("u"), Some(REG_U));
    assert_eq!(<Cpu as Processor>::map_register_name("w"), None);
}

#[test]
fn the_watch_context_reads_every_register() {
    let mut cpu = cpu(&[]);
    cpu.regs.x = 0x1111;
    cpu.regs.y = 0x2222;
    cpu.regs.u = 0x3333;
    cpu.regs.s = 0x4444;
    cpu.regs.pc = 0x5555;
    cpu.regs.dp = 0x66;
    cpu.regs.cc = ConditionCodes::from_byte(0x77);
    let ctx = CpuWatchContext {
        regs: &cpu.regs,
        bus: cpu.machine.bus(),
    };
    for (id, value) in [
        (REG_X, 0x1111),
        (REG_Y, 0x2222),
        (REG_U, 0x3333),
        (REG_S, 0x4444),
        (REG_PC, 0x5555),
        (REG_DP, 0x66),
        (REG_CC, 0x77),
    ] {
        assert_eq!(ctx.read_register_u32(id), value, "register {id}");
    }
}

#[test]
fn the_processor_reset_reads_the_reset_vector() {
    let mut cpu = cpu(&[]);
    cpu.regs.pc = 0x4000;
    Processor::reset(&mut cpu).unwrap();
    assert_eq!(cpu.pc(), START);
}

#[test]
fn flush_trace_flushes_the_callback() {
    struct Flushes(Arc<std::sync::atomic::AtomicUsize>);
    impl TraceCallback<Registers> for Flushes {
        fn record(&mut self, _rec: TraceRecord) {}
        fn flush(&mut self) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let flushes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut cpu = cpu(&[]);
    cpu.set_trace_callback(Some(Box::new(Flushes(flushes.clone()))));
    cpu.flush_trace();
    Processor::flush_trace(&mut cpu);
    assert_eq!(flushes.load(std::sync::atomic::Ordering::Relaxed), 2);
}

#[test]
fn a_failed_write_leaves_no_write_record() {
    struct Writes(Arc<Mutex<Vec<TraceRecord>>>);
    impl TraceCallback<Registers> for Writes {
        fn record(&mut self, rec: TraceRecord) {
            self.0.lock().unwrap().push(rec);
        }
    }
    // RAM below $8000, ROM that refuses writes above it.
    let bus = Bus::config()
        .ram_with_fill(AddressRange::new(0x0000, 0x7FFF), 0)
        .unwrap()
        .rom_with_write_policy(
            AddressRange::new(0x8000, 0xFFFF),
            vec![0; 0x8000],
            RomWritePolicy::Error,
        )
        .unwrap()
        .build();
    let mut cpu = Cpu::builder().bus(bus).build().unwrap();
    write(&mut cpu, START, &[0xB7, 0x90, 0x00]); // STA $9000
    cpu.regs.pc = START;
    let records = Arc::new(Mutex::new(Vec::new()));
    cpu.set_trace_callback(Some(Box::new(Writes(records.clone()))));
    assert!(matches!(cpu.step(None, false), StepResult::Error(_)));
    let records = records.lock().unwrap();
    assert!(!records.is_empty());
    assert!(
        !records
            .iter()
            .any(|r| matches!(r.kind, TraceKind::Write { .. }))
    );
}

#[test]
fn builder_options_survive_later_calls() {
    struct Redirect;
    impl VectorResolver for Redirect {
        fn resolve(&self, vector_addr: u16, _: &InterruptController) -> u16 {
            if vector_addr == RESET_VECTOR {
                0x0100
            } else {
                vector_addr
            }
        }
    }
    let bus = Bus::config()
        .ram_with_fill(AddressRange::new(0x0000, 0xFFFF), 0)
        .unwrap()
        .build();
    let (sender, rx) = emma6k_core::log_channel(4);
    // Each option is set after the bus, so dropping the builder's earlier
    // state would lose the bus.
    let mut cpu = Cpu::builder()
        .bus(bus)
        .clock_speed(ClockSpeed::mhz(1.0))
        .vector_resolver(Box::new(Redirect))
        .log_sender(sender)
        .build()
        .unwrap();
    assert_eq!(cpu.machine().clock_speed(), ClockSpeed::mhz(1.0));
    write(&mut cpu, 0x0100, &[0x12, 0x34]);
    cpu.reset().unwrap();
    assert_eq!(cpu.pc(), 0x1234);
    // The reset logged through the sender.
    assert_eq!(rx.try_recv().unwrap().message, "6809 CPU reset");
}
