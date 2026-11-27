//! `emma6k`: the emulator command line for every supported CPU.
//!
//! The configuration chooses the CPU family once, at startup (see
//! [`config`]). [`run_session`] then runs that family's CPU; it is generic
//! over the `Processor`, so each family gets its own compiled copy and the
//! hot path never goes through dynamic dispatch.

mod config;
mod family;
mod tty;

use std::path::Path;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use crate::config::{AppConfig, RunPlan};
use crate::family::Family;
use emma6k_core::exec::StepResult;
use emma6k_core::session::EmulatorSession;
use emma6k_core::{
    BuildError, DeviceRegistry, InstantiationContext, InternalPipeTransport, LogSender, Processor,
    Transport, TransportReporter, log_device_event,
};

/// Finishes a trace started by a family's `start_trace`: waits for the
/// writer to flush.
type TraceFinish = Box<dyn FnOnce() + Send>;

#[tokio::main]
async fn main() -> ExitCode {
    // The console, when attached to this process's own stdio, puts the terminal into raw mode
    // (see tty::enter_raw_mode), which disables OPOST — so a bare '\n' no longer returns the
    // cursor to column 0. env_logger's default suffix is just '\n'; force '\r\n' so log lines
    // (e.g. TRANSPORT connect/disconnect events, which arrive after raw mode is entered) still
    // render as proper new lines. Harmless when raw mode isn't in effect: the terminal treats a
    // leading '\r' before '\n' as a no-op cursor return.
    let mut log_format = env_logger::fmt::ConfigurableFormat::default();
    log_format.suffix("\r\n");
    env_logger::Builder::from_default_env()
        .format(move |buf, record| log_format.format(buf, record))
        .init();

    let plan = AppConfig::load()
        .and_then(|loaded| loaded.resolve())
        .unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1);
        });
    match plan.family {
        Family::Mos6502 => run_6502(plan).await,
        Family::Mc6809 => run_6809(plan).await,
    }
}

/// Builds and runs a 6502 session.
async fn run_6502(plan: RunPlan) -> ExitCode {
    use emma65::emulator::{Config, CpuVariantSpec};
    let app = &plan.app;
    let config = Config {
        cpu_variant_spec: Some(parse_variant::<CpuVariantSpec>(app)),
        clock_speed_hz: app.clock_speed_hz,
        devices: app.devices.clone(),
        unmapped_policy: app.unmapped_policy.clone(),
    };
    let registry = DeviceRegistry::with_builtins();
    run_session(
        app,
        async |context| config.build_with_context(&registry, context).await,
        start_6502_trace,
    )
    .await
}

/// Builds and runs a 6809 session.
async fn run_6809(plan: RunPlan) -> ExitCode {
    use emma09::{Config, CpuVariantSpec};
    let app = &plan.app;
    let config = Config {
        cpu_variant_spec: Some(parse_variant::<CpuVariantSpec>(app)),
        clock_speed_hz: app.clock_speed_hz,
        devices: app.devices.clone(),
        unmapped_policy: app.unmapped_policy.clone(),
    };
    let registry = emma09::device_registry();
    run_session(
        app,
        async |context| config.build_with_context(&registry, context).await,
        |_, _| Err("--trace-file doesn't support the 6809 yet".to_string()),
    )
    .await
}

/// Parses the chosen variant with the family's own parser, exiting with its
/// message (for example "the 6309 isn't supported yet") if it refuses.
fn parse_variant<V: std::str::FromStr<Err = String>>(app: &AppConfig) -> V {
    let variant = app.cpu_variant.as_deref().unwrap_or_default();
    variant.parse().unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    })
}

/// Starts a binary execution trace of the 6502 into `path`.
fn start_6502_trace(cpu: &mut emma65::emulator::Cpu, path: &Path) -> Result<TraceFinish, String> {
    let file = std::fs::File::create(path)
        .map_err(|e| format!("failed to open trace file {}: {e}", path.display()))?;
    let writer = emma65::emulator::BinaryTraceWriter::new(file, cpu.variant());
    let (callback, handle, _dropped) = emma65::emulator::spawn_trace_writer(
        writer,
        4096,
        emma65::emulator::OverflowPolicy::DropOnFull,
    );
    cpu.set_trace_callback(Some(Box::new(callback)));
    Ok(Box::new(move || {
        let _ = handle.join();
    }))
}

