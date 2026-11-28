//! What the session's CPU looks like to the panels: its [`CpuDescription`], and the
//! register access the Register and Trace panels build on.
//!
//! The frontend renders the Register panel, the Trace panel's register columns and the
//! status bar's interrupt controls from the description, rather than from 6502 constants
//! of its own. Only the 6502 is described so far.

use std::collections::BTreeMap;
use std::sync::Mutex;

use emma65::emulator::cpu::Registers;
use emma65::emulator::{CpuVariant, StatusRegister};
use tauri::State;

/// What a register is for, which decides where the panels show it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegisterRole {
    Data,
    Index,
    Stack,
    Pc,
    Flags,
}

/// One register, in the order the Register panel lists it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct RegisterDescription {
    /// The register's name, which is also its key in a snapshot's `values` and the name
    /// `set_register` takes.
    pub name: &'static str,
    pub width_bits: u8,
    pub role: RegisterRole,
}

/// One bit of the flags register.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct FlagDescription {
    /// The character shown when the flag is set. An unused bit's label is `-`, and the
    /// panels never mark it changed or let it be toggled.
    pub label: &'static str,
    /// The bit's position in the flags register, 0 for the least significant.
    pub bit: u8,
    pub name: &'static str,
}

/// The CPU's hardware stack.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StackDescription {
    /// The name of the stack pointer register.
    pub pointer: &'static str,
    /// True when the stack is confined to one page (the 6502's page 1).
    pub page_fixed: bool,
}

/// The byte order of a word in memory. The 6809, still to come, adds `Big`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ByteOrder {
    Little,
}

/// What the session's CPU looks like, sent to the frontend by `get_cpu_description` and
/// as the payload of the `session-loaded` event.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct CpuDescription {
    pub family: &'static str,
    pub variant: String,
    pub registers: Vec<RegisterDescription>,
    /// The flags register's bits, high bit first.
    pub flags: Vec<FlagDescription>,
    /// The name of the register `flags` describes.
    pub flags_register: &'static str,
    pub interrupt_lines: Vec<&'static str>,
    pub stack: StackDescription,
    pub byte_order: ByteOrder,
    pub max_instruction_len: u8,
    /// True when the Assembler panel can assemble for this CPU.
    pub assembler: bool,
}

/// The loaded session's CPU description, `None` before the first session loads.
pub struct CpuDescriptionState(pub Mutex<Option<CpuDescription>>);

/// Returns the loaded session's CPU description.
#[tauri::command]
pub fn get_cpu_description(
    description_state: State<CpuDescriptionState>,
) -> Result<CpuDescription, String> {
    description_state
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "CPU not ready".to_string())
}

/// The 6502's registers, in the Register panel's order.
const MOS6502_REGISTERS: [RegisterDescription; 6] = [
    register("A", 8, RegisterRole::Data),
    register("X", 8, RegisterRole::Index),
    register("Y", 8, RegisterRole::Index),
    register("PC", 16, RegisterRole::Pc),
    register("S", 8, RegisterRole::Stack),
    register("P", 8, RegisterRole::Flags),
];

const fn register(name: &'static str, width_bits: u8, role: RegisterRole) -> RegisterDescription {
    RegisterDescription {
        name,
        width_bits,
        role,
    }
}

/// Describes the 6502 `variant`.
pub fn describe_6502(variant: CpuVariant) -> CpuDescription {
    let flag = |label, bit, name| FlagDescription { label, bit, name };
    CpuDescription {
        family: "6502",
        variant: match variant {
            CpuVariant::Cmos65C02 => "65C02",
            CpuVariant::Wdc65C02 => "WDC65C02",
        }
        .to_string(),
        registers: MOS6502_REGISTERS.to_vec(),
        flags: vec![
            flag("N", 7, "Negative"),
            flag("V", 6, "Overflow"),
            flag("-", 5, "Unused"),
            flag("B", 4, "Break"),
            flag("D", 3, "Decimal"),
            flag("I", 2, "Interrupt disable"),
            flag("Z", 1, "Zero"),
            flag("C", 0, "Carry"),
        ],
        flags_register: "P",
        interrupt_lines: vec!["IRQ", "NMI"],
        stack: StackDescription {
            pointer: "S",
            page_fixed: true,
        },
        byte_order: ByteOrder::Little,
        max_instruction_len: 3,
        assembler: true,
    }
}

