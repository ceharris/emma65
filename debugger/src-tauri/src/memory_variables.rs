//! Memory Variables panel: data model, profile-scoped persistence
//! (`memory-variables.json`), and symbol-table binding materialization.
//!
//! Named "Memory Variables" throughout — never shortened to "Variables" —
//! because `WatchpointPanel.tsx` already uses that word for a different
//! concept (walrus-assigned watch-expression runtime variables). See
//! `plan/memory-variables-panel-plan.md`.

use std::path::Path;
use std::sync::Mutex;

use emma65::emulator::{Bus, SymbolSource, SymbolTable};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::CpuState;
use crate::profile::ProfileDirState;
use crate::symbols::format_source;

/// Scalar types a memory variable can be displayed/edited as.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableType {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    Char,
    Bool,
}

impl VariableType {
    /// Storage width in bytes.
    pub fn size_bytes(self) -> usize {
        match self {
            VariableType::I8 | VariableType::U8 | VariableType::Char | VariableType::Bool => 1,
            VariableType::I16 | VariableType::U16 => 2,
            VariableType::I32 | VariableType::U32 => 4,
        }
    }

    /// True for the signed integer types.
    pub fn is_signed(self) -> bool {
        matches!(
            self,
            VariableType::I8 | VariableType::I16 | VariableType::I32
        )
    }
}

/// Display radix for a memory variable's value, string-tagged to match the
/// frontend's `DataRadix` literals (`RadixControl.tsx`) exactly, so no
/// translation layer is needed at the Tauri boundary.
///
/// `UDec`/`SDec` need explicit `rename`s: plain `rename_all = "snake_case"`
/// would serialize them as `"u_dec"`/`"s_dec"` (a word-boundary is inserted
/// before the capitalized `Dec`), not the `"udec"`/`"sdec"` `DataRadix`
/// actually uses.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Radix {
    Hex,
    #[serde(rename = "udec")]
    UDec,
    #[serde(rename = "sdec")]
    SDec,
    Oct,
    Bin,
}

/// One user-defined memory variable, identified by `name`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariableDef {
    pub name: String,
    /// Last address this name was bound to. Used only to re-materialize a
    /// `User`-sourced symbol-table entry on profile load when nothing else
    /// (File/Assembler) defines `name` — see `materialize_bindings`. Live
    /// display always re-resolves through the symbol table, never this
    /// field directly.
    pub address: u16,
    pub data_type: VariableType,
    pub radix: Radix,
}

/// Tauri-managed state for the loaded profile's memory variable definitions.
pub struct MemoryVariablesState(pub Mutex<Vec<VariableDef>>);

/// Loads `dir/memory-variables.json`: a JSON array of `VariableDef`. A
/// missing or unparseable file means no variables, not an error — matching
/// `breakpoints::load_breakpoints_from`.
pub fn load_memory_variables_from(dir: &Path) -> Vec<VariableDef> {
    let path = dir.join("memory-variables.json");
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&contents).unwrap_or_default()
}

/// Saves `defs` to `dir/memory-variables.json`, creating `dir` if needed.
pub fn save_memory_variables_to(dir: &Path, defs: &[VariableDef]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let contents = serde_json::to_string_pretty(defs).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("memory-variables.json"), contents).map_err(|e| e.to_string())
}

/// Re-materializes each loaded definition's binding into `table` as a
/// `SymbolSource::User` entry, but only when `name` doesn't already resolve
/// through some other source — a `File`/`Assembler` entry takes precedence
/// and is left alone, so `address_for` reflects it rather than the
/// possibly-stale `def.address`. Idempotent: once a `User` entry exists for
/// `name`, re-running is a no-op.
pub fn materialize_bindings(defs: &[VariableDef], table: &mut SymbolTable) {
    for def in defs {
        if table.address_for(&def.name).is_none() {
            table.insert_tagged(def.name.clone(), def.address, SymbolSource::User);
        }
    }
}

/// One row of the Memory Variables panel: a definition's current live
/// resolution against the symbol table and memory.
///
/// `address`/`source`/`value` are all `None` together when `name` currently
/// resolves to nothing (e.g. a `File`-sourced label that was materializing it
/// has since been unloaded) — the panel renders that as "undefined" rather
/// than an error.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MemoryVariableRow {
    pub name: String,
    pub data_type: VariableType,
    pub radix: Radix,
    pub address: Option<u16>,
    /// Human-readable source label (`"User"`, `"Assembler"`, `"File: <basename>"`),
    /// formatted via the same helper the Symbols panel uses.
    pub source: Option<String>,
    /// Widened into a single container big enough for every scalar type's
    /// full range; `Char`/`Bool` are read exactly like `U8`, the frontend
    /// applies their fixed display semantics.
    pub value: Option<i64>,
}

