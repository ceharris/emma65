//! The CPU-neutral emulator core: bus, devices, transports, configuration
//! framework, execution control, and the [`Processor`] trait a CPU implements
//! to run on it. The 6502 itself lives in [`crate::mos6502`].
//!
//! For compatibility, this module also re-exports the 6502's public types
//! under their pre-split paths (`emma65::emulator::Cpu`,
//! `emma65::emulator::cpu::StepResult`, and so on); see the section marked
//! "6502 re-exports" below.

pub mod bus;
pub mod config;
pub mod device;
pub mod error;
pub mod exec;
pub(crate) mod logging;
mod machine;
mod processor;
pub mod session;
pub mod transport;

pub use bus::vector::{IdentityVectorResolver, VectorResolver};
pub use bus::{
    AddressRange, Bus, BusConfig, BusOp, InterruptController, IrqSource, RomWritePolicy,
    SymbolSource, SymbolTable, UnmappedPolicy,
};
pub use config::{
    BuildError, DeviceModule, DeviceModuleError, DeviceRegistry, DeviceSpec, DisplayFrameSlot,
    DisplayGeometry, DisplayGeometrySlot, ExpandedPathBuf, InstantiationContext,
    LcdDisplayFrameSlot, LcdDisplayGeometry, LcdDisplayGeometrySlot, LedMatrixFrameSlot,
    LedMatrixGeometry, LedMatrixGeometrySlot, RamModule, RomModule, TransportSlot, TransportSpec,
    TransportSpecFormat,
};
pub use device::display::DisplayFrame;
pub use device::lcd_display::LcdDisplayFrame;
pub use device::led_matrix::LedMatrixFrame;
pub use device::{
    DeviceEvent, DeviceId, ErrorReceiver, ErrorSender, IoDevice, device_event_channel,
    log_device_event, log_device_events,
};
pub use device::{
    PtmAsciiProtocolDecoder, PtmAsciiProtocolEncoder, PtmBinaryProtocolDecoder,
    PtmBinaryProtocolEncoder, PtmProtocolMessage,
};
pub use device::{
    ViaAsciiProtocolDecoder, ViaAsciiProtocolEncoder, ViaBinaryProtocolDecoder,
    ViaBinaryProtocolEncoder, ViaProtocolMessage,
};
pub use error::{BusConfigError, BusError, CpuBuildError, ExecError};
pub use exec::{
    ClockSpeed, RunStopper, run, run_from, step_into, step_over_breakpoint, step_over_subroutine,
    step_return,
};
pub(crate) use logging::log_msg;
pub use logging::{
    LogCategory, LogLevel, LogRecord, LogSender, spawn_log_collector, spawn_log_writer,
};
pub use machine::MachineState;
pub use processor::{HaltState, Processor};
pub use transport::{
    ChannelRelay, InternalPipeTransport, PipeTransport, PtyTransport, TcpSocketTransport,
    Transport, TransportError, TransportEvent, TransportRelay, TransportReporter,
    UnixSocketTransport,
};

// --- 6502 re-exports ---
// Everything below names a `crate::mos6502` item. It keeps the pre-split
// `emma65::emulator::*` paths working for the debugger, the binaries, and the
// integration tests; the core above doesn't depend on it.
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
