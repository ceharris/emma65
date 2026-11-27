//! The Motorola 6809 for the emma6k emulator.
//!
//! - [`cpu`]: the CPU (registers, opcode tables, indexed addressing, ALU and
//!   condition codes) and its [`Processor`](emma6k_core::Processor)
//!   implementation, which `emma6k_core::exec` runs and steps.
//!
//! The 6809 uses only `emma6k-core`'s public API, so it lives in its own crate,
//! like every CPU family after the 6502.
pub mod cpu;

pub use cpu::{Cpu, CpuBuilder, InvalidOpcodePolicy, Registers};

#[cfg(test)]
mod exec_tests;
