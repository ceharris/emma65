//! The `emma65::emulator` facade: the whole CPU-neutral core
//! ([`emma6k_core`]) plus the 6502's public types, under the paths they had
//! before the core moved into its own crate.
//!
//! Everything in `emma6k_core` is re-exported here unchanged (so
//! `emma65::emulator::bus::Bus`, `emma65::emulator::exec::run`, and so on keep
//! working), and the 6502 items below fill in the CPU-specific names
//! (`emma65::emulator::Cpu`, `emma65::emulator::cpu::StepResult`,
//! `emma65::emulator::Config`, ...). New code inside this crate should name
//! `emma6k_core` and `crate::mos6502` directly.

pub use emma6k_core::*;

/// `emma6k_core::config` plus the 6502's top-level [`Config`] schema and its
/// bundled [`templates`](crate::mos6502::templates).
pub mod config {
    pub use crate::mos6502::config::{Config, CpuVariantSpec};
    pub use crate::mos6502::templates;
    pub use emma6k_core::config::*;
}

// The 6502's public types, at their pre-split `emma65::emulator::*` paths.
pub use crate::mos6502::EmulatorSession;
pub use crate::mos6502::config::{Config, CpuVariantSpec};
pub use crate::mos6502::cpu;
pub use crate::mos6502::cpu::StepResult;
pub use crate::mos6502::cpu::opcodes::{AddressingMode, DecodedOp, Mnemonic};
pub use crate::mos6502::cpu::status::StatusRegister;
pub use crate::mos6502::cpu::trace::{
    BinaryTraceReader, BinaryTraceWriter, ChannelTraceCallback, OverflowPolicy, TraceCallback,
    TraceKind, TraceRecord, spawn_trace_writer,
};
pub use crate::mos6502::cpu::variant::{CpuVariant, InvalidOpcodePolicy};
pub use crate::mos6502::cpu::{
    Cpu, CpuBuilder, CpuLiveSnapshot, Registers, RunHandle, map_flag_name, map_register_name,
};