/// Every register's value, keyed by the description's names.
pub fn register_values(regs: &Registers) -> BTreeMap<&'static str, u32> {
    BTreeMap::from([
        ("A", u32::from(regs.a)),
        ("X", u32::from(regs.x)),
        ("Y", u32::from(regs.y)),
        ("PC", u32::from(regs.pc)),
        ("S", u32::from(regs.s)),
        ("P", u32::from(regs.p.to_byte())),
    ])
}

/// The flags register's value, for working out which flags a step changed.
pub fn flags_value(regs: &Registers) -> u32 {
    u32::from(regs.p.to_byte())
}

/// Sets the register `name` to `value`, which must fit the register's width.
pub fn set_register_value(regs: &mut Registers, name: &str, value: u32) -> Result<(), String> {
    let description = MOS6502_REGISTERS
        .iter()
        .find(|r| r.name == name)
        .ok_or_else(|| format!("Unknown register: {name}"))?;
    let max = (1u32 << description.width_bits) - 1;
    if value > max {
        return Err(format!("{name} value out of range: must be 0-{max}"));
    }
    // Both casts are exact: the width check above bounds `value`.
    let byte = value as u8;
    match name {
        "A" => regs.a = byte,
        "X" => regs.x = byte,
        "Y" => regs.y = byte,
        "S" => regs.s = byte,
        "P" => regs.p = StatusRegister::from_byte(byte) | StatusRegister::UNUSED,
        "PC" => regs.pc = value as u16,
        _ => unreachable!("every described register is handled"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zeroed() -> Registers {
        Registers {
            a: 0,
            x: 0,
            y: 0,
            s: 0,
            pc: 0,
            p: StatusRegister::UNUSED,
        }
    }

    #[test]
    fn the_6502_description_names_every_register_a_snapshot_has() {
        let description = describe_6502(CpuVariant::Wdc65C02);
        let values = register_values(&zeroed());
        let mut names: Vec<_> = description.registers.iter().map(|r| r.name).collect();
        names.sort_unstable();
        assert_eq!(names, values.keys().copied().collect::<Vec<_>>());
        assert_eq!(description.variant, "WDC65C02");
        assert_eq!(describe_6502(CpuVariant::Cmos65C02).variant, "65C02");
    }

    #[test]
    fn the_6502_flags_run_high_bit_first() {
        let flags = describe_6502(CpuVariant::Cmos65C02).flags;
        let labels: String = flags.iter().map(|f| f.label).collect();
        assert_eq!(labels, "NV-BDIZC");
        let bits: Vec<u8> = flags.iter().map(|f| f.bit).collect();
        assert_eq!(bits, [7, 6, 5, 4, 3, 2, 1, 0]);
    }

    #[test]
    fn set_register_value_writes_each_register() {
        let mut regs = zeroed();
        for (name, value) in [
            ("A", 0x12),
            ("X", 0x34),
            ("Y", 0x56),
            ("S", 0x78),
            ("PC", 0x9abc),
        ] {
            set_register_value(&mut regs, name, value).unwrap();
            assert_eq!(register_values(&regs)[name], value, "{name}");
        }
    }

    #[test]
    fn set_register_value_keeps_p_unused_bit_set() {
        let mut regs = zeroed();
        set_register_value(&mut regs, "P", 0x00).unwrap();
        assert_eq!(
            flags_value(&regs),
            u32::from(StatusRegister::UNUSED.to_byte())
        );
    }

    #[test]
    fn set_register_value_rejects_out_of_range_and_unknown_names() {
        let mut regs = zeroed();
        assert!(set_register_value(&mut regs, "A", 0x100).is_err());
        assert!(set_register_value(&mut regs, "PC", 0x1_0000).is_err());
        assert!(set_register_value(&mut regs, "PC", 0xffff).is_ok());
        assert!(set_register_value(&mut regs, "B", 0).is_err());
        assert!(set_register_value(&mut regs, "a", 0).is_err());
    }
}