/// Reads `data_type.size_bytes()` bytes starting at `addr` via `Bus::peek`
/// (side-effect-free), assembles them little-endian, and sign-extends when
/// `data_type.is_signed()` — the same wrapping-address, little-endian
/// convention `watch::context`'s `FetchWord`/`FetchDWord` and
/// `Cpu::read_mem_u32`/`read_mem_i32` already establish for this codebase.
fn read_value(bus: &Bus, addr: u16, data_type: VariableType) -> i64 {
    let width = data_type.size_bytes() as u16;
    let mut raw: u32 = 0;
    for i in 0..width {
        let byte = bus.peek(addr.wrapping_add(i)).unwrap_or(0) as u32;
        raw |= byte << (i * 8);
    }
    if data_type.is_signed() {
        match width {
            1 => (raw as u8 as i8) as i64,
            2 => (raw as u16 as i16) as i64,
            4 => raw as i32 as i64,
            _ => raw as i64,
        }
    } else {
        raw as i64
    }
}

/// Resolves `def` against the live symbol table and memory into a display row.
fn resolve_row(def: &VariableDef, table: &SymbolTable, bus: &Bus) -> MemoryVariableRow {
    let address = table.address_for(&def.name);
    let source = address.and_then(|_| table.source_for(&def.name).map(|s| format_source(s).0));
    let value = address.map(|addr| read_value(bus, addr, def.data_type));
    MemoryVariableRow {
        name: def.name.clone(),
        data_type: def.data_type,
        radix: def.radix,
        address,
        source,
        value,
    }
}

/// Builds a fresh row list for every definition in `defs`, resolved against `cpu`.
fn resolve_rows(defs: &[VariableDef], cpu: &emma65::emulator::Cpu) -> Vec<MemoryVariableRow> {
    let table = cpu.bus().symbol_table();
    defs.iter()
        .map(|def| resolve_row(def, table, cpu.bus()))
        .collect()
}

/// Core logic for adding a new memory variable, factored out from the
/// `#[tauri::command]` wrapper so it can be exercised directly in tests
/// without a full Tauri `State`/`AppHandle` context.
///
/// If `name` already resolves in `table`, `address` is ignored and the
/// existing binding is reused. Otherwise `address` is required and a new
/// `SymbolSource::User` entry is inserted. Rejects a duplicate `defs` name.
/// Returns whether a new symbol was inserted, so the caller knows whether to
/// also emit `"symbols-changed"`.
fn add_variable(
    defs: &mut Vec<VariableDef>,
    table: &mut SymbolTable,
    name: String,
    address: Option<u16>,
    data_type: VariableType,
    radix: Radix,
) -> Result<bool, String> {
    if defs.iter().any(|d| d.name == name) {
        return Err(format!("A memory variable named \"{name}\" already exists"));
    }
    let (bound_address, inserted_symbol) = match table.address_for(&name) {
        Some(addr) => (addr, false),
        None => {
            let addr = address.ok_or_else(|| {
                format!("\"{name}\" is not a known symbol; an address is required")
            })?;
            table.insert_tagged(name.clone(), addr, SymbolSource::User);
            (addr, true)
        }
    };
    defs.push(VariableDef {
        name,
        address: bound_address,
        data_type,
        radix,
    });
    Ok(inserted_symbol)
}