/// Runs one session to completion: builds the machine with `build`, wires
/// the console to this process's stdio, starts logging and (if configured)
/// tracing with `start_trace`, and runs the CPU until it halts, fails, or
/// Ctrl+C. Compiled once per CPU.
async fn run_session<P: Processor>(
    app: &AppConfig,
    build: impl AsyncFnOnce(InstantiationContext) -> Result<EmulatorSession<P>, BuildError>,
    start_trace: impl FnOnce(&mut P, &Path) -> Result<TraceFinish, String>,
) -> ExitCode {
    // Always offer stdin/stdout to the console via the context. If the console has no
    // `transport=` attribute it will take this transport; if it does have one it will ignore it.
    // Checking whether the slot was consumed after build tells us whether to enter raw mode.
    //
    // No `DeviceId` exists yet at this point (the console's isn't allocated until
    // `ConsoleModule::instantiate` runs deep inside `build` below), so the reporter starts
    // unbound; `ConsoleModule::instantiate` binds it once the id is known.
    let reporter = TransportReporter::pending(None);
    let (transport, relay) = InternalPipeTransport::stdio(reporter.clone()).unwrap_or_else(|e| {
        eprintln!("error: failed to attach console to stdin/stdout: {e}");
        std::process::exit(1);
    });
    let console_transport_slot = Arc::new(Mutex::new(Some((
        Box::new(transport) as Box<dyn Transport>,
        relay,
        reporter,
    ))));

    // Always one concrete `LogSender`, cloned into `context`, the CPU, and the event-logging
    // loop below, so every clone shares the same underlying cycle-count `Arc` (see
    // `LogSender::set_cycles`) — without a single shared instance, each of those independently
    // defaulted senders would carry its own always-zero counter, even on the `log`-crate
    // fallback path used when no `--log-file` is configured.
    let (log_sender, log_writer_handle) = match app.log_file.as_ref() {
        Some(path) => {
            let file = std::fs::File::create(path).unwrap_or_else(|e| {
                eprintln!("error: failed to open log file {}: {e}", path.display());
                std::process::exit(1);
            });
            let (sender, handle) = emma6k_core::spawn_log_writer(file, 4096);
            (sender, Some(handle))
        }
        None => (LogSender::default(), None),
    };

    let context = InstantiationContext {
        clock_hz: app.clock_speed_hz,
        console_transport: Some(Arc::clone(&console_transport_slot)),
        log_sender: Some(log_sender.clone()),
        ..Default::default()
    };
    let session = match build(context).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("startup error: {e}");
            std::process::exit(1);
        }
    };

    let (mut cpu, mut error_receiver) = (session.cpu, session.error_receiver);
    let log_sender_for_events = log_sender.clone();
    cpu.machine_mut().set_log_sender(log_sender);
    if let Err(e) = cpu.reset() {
        eprintln!("reset error: {e}");
        std::process::exit(1);
    }

    let finish_trace = app.trace_file.as_ref().map(|path| {
        start_trace(&mut cpu, path).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1);
        })
    });

    // Enter raw mode only if the console took the stdio transport — and only after startup has
    // fully succeeded, so no error exit above ever needs to restore the terminal first.
    let stdio_in_use = console_transport_slot
        .lock()
        .is_ok_and(|slot| slot.is_none());
    let _raw_mode_guard = if stdio_in_use {
        tty::enter_raw_mode(app.keep_isig)
    } else {
        None
    };

    let run_handle = emma6k_core::run_from(
        cpu,
        None,
        Arc::new(std::sync::atomic::AtomicU16::new(0)),
        Vec::new(),
        app.park_on_halt,
    );
    let (cpu_done_tx, mut cpu_done_rx) = tokio::sync::oneshot::channel::<StepResult<P::Op>>();
    tokio::spawn(async move {
        let _ = cpu_done_tx.send(run_handle.wait().await);
    });

    let mut events_open = true;
    let mut exit_code = ExitCode::SUCCESS;
    loop {
        tokio::select! {
            event = error_receiver.recv(), if events_open => {
                match &event {
                    Some(event) => log_device_event(&log_sender_for_events, event),
                    None => events_open = false,      // all senders dropped
                }
            },

            result = &mut cpu_done_rx => {
                if let StepResult::Error(e) = result.unwrap_or(StepResult::Stopped) {
                    eprintln!("CPU error: {e}");
                    exit_code = ExitCode::FAILURE;
                }
                break;
            },
            _ = tokio::signal::ctrl_c() => break,
        }
    }

    if let Some(finish) = finish_trace {
        finish();
    }
    if let Some(handle) = log_writer_handle {
        let _ = handle.join();
    }

    print!("\r\n"); // canonical newline to delineate emulator output from user's shell prompt

    // Falling off the end here (rather than calling std::process::exit) lets `_raw_mode_guard`
    // drop normally, restoring the terminal before the process actually exits.
    exit_code
}
