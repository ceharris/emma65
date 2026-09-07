//! Emulator for the (WDC)65C02 microprocessor and common peripherals.

pub mod bus;
pub mod config;
pub mod cpu;
pub mod device;
pub mod error;
pub mod exec;
mod logging;
mod session;
pub mod transport;

pub use bus::{
    AddressRange, Bus, BusConfig, BusOp, InterruptController, IrqSource, RomWritePolicy,
    SymbolSource, SymbolTable, UnmappedPolicy,
};
pub use config::{
    BuildError, Config, CpuVariantSpec, DeviceModule, DeviceModuleError, DeviceRegistry,
    DeviceSpec, DisplayFrameSlot, DisplayGeometry, DisplayGeometrySlot, ExpandedPathBuf,
    InstantiationContext, LcdDisplayFrameSlot, LcdDisplayGeometry, LcdDisplayGeometrySlot,
    LedMatrixFrameSlot, LedMatrixGeometry, LedMatrixGeometrySlot, RamModule, RomModule,
    TransportSlot, TransportSpec, TransportSpecFormat,
};
pub use cpu::StepResult;
pub use cpu::opcodes::{AddressingMode, DecodedOp, Mnemonic};
pub use cpu::status::StatusRegister;
pub use cpu::trace::{
    BinaryTraceReader, BinaryTraceWriter, ChannelTraceCallback, OverflowPolicy, TraceCallback,
    TraceKind, TraceRecord, spawn_trace_writer,
};
pub use cpu::variant::{CpuVariant, InvalidOpcodePolicy};
pub use cpu::vector::{IdentityVectorResolver, VectorResolver};
pub use cpu::{Cpu, CpuBuilder, Registers, map_flag_name, map_register_name};
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
    ClockSpeed, CpuLiveSnapshot, RunHandle, RunStopper, run, run_from, step_into,
    step_over_breakpoint, step_over_subroutine, step_return,
};
pub(crate) use logging::log_msg;
pub use logging::{
    LogCategory, LogLevel, LogRecord, LogSender, spawn_log_collector, spawn_log_writer,
};
pub use session::EmulatorSession;
pub use transport::{
    ChannelRelay, InternalPipeTransport, PipeTransport, PtyTransport, TcpSocketTransport,
    Transport, TransportError, TransportEvent, TransportRelay, TransportReporter,
    UnixSocketTransport,
};