/// Core logic for editing a memory variable (rename and/or type/radix
/// change), factored out for the same reason as `add_variable`.
///
/// If renaming (`new_name != old_name`) and `old_name` currently resolves via
/// a live `SymbolSource::User` entry — i.e. this panel owns it — that entry
/// is removed once the new binding is established, so a rename never leaks a
/// stale `User` symbol. A `File`/`Assembler`-owned name is left alone: some
/// other source may still be using it. Returns whether the rename touched
/// the symbol table's live names, so the caller knows whether to also emit
/// `"symbols-changed"`.
fn edit_variable(
    defs: &mut [VariableDef],
    table: &mut SymbolTable,
    old_name: &str,
    new_name: String,
    address: Option<u16>,
    data_type: VariableType,
    radix: Radix,
) -> Result<bool, String> {
    let index = defs
        .iter()
        .position(|d| d.name == old_name)
        .ok_or_else(|| format!("No memory variable named \"{old_name}\""))?;
    let renaming = new_name != old_name;
    if renaming && defs.iter().any(|d| d.name == new_name) {
        return Err(format!(
            "A memory variable named \"{new_name}\" already exists"
        ));
    }
    let old_owned_by_panel =
        renaming && matches!(table.source_for(old_name), Some(SymbolSource::User));
    let bound_address = match table.address_for(&new_name) {
        Some(addr) => addr,
        None => {
            let addr = address.ok_or_else(|| {
                format!("\"{new_name}\" is not a known symbol; an address is required")
            })?;
            table.insert_tagged(new_name.clone(), addr, SymbolSource::User);
            addr
        }
    };
    if old_owned_by_panel {
        table.remove_tagged(old_name, &SymbolSource::User);
    }
    defs[index] = VariableDef {
        name: new_name,
        address: bound_address,
        data_type,
        radix,
    };
    Ok(renaming)
}

/// Removes the definition named `name`. Deliberately never touches the
/// symbol table: a bound symbol may still be useful elsewhere (e.g. in a
/// watch expression), so deleting a variable only removes the panel's own
/// display definition for it.
fn remove_variable(defs: &mut Vec<VariableDef>, name: &str) -> Result<(), String> {
    let index = defs
        .iter()
        .position(|d| d.name == name)
        .ok_or_else(|| format!("No memory variable named \"{name}\""))?;
    defs.remove(index);
    Ok(())
}

/// Returns a fresh snapshot of every loaded memory variable, resolved against
/// live symbol-table and memory state. Returns an empty list (not an error)
/// if the CPU isn't ready, since the panel should just render empty in that
/// case rather than show an error state — matching `symbols::get_symbols`.
#[tauri::command]
pub fn get_memory_variables(
    cpu_state: State<CpuState>,
    memory_variables_state: State<MemoryVariablesState>,
) -> Result<Vec<MemoryVariableRow>, String> {
    let defs = memory_variables_state.0.lock().unwrap();
    let cpu_guard = cpu_state.0.lock().unwrap();
    match cpu_guard.as_ref() {
        Some(cpu) => Ok(resolve_rows(&defs, cpu)),
        None => Ok(Vec::new()),
    }
}

/// A memory variable's user-editable fields, grouped into one struct so
/// `add_memory_variable`/`edit_memory_variable` stay under clippy's
/// argument-count lint instead of using `#[allow(clippy::too_many_arguments)]`
/// — the same grouping `emulator::transport::pipe` uses for `run_pipe_task`
/// (`PipeTaskIo`/`PipeTaskChannels`). Shared by both commands: for `add` it's
/// the new variable's fields; for `edit` it's the target's new fields,
/// alongside a separate `old_name` identifying which definition to edit.
#[derive(Deserialize)]
pub struct MemoryVariableFields {
    pub name: String,
    pub address: Option<u16>,
    pub data_type: VariableType,
    pub radix: Radix,
}

/// Adds a new memory variable, persists the updated definition list, and
/// returns a fresh snapshot. See `add_variable` for the binding semantics.
#[tauri::command]
pub fn add_memory_variable(
    fields: MemoryVariableFields,
    cpu_state: State<CpuState>,
    memory_variables_state: State<MemoryVariablesState>,
    profile_dir: State<ProfileDirState>,
    app: AppHandle,
) -> Result<Vec<MemoryVariableRow>, String> {
    let mut defs = memory_variables_state.0.lock().unwrap();
    let mut cpu_guard = cpu_state.0.lock().unwrap();
    let cpu = cpu_guard.as_mut().ok_or("CPU not ready")?;
    let inserted_symbol = add_variable(
        &mut defs,
        cpu.bus_mut().symbol_table_mut(),
        fields.name,
        fields.address,
        fields.data_type,
        fields.radix,
    )?;
    save_memory_variables_to(&profile_dir.0.lock().unwrap().clone(), &defs)?;
    let rows = resolve_rows(&defs, cpu);
    app.emit("memory-variables-changed", ()).ok();
    if inserted_symbol {
        app.emit("symbols-changed", ()).ok();
    }
    Ok(rows)
}

