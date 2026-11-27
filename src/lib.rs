pub mod emulator;
pub mod mos6502;

// The watch pipeline lives in the core crate; this keeps `emma65::watch` working.
pub use emma6k_core::watch;

// The assembler and disassembler moved under `mos6502`; these keep the
// `emma65::assembler` and `emma65::disassembler` paths working.
pub use mos6502::{assembler, disassembler};

#[cfg(test)]
mod test_support;
