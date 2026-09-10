//! Memory Variables panel: data model, profile-scoped persistence
//! (`memory-variables.json`), and symbol-table binding materialization.
//!
//! Named "Memory Variables" throughout — never shortened to "Variables" —
//! because `WatchpointPanel.tsx` already uses that word for a different
//! concept (walrus-assigned watch-expression runtime variables). See
//! `plan/memory-variables-panel-plan.md`.

use std::path::Path;
use std::sync::Mutex;

use emma65::emulator::{SymbolSource, SymbolTable};
use serde::{Deserialize, Serialize};

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
    #[allow(dead_code)] // used by Unit 2's resolve_row
    pub fn size_bytes(self) -> usize {
        match self {
            VariableType::I8 | VariableType::U8 | VariableType::Char | VariableType::Bool => 1,
            VariableType::I16 | VariableType::U16 => 2,
            VariableType::I32 | VariableType::U32 => 4,
        }
    }

    /// True for the signed integer types.
    #[allow(dead_code)] // used by Unit 2's resolve_row
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
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Radix {
    Hex,
    UDec,
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
#[allow(dead_code)] // used by Unit 2's add/edit/remove commands
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
}
