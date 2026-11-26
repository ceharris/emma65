//! The MOS/WDC 65C02: everything in the emulator that is specific to the
//! 6502 family.
//!
//! - [`cpu`]: the CPU itself (decoder, ALU, status register, variants), its
//!   [`Processor`](crate::emulator::Processor) implementation, and the binary
//!   execution trace, whose register record is the 6502's.
//! - [`disassembler`] and [`assembler`]: the 65C02 instruction set.
//! - [`config`]: the top-level [`Config`](config::Config) schema with its
//!   `cpu-variant` selector, and building a 6502 [`EmulatorSession`].
//! - [`templates`]: the bundled starter-profile templates, whose ROM images
//!   are 6502 programs.
//!
//! Nothing outside this module names a 6502 type, apart from re-exports that
//! keep existing `emma65::emulator::*` paths working, and tests.
pub mod assembler;
pub mod config;
pub mod cpu;
pub mod disassembler;
pub mod templates;

/// [`emulator::session::EmulatorSession`](crate::emulator::session::EmulatorSession)
/// for the 6502 [`Cpu`](cpu::Cpu).
pub type EmulatorSession = crate::emulator::session::EmulatorSession<cpu::Cpu>;
