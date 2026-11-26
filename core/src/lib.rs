//! emma6k-core: the CPU-neutral emulator core shared by every CPU family in
//! the emma6k project.
//!
//! - [`bus`], [`device`], [`transport`]: the memory bus, the `IoDevice` model
//!   and built-in devices, and the byte-stream transports devices talk through.
//! - [`config`]: the device spec and module registry, [`config::build_machine`],
//!   and the starter-template mechanism.
//! - [`exec`]: running and stepping a CPU, generic over the [`Processor`]
//!   trait; [`MachineState`] holds the state every CPU shares.
//! - [`watch`]: the watchpoint expression pipeline.
//!
//! A CPU family lives in its own crate, implements [`Processor`], and builds
//! its CPU around a [`MachineState`]. This crate never names a CPU-specific
//! type; the compiler enforces that.

pub mod bus;
pub mod config;
pub mod device;
pub mod error;
pub mod exec;
mod logging;
mod machine;
mod processor;
pub mod session;
pub mod transport;
pub mod watch;

// Shared source-text helpers, used by `watch` here and by CPU crates' assemblers.
pub mod location;
pub mod text;

#[cfg(test)]
mod test_support;

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
pub use logging::{
    LogCategory, LogLevel, LogRecord, LogSender, log_channel, spawn_log_collector, spawn_log_writer,
};
pub use machine::MachineState;
pub use processor::{HaltState, Processor};
pub use transport::{
    ChannelRelay, InternalPipeTransport, PipeTransport, PtyTransport, TcpSocketTransport,
    Transport, TransportError, TransportEvent, TransportRelay, TransportReporter,
    UnixSocketTransport,
};
