//! Register panel: register snapshot/edit commands and changed-flag tracking.

use std::collections::BTreeMap;
use std::sync::Mutex;

use emma65::emulator::cpu::Registers;
use tauri::{AppHandle, Emitter, State};

use crate::CpuState;
use crate::cpu;
use crate::cpu_bus::{CpuBusCache, snapshot_cpu_bus};
use crate::disassembly::LiveSnapshotRx;

/// Bitmask of flags-register bits that changed on the most recent step.
///
/// Reset to 0 on session start; updated by `step_into` and read by `get_registers`.
pub struct ChangedFlagsState(pub Mutex<u32>);

/// Register snapshot returned to the frontend.
#[derive(Clone, serde::Serialize)]
pub struct RegisterSnapshot {
    /// Every register in the CPU description, keyed by its name.
    pub values: BTreeMap<&'static str, u32>,
    /// The program counter, also in `values`, for the many consumers that want only it.
    pub pc: u16,
    /// Bitmask of flags-register bits that changed on the most recent step (0 on initial load).
    pub changed_flags: u32,
    /// True when the CPU executed STP and is now halted; auto-step should stop.
    pub cpu_stopped: bool,
    /// True when the CPU executed WAI and is waiting for an interrupt.
    pub cpu_waiting: bool,
    /// True when the post-step PC matches a breakpoint address; auto-step should stop.
    pub breakpoint_hit: bool,
}

impl RegisterSnapshot {
    /// A snapshot of `regs`, with `changed_flags` and the halt flags as given.
    pub fn new(
        regs: &Registers,
        changed_flags: u32,
        cpu_stopped: bool,
        cpu_waiting: bool,
        breakpoint_hit: bool,
    ) -> Self {
        Self {
            values: cpu::register_values(regs),
            pc: regs.pc,
            changed_flags,
            cpu_stopped,
            cpu_waiting,
            breakpoint_hit,
        }
    }
}

/// Sets the register `name` (as the CPU description names it) to `value`, which must
/// fit the register's width.
///
/// Only callable while the CPU is stopped (not free-running). Emits
/// `debugger-halted` with the (possibly unchanged) PC so the disassembly view
/// re-centers and the stack view refreshes, covering PC/S edits.
#[tauri::command]
pub fn set_register(
    app: AppHandle,
    name: String,
    value: u32,
    cpu_state: State<CpuState>,
    changed_flags_state: State<ChangedFlagsState>,
    cpu_bus_cache: State<CpuBusCache>,
) -> Result<RegisterSnapshot, String> {
    let mut guard = cpu_state.0.lock().unwrap();
    let cpu = guard.as_mut().ok_or("CPU not ready")?;

    let flags_before = cpu::flags_value(cpu.registers());
    cpu::set_register_value(cpu.registers_mut(), &name, value)?;

    let regs = *cpu.registers();
    let changed = flags_before ^ cpu::flags_value(&regs);
    *changed_flags_state.0.lock().unwrap() = changed;
    *cpu_bus_cache.0.lock().unwrap() = snapshot_cpu_bus(cpu);

    let snapshot = RegisterSnapshot::new(&regs, changed, cpu.is_stopped(), cpu.is_waiting(), false);

    let _ = app.emit("debugger-halted", regs.pc);
    Ok(snapshot)
}

/// Returns a register snapshot of the current CPU state without stepping.
///
/// Falls back to the live snapshot channel when the CPU is free-running
/// (i.e. `CpuState` is `None`). `changed_flags` is 0 during free-run.
#[tauri::command]
pub fn get_registers(
    cpu_state: State<CpuState>,
    changed_flags_state: State<ChangedFlagsState>,
    live_snapshot_rx: State<LiveSnapshotRx>,
) -> Result<RegisterSnapshot, String> {
    let guard = cpu_state.0.lock().unwrap();
    if let Some(cpu) = guard.as_ref() {
        let changed_flags = *changed_flags_state.0.lock().unwrap();
        return Ok(RegisterSnapshot::new(
            cpu.registers(),
            changed_flags,
            cpu.is_stopped(),
            cpu.is_waiting(),
            false,
        ));
    }
    // CPU is free-running — read from the live snapshot channel.
    let live = live_snapshot_rx
        .0
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|rx| rx.borrow().clone())
        .ok_or("CPU not ready")?;
    Ok(RegisterSnapshot::new(
        &live.registers,
        0,
        false,
        false,
        false,
    ))
}