/// Renames and/or retypes a memory variable, persists the updated definition
/// list, and returns a fresh snapshot. See `edit_variable` for the rename
/// cleanup semantics.
#[tauri::command]
pub fn edit_memory_variable(
    old_name: String,
    fields: MemoryVariableFields,
    cpu_state: State<CpuState>,
    memory_variables_state: State<MemoryVariablesState>,
    profile_dir: State<ProfileDirState>,
    app: AppHandle,
) -> Result<Vec<MemoryVariableRow>, String> {
    let mut defs = memory_variables_state.0.lock().unwrap();
    let mut cpu_guard = cpu_state.0.lock().unwrap();
    let cpu = cpu_guard.as_mut().ok_or("CPU not ready")?;
    let renamed = edit_variable(
        &mut defs,
        cpu.bus_mut().symbol_table_mut(),
        &old_name,
        fields.name,
        fields.address,
        fields.data_type,
        fields.radix,
    )?;
    save_memory_variables_to(&profile_dir.0.lock().unwrap().clone(), &defs)?;
    let rows = resolve_rows(&defs, cpu);
    app.emit("memory-variables-changed", ()).ok();
    if renamed {
        app.emit("symbols-changed", ()).ok();
    }
    Ok(rows)
}

/// Removes a memory variable definition (leaving the symbol table untouched),
/// persists the updated definition list, and returns a fresh snapshot.
#[tauri::command]
pub fn remove_memory_variable(
    name: String,
    cpu_state: State<CpuState>,
    memory_variables_state: State<MemoryVariablesState>,
    profile_dir: State<ProfileDirState>,
    app: AppHandle,
) -> Result<Vec<MemoryVariableRow>, String> {
    let mut defs = memory_variables_state.0.lock().unwrap();
    remove_variable(&mut defs, &name)?;
    save_memory_variables_to(&profile_dir.0.lock().unwrap().clone(), &defs)?;
    let cpu_guard = cpu_state.0.lock().unwrap();
    let rows = match cpu_guard.as_ref() {
        Some(cpu) => resolve_rows(&defs, cpu),
        None => Vec::new(),
    };
    app.emit("memory-variables-changed", ()).ok();
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "emma65-memory-variables-test-{name}-{:?}",
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_def() -> VariableDef {
        VariableDef {
            name: "counter".to_string(),
            address: 0x0200,
            data_type: VariableType::U8,
            radix: Radix::Hex,
        }
    }

    /// Regression check: `Radix` must serialize/deserialize using exactly the
    /// frontend's `DataRadix` string literals (`"udec"`/`"sdec"`, not
    /// `rename_all = "snake_case"`'s default `"u_dec"`/`"s_dec"`) — otherwise
    /// a value round-tripped through `edit_memory_variable` from the
    /// frontend's radix-cycle button fails to deserialize.
    #[test]
    fn radix_serializes_using_frontend_data_radix_literals() {
        let cases = [
            (Radix::Hex, "\"hex\""),
            (Radix::UDec, "\"udec\""),
            (Radix::SDec, "\"sdec\""),
            (Radix::Oct, "\"oct\""),
            (Radix::Bin, "\"bin\""),
        ];
        for (radix, expected) in cases {
            let json = serde_json::to_string(&radix).unwrap();
            assert_eq!(json, expected);
            let back: Radix = serde_json::from_str(&json).unwrap();
            assert_eq!(back, radix);
        }
    }

    #[test]
    fn load_returns_empty_when_file_missing() {
        let dir = temp_dir("load-missing");
        assert!(load_memory_variables_from(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_returns_empty_when_file_unparseable() {
        let dir = temp_dir("load-malformed");
        std::fs::write(dir.join("memory-variables.json"), "not json").unwrap();
        assert!(load_memory_variables_from(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = temp_dir("round-trip");
        let defs = vec![
            sample_def(),
            VariableDef {
                name: "score".to_string(),
                address: 0x0300,
                data_type: VariableType::I16,
                radix: Radix::SDec,
            },
        ];
        save_memory_variables_to(&dir, &defs).unwrap();
        let reloaded = load_memory_variables_from(&dir);
        assert_eq!(reloaded.len(), defs.len());
        assert_eq!(reloaded[0].name, "counter");
        assert_eq!(reloaded[0].address, 0x0200);
        assert_eq!(reloaded[0].data_type, VariableType::U8);
        assert_eq!(reloaded[0].radix, Radix::Hex);
        assert_eq!(reloaded[1].name, "score");
        assert_eq!(reloaded[1].data_type, VariableType::I16);
        assert_eq!(reloaded[1].radix, Radix::SDec);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_with_empty_list_round_trips() {
        let dir = temp_dir("empty");
        save_memory_variables_to(&dir, &[]).unwrap();
        assert!(load_memory_variables_from(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn materialize_bindings_inserts_for_unresolved_name() {
        let mut table = SymbolTable::default();
        let defs = vec![sample_def()];
        materialize_bindings(&defs, &mut table);
        assert_eq!(table.address_for("counter"), Some(0x0200));
    }

    #[test]
    fn materialize_bindings_is_noop_for_name_already_defined_by_another_source() {
        let mut table = SymbolTable::default();
        table.insert_tagged("counter".to_string(), 0x9000, SymbolSource::Assembler);
        let defs = vec![sample_def()];
        materialize_bindings(&defs, &mut table);
        assert_eq!(table.address_for("counter"), Some(0x9000));
    }

    #[test]
    fn materialize_bindings_is_idempotent() {
        let mut table = SymbolTable::default();
        let defs = vec![sample_def()];
        materialize_bindings(&defs, &mut table);
        materialize_bindings(&defs, &mut table);
        assert_eq!(table.address_for("counter"), Some(0x0200));
        assert_eq!(table.iter().filter(|(n, _, _)| *n == "counter").count(), 1);
    }

    fn make_bus() -> Bus {
        use emma65::emulator::AddressRange;
        Bus::config()
            .ram_with_fill(AddressRange::new(0x0000, 0xFFFF), 0)
            .unwrap()
            .build()
    }

    #[test]
    fn read_value_reads_unsigned_little_endian() {
        let mut bus = make_bus();
        bus.write(0x0200, 0x34).unwrap();
        bus.write(0x0201, 0x12).unwrap();
        assert_eq!(read_value(&bus, 0x0200, VariableType::U16), 0x1234);
    }

    #[test]
    fn read_value_sign_extends_negative_values() {
        let mut bus = make_bus();
        bus.write(0x0200, 0xFF).unwrap();
        assert_eq!(read_value(&bus, 0x0200, VariableType::I8), -1);

        bus.write(0x0300, 0x00).unwrap();
        bus.write(0x0301, 0x80).unwrap();
        assert_eq!(read_value(&bus, 0x0300, VariableType::I16), i16::MIN as i64);
    }

    #[test]
    fn resolve_row_undefined_when_name_unresolved() {
        let table = SymbolTable::default();
        let bus = make_bus();
        let def = sample_def();
        let row = resolve_row(&def, &table, &bus);
        assert_eq!(row.address, None);
        assert_eq!(row.source, None);
        assert_eq!(row.value, None);
    }

    #[test]
    fn resolve_row_reflects_source_cleared_after_definition_was_resolved() {
        // Regression check: a name that resolved when its `VariableDef` was
        // created can stop resolving later (e.g. its label file was
        // unloaded) — `resolve_row` must reflect the live table, not
        // `def.address`.
        let mut table = SymbolTable::default();
        table.insert_tagged("counter".to_string(), 0x0200, SymbolSource::User);
        let bus = make_bus();
        let def = sample_def();

        let row = resolve_row(&def, &table, &bus);
        assert_eq!(row.address, Some(0x0200));
        assert_eq!(row.source.as_deref(), Some("User"));

        table.clear_source(&SymbolSource::User);
        let row = resolve_row(&def, &table, &bus);
        assert_eq!(row.address, None);
        assert_eq!(row.source, None);
        assert_eq!(row.value, None);
    }

    #[test]
    fn add_variable_reuses_existing_symbol_and_ignores_address() {
        let mut table = SymbolTable::default();
        table.insert_tagged("counter".to_string(), 0x9000, SymbolSource::Assembler);
        let mut defs = Vec::new();
        let inserted = add_variable(
            &mut defs,
            &mut table,
            "counter".to_string(),
            None,
            VariableType::U8,
            Radix::Hex,
        )
        .unwrap();
        assert!(!inserted);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].address, 0x9000);
        assert_eq!(table.address_for("counter"), Some(0x9000));
    }

    #[test]
    fn add_variable_with_new_name_creates_user_symbol() {
        let mut table = SymbolTable::default();
        let mut defs = Vec::new();
        let inserted = add_variable(
            &mut defs,
            &mut table,
            "score".to_string(),
            Some(0x0300),
            VariableType::I16,
            Radix::SDec,
        )
        .unwrap();
        assert!(inserted);
        assert_eq!(table.address_for("score"), Some(0x0300));
        assert_eq!(table.source_for("score"), Some(&SymbolSource::User));
    }

    #[test]
    fn add_variable_requires_address_for_unresolved_new_name() {
        let mut table = SymbolTable::default();
        let mut defs = Vec::new();
        let result = add_variable(
            &mut defs,
            &mut table,
            "score".to_string(),
            None,
            VariableType::I16,
            Radix::SDec,
        );
        assert!(result.is_err());
    }

    #[test]
    fn add_variable_rejects_duplicate_name() {
        let mut table = SymbolTable::default();
        let mut defs = vec![sample_def()];
        let result = add_variable(
            &mut defs,
            &mut table,
            "counter".to_string(),
            Some(0x0400),
            VariableType::U8,
            Radix::Hex,
        );
        assert!(result.is_err());
        assert_eq!(defs.len(), 1);
    }

    #[test]
    fn edit_variable_rename_removes_stale_user_owned_symbol() {
        let mut table = SymbolTable::default();
        table.insert_tagged("counter".to_string(), 0x0200, SymbolSource::User);
        let mut defs = vec![sample_def()];
        let renamed = edit_variable(
            &mut defs,
            &mut table,
            "counter",
            "total".to_string(),
            Some(0x0400),
            VariableType::U8,
            Radix::Hex,
        )
        .unwrap();
        assert!(renamed);
        assert_eq!(table.address_for("counter"), None);
        assert_eq!(table.address_for("total"), Some(0x0400));
        assert_eq!(defs[0].name, "total");
    }

    #[test]
    fn edit_variable_rename_leaves_non_user_owned_symbol_alone() {
        let mut table = SymbolTable::default();
        table.insert_tagged("counter".to_string(), 0x0200, SymbolSource::Assembler);
        let mut defs = vec![sample_def()];
        edit_variable(
            &mut defs,
            &mut table,
            "counter",
            "total".to_string(),
            Some(0x0400),
            VariableType::U8,
            Radix::Hex,
        )
        .unwrap();
        // The Assembler-sourced "counter" symbol is untouched, only this
        // panel's own definition moved to a new name.
        assert_eq!(table.address_for("counter"), Some(0x0200));
        assert_eq!(table.address_for("total"), Some(0x0400));
    }

    #[test]
    fn edit_variable_type_and_radix_only_leaves_symbol_table_untouched() {
        let mut table = SymbolTable::default();
        table.insert_tagged("counter".to_string(), 0x0200, SymbolSource::User);
        let mut defs = vec![sample_def()];
        let renamed = edit_variable(
            &mut defs,
            &mut table,
            "counter",
            "counter".to_string(),
            None,
            VariableType::I16,
            Radix::SDec,
        )
        .unwrap();
        assert!(!renamed);
        assert_eq!(defs[0].data_type, VariableType::I16);
        assert_eq!(defs[0].radix, Radix::SDec);
        assert_eq!(table.iter().filter(|(n, _, _)| *n == "counter").count(), 1);
    }

    #[test]
    fn edit_variable_rejects_rename_to_existing_variable_name() {
        let mut table = SymbolTable::default();
        table.insert_tagged("counter".to_string(), 0x0200, SymbolSource::User);
        table.insert_tagged("score".to_string(), 0x0300, SymbolSource::User);
        let mut defs = vec![
            sample_def(),
            VariableDef {
                name: "score".to_string(),
                address: 0x0300,
                data_type: VariableType::I16,
                radix: Radix::SDec,
            },
        ];
        let result = edit_variable(
            &mut defs,
            &mut table,
            "counter",
            "score".to_string(),
            None,
            VariableType::U8,
            Radix::Hex,
        );
        assert!(result.is_err());
    }

    #[test]
    fn remove_variable_removes_definition_and_leaves_symbol_table_untouched() {
        let mut table = SymbolTable::default();
        table.insert_tagged("counter".to_string(), 0x0200, SymbolSource::User);
        let mut defs = vec![sample_def()];
        remove_variable(&mut defs, "counter").unwrap();
        assert!(defs.is_empty());
        assert_eq!(table.address_for("counter"), Some(0x0200));
    }

    #[test]
    fn remove_variable_errors_for_unknown_name() {
        let mut defs = vec![sample_def()];
        let result = remove_variable(&mut defs, "nonexistent");
        assert!(result.is_err());
        assert_eq!(defs.len(), 1);
    }
}
