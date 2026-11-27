//! `emma6k`'s configuration: the command line, merging its sources (a
//! `--config` TOML file < `EMMA6K_*` environment variables < command-line
//! flags), choosing the CPU, and applying a starter-profile template.
//!
//! **Choosing the CPU.** `cpu-variant` (or its synonym `cpu`) names a
//! variant, and the variant implies the family. Each source may use either
//! spelling but not both; across sources the usual precedence applies. A
//! `--profile` template's own `cpu-variant` counts as the lowest-precedence
//! source. With no variant from any source, the family comes from the
//! `emma65`/`emma09` wrapper's hidden `--family` flag, and bare `emma6k`
//! stops with an error: it has no default CPU.

use crate::family::{Family, find_template};
use clap::Parser;
use emma6k_core::config::template::Template;
use emma6k_core::config::{DeviceSpec, ExpandedPathBuf, UnmappedPolicySpec};
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Deserializer, Serialize};

/// The prefix of environment variables that set configuration keys.
const ENV_PREFIX: &str = "EMMA6K_";

/// The error when nothing chooses a CPU.
pub const NO_CPU: &str = "no CPU selected: set cpu-variant (or cpu) in the configuration, \
     pass --cpu, or run emma65 or emma09";

// CLI args.
// This struct exists to capture the options that aren't configuration keys
// (`--config`, `--profile`, `--family`) before Figment runs. It must not
// derive Serde's Serialize or Deserialize.
#[derive(Parser)]
#[clap(
    name = "emma6k",
    version = env!("BUILD_VERSION"),
    disable_version_flag = true
)]
struct CliArgs {
    /// Print version information and exit
    #[clap(short = 'v', long = "version", action = clap::ArgAction::Version)]
    version: Option<bool>,

    /// Path to a TOML configuration file
    #[clap(long = "config", conflicts_with = "profile")]
    config: Option<std::path::PathBuf>,

    /// Name of a bundled starter-profile template to run, materialized into a
    /// temporary directory for this run. The template chooses its CPU.
    /// Mutually exclusive with `--config`.
    #[clap(long = "profile", conflicts_with = "config")]
    profile: Option<String>,

    /// The CPU family a wrapper (emma65, emma09) runs: supplies the family's
    /// default variant and rejects another family's.
    #[clap(long = "family", hide = true)]
    family: Option<Family>,

    #[clap(flatten)]
    app: AppConfig,
}

#[derive(Debug, Clone, Default, Parser, Serialize, Deserialize)]
#[clap(name = "emma6k")]
#[serde(rename_all = "kebab-case")]
/// Configuration attributes for the emulator command line.
pub struct AppConfig {
    /// CPU variant: 65c02 or wdc65c02 (6502 family), or 6809. `--cpu` is a
    /// synonym.
    #[clap(long = "cpu-variant", visible_alias = "cpu")]
    #[serde(
        default,
        deserialize_with = "variant_value",
        skip_serializing_if = "Option::is_none"
    )]
    pub cpu_variant: Option<String>,

    /// The `cpu` spelling of `cpu-variant` in TOML and the environment. The
    /// command line's `--cpu` fills `cpu_variant` directly.
    #[clap(skip)]
    #[serde(
        default,
        deserialize_with = "variant_value",
        skip_serializing_if = "Option::is_none"
    )]
    pub cpu: Option<String>,

    /// Clock speed to simulate via throttling.
    #[clap(long = "clock-speed-hz")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_speed_hz: Option<u64>,

    /// Device config specifications.
    #[clap(long = "device", num_args = 1..)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devices: Option<Vec<DeviceSpec>>,

    /// Policy for accesses to unmapped addresses: "ignore" (read as a default
    /// value, discard writes) or "error" (fail the access). Defaults to
    /// "ignore".
    #[clap(long = "unmapped-policy")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unmapped_policy: Option<UnmappedPolicySpec>,

    /// Path to write a binary CPU execution trace to (decode it with emma6k-tracer).
    #[clap(long = "trace-file")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_file: Option<ExpandedPathBuf>,

    /// Path to write structured device/CPU log messages to.
    #[clap(long = "log-file")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_file: Option<ExpandedPathBuf>,

    /// Keep running when the CPU halts (6502 STP or WAI, 6809 CWAI or SYNC)
    /// with nothing yet to wake it, instead of exiting. While parked, a
    /// device-triggered interrupt or reset (e.g. a watchdog device's
    /// `IoDevice::take_reset`) can still wake it. By default the emulator
    /// exits as soon as the CPU halts this way, since a headless run has
    /// nothing left to do at that point.
    #[clap(long = "park-on-halt")]
    #[serde(default)]
    pub park_on_halt: bool,

    /// Keep the controlling terminal's signal-generation behavior (`ISIG`) enabled while the
    /// console has it in raw mode, so Ctrl+C still exits the emulator instead of being delivered
    /// to the emulated console as input. By default the console gets Ctrl+C as raw byte `0x03`,
    /// like a real serial console; with this set, quitting via Ctrl+C works but the console never
    /// sees that byte.
    #[clap(long = "keep-isig")]
    #[serde(default)]
    pub keep_isig: bool,
}

/// The merged configuration, before the CPU is chosen.
pub struct LoadedConfig {
    /// The merged configuration from every source.
    pub app: AppConfig,
    /// The CPU variant, after per-source checks and precedence.
    pub variant: Option<String>,
    /// The raw `--profile <id>` value, if given.
    pub profile: Option<String>,
    /// The wrapper's family, if run through one.
    pub family: Option<Family>,
}

/// What to run: the chosen family and the configuration to build it from.
pub struct RunPlan {
    /// The CPU family to build.
    pub family: Family,
    /// The configuration, with `cpu_variant` set to the variant to run and a
    /// template's devices merged in.
    pub app: AppConfig,
    /// The materialized template's directory, if any: keep it alive until
    /// the machine is built.
    #[allow(dead_code, reason = "held so the template's files outlive the build")]
    pub template_dir: Option<tempfile::TempDir>,
}

/// The `cpu` and `cpu-variant` keys of one source.
#[derive(Default, Deserialize)]
struct CpuKeys {
    #[serde(default, deserialize_with = "variant_value")]
    cpu: Option<String>,
    #[serde(rename = "cpu-variant", default, deserialize_with = "variant_value")]
    cpu_variant: Option<String>,
}

/// Reads a CPU variant as text. Accepts a number too, since `6809` in an
/// environment variable or unquoted in TOML arrives as one.
fn variant_value<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Variant {
        Text(String),
        Number(u64),
    }
    Ok(
        Option::<Variant>::deserialize(deserializer)?.map(|v| match v {
            Variant::Text(s) => s,
            Variant::Number(n) => n.to_string(),
        }),
    )
}

/// Returns the CPU variant one source sets, or an error if it sets both
/// spellings.
fn variant_in(source: Figment, name: &str) -> Result<Option<String>, String> {
    let keys: CpuKeys = source.extract().map_err(|e| format!("{name}: {e}"))?;
    match keys {
        CpuKeys {
            cpu: Some(_),
            cpu_variant: Some(_),
        } => Err(format!(
            "{name} sets both cpu and cpu-variant; set only one"
        )),
        CpuKeys { cpu, cpu_variant } => Ok(cpu.or(cpu_variant)),
    }
}

fn env_source() -> Env {
    Env::prefixed(ENV_PREFIX).map(|k| k.as_str().replace('_', "-").into())
}

impl AppConfig {
    /// Parses the command line and merges every configuration source.
    pub fn load() -> Result<LoadedConfig, String> {
        Self::load_from(CliArgs::parse())
    }

    fn load_from(cli: CliArgs) -> Result<LoadedConfig, String> {
        let mut figment = Figment::new();
        let mut variant = None;
        if let Some(path) = &cli.config {
            variant = variant_in(Figment::from(Toml::file(path)), &path.display().to_string())?;
            figment = figment.merge(Toml::file(path));
        }
        variant = variant_in(Figment::from(env_source()), "the environment")?.or(variant);
        variant = cli.app.cpu_variant.clone().or(variant);

        let app = figment
            .merge(env_source())
            .merge(Serialized::globals(&cli.app))
            .extract()
            .map_err(|e| e.to_string())?;
        Ok(LoadedConfig {
            app,
            variant,
            profile: cli.profile,
            family: cli.family,
        })
    }
}

impl LoadedConfig {
    /// Chooses the family and variant, and applies the `--profile` template,
    /// or the family's default template when no devices are configured.
    pub fn resolve(self) -> Result<RunPlan, String> {
        let LoadedConfig {
            mut app,
            variant,
            profile,
            family: wrapper,
        } = self;

        let profile = profile.map(|id| find_template(&id)).transpose()?;
        let named_family = variant.as_deref().map(Family::of).transpose()?;
        let family = named_family
            .or(profile.map(|(family, _)| family))
            .or(wrapper)
            .ok_or_else(|| NO_CPU.to_string())?;

        // Where the family came from, for error messages.
        let chosen_by = match (&variant, profile) {
            (Some(v), _) => format!("CPU variant '{v}' is a {family}"),
            (None, Some((_, t))) => format!("profile '{}' is for the {family}", t.id),
            (None, None) => String::new(),
        };
        if let Some(wrapper) = wrapper
            && wrapper != family
        {
            return Err(format!(
                "{} runs only {wrapper}-family CPUs, but {chosen_by}; use {} or emma6k",
                wrapper.wrapper(),
                family.wrapper()
            ));
        }
        if let (Some((profile_family, t)), Some(v)) = (profile, &variant)
            && profile_family != family
        {
            return Err(format!(
                "profile '{}' is for the {profile_family}, but CPU variant '{v}' is a {family}",
                t.id
            ));
        }

        let no_devices = app.devices.as_ref().is_none_or(|d| d.is_empty());
        let template = match profile {
            Some((_, t)) => Some(t),
            None if no_devices => match family.default_template() {
                Some(id) => Some(find_template(id)?.1),
                None => {
                    return Err(format!(
                        "the {family} has no default profile yet: pass --config (or --device) \
                         with a machine layout"
                    ));
                }
            },
            None => None,
        };

        let mut template_variant = None;
        let template_dir = template
            .map(|t| apply_template(&mut app, t, &mut template_variant))
            .transpose()?;

        app.cpu_variant = Some(
            variant
                .or(template_variant)
                .unwrap_or_else(|| family.default_variant().to_string()),
        );
        app.cpu = None;

        Ok(RunPlan {
            family,
            app,
            template_dir,
        })
    }
}

/// Materializes `template` into a fresh tempdir, loads it through the same
/// `Figment`/`Toml::file()` path used for `--config`, and merges its devices
/// into `config`, keeping any `clock_speed_hz`/`unmapped_policy` already set
/// from the command line or environment. Sets `variant` to the template's
/// CPU variant. Returns the tempdir handle, which must be kept alive until
/// the machine is built.
fn apply_template(
    config: &mut AppConfig,
    template: &Template,
    variant: &mut Option<String>,
) -> Result<tempfile::TempDir, String> {
    let id = template.id;
    let dir = tempfile::tempdir()
        .map_err(|e| format!("failed to create tempdir for profile '{id}': {e}"))?;
    let toml_path = template
        .materialize(dir.path())
        .map_err(|e| format!("failed to materialize profile '{id}': {e}"))?;
    let parsed: AppConfig = Figment::new()
        .merge(Toml::file(&toml_path))
        .extract()
        .map_err(|e| format!("bundled profile '{id}' failed to parse: {e}"))?;
    *variant = parsed.cpu.or(parsed.cpu_variant);
    config.clock_speed_hz = config.clock_speed_hz.or(parsed.clock_speed_hz);
    config.unmapped_policy = config.unmapped_policy.take().or(parsed.unmapped_policy);
    config.devices = parsed.devices;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family::FAMILIES;

    fn loaded(args: &[&str]) -> Result<LoadedConfig, String> {
        let cli = CliArgs::try_parse_from(std::iter::once("emma6k").chain(args.iter().copied()))
            .map_err(|e| e.to_string())?;
        AppConfig::load_from(cli)
    }

    fn resolve(args: &[&str]) -> Result<RunPlan, String> {
        loaded(args)?.resolve()
    }

    #[test]
    fn no_cpu_is_an_error() {
        assert_eq!(resolve(&[]).err().unwrap(), NO_CPU);
        assert_eq!(
            resolve(&["--device", "ram@0x0000,size=256"]).err().unwrap(),
            NO_CPU
        );
    }

    #[test]
    fn the_wrapper_family_supplies_the_default_variant() {
        let plan = resolve(&["--family", "6502", "--device", "ram@0x0000,size=256"]).unwrap();
        assert_eq!(plan.family, Family::Mos6502);
        assert_eq!(plan.app.cpu_variant.as_deref(), Some("65c02"));
        assert!(plan.template_dir.is_none());
    }

    #[test]
    fn the_6502_default_template_applies_with_no_devices() {
        let plan = resolve(&["--family", "6502"]).unwrap();
        assert_eq!(plan.app.cpu_variant.as_deref(), Some("WDC65C02"));
        assert!(plan.app.devices.is_some_and(|d| !d.is_empty()));
        assert!(plan.template_dir.is_some());
    }

    #[test]
    fn an_explicit_variant_beats_the_template_within_a_family() {
        let plan = resolve(&["--family", "6502", "--cpu", "65c02"]).unwrap();
        assert_eq!(plan.app.cpu_variant.as_deref(), Some("65c02"));
    }

    #[test]
    fn the_6809_default_template_is_camelforth() {
        for args in [&["--family", "6809"][..], &["--cpu", "6809"][..]] {
            let plan = resolve(args).unwrap();
            assert_eq!(plan.family, Family::Mc6809, "{args:?}");
            assert_eq!(plan.app.cpu_variant.as_deref(), Some("6809"), "{args:?}");
            assert!(plan.app.devices.is_some_and(|d| !d.is_empty()), "{args:?}");
        }
    }

    #[test]
    fn camelforth_chooses_the_6809_and_emma65_refuses_it() {
        assert_eq!(
            resolve(&["--profile", "camelforth"]).unwrap().family,
            Family::Mc6809
        );
        let err = resolve(&["--family", "6502", "--profile", "camelforth"])
            .err()
            .unwrap();
        assert!(
            err.contains("profile 'camelforth' is for the 6809"),
            "{err}"
        );
    }

    #[test]
    fn cpu_and_cpu_variant_are_synonyms_on_the_command_line() {
        for flag in ["--cpu", "--cpu-variant"] {
            let plan = resolve(&[flag, "6809", "--device", "ram@0x0000,size=256"]).unwrap();
            assert_eq!(plan.family, Family::Mc6809, "{flag}");
        }
        assert!(loaded(&["--cpu", "6809", "--cpu-variant", "6809"]).is_err());
    }

    #[test]
    fn a_wrapper_rejects_another_familys_variant() {
        let err = resolve(&["--family", "6502", "--cpu", "6809"])
            .err()
            .unwrap();
        assert!(
            err.contains("emma65 runs only 6502-family CPUs")
                && err.contains("CPU variant '6809' is a 6809")
                && err.contains("use emma09"),
            "{err}"
        );
        let err = resolve(&["--family", "6809", "--cpu", "wdc65c02"])
            .err()
            .unwrap();
        assert!(err.contains("emma09 runs only 6809-family CPUs"), "{err}");
    }

    #[test]
    fn a_wrapper_rejects_another_familys_profile() {
        let err = resolve(&["--family", "6809", "--profile", "snake"])
            .err()
            .unwrap();
        assert!(
            err.contains("emma09 runs only 6809-family CPUs")
                && err.contains("profile 'snake' is for the 6502"),
            "{err}"
        );
    }

    #[test]
    fn a_profile_chooses_its_family() {
        let plan = resolve(&["--profile", "snake"]).unwrap();
        assert_eq!(plan.family, Family::Mos6502);
        assert_eq!(plan.app.cpu_variant.as_deref(), Some("WDC65C02"));
    }

    #[test]
    fn a_profile_conflicts_with_another_familys_variant() {
        let err = resolve(&["--profile", "snake", "--cpu", "6809"])
            .err()
            .unwrap();
        assert!(
            err.contains("profile 'snake' is for the 6502, but CPU variant '6809' is a 6809"),
            "{err}"
        );
    }

    #[test]
    fn an_unknown_profile_lists_every_family_s_templates() {
        let err = resolve(&["--profile", "nope"]).err().unwrap();
        assert!(err.contains("'nope'") && err.contains("taliforth"), "{err}");
    }

    #[test]
    fn an_invalid_variant_names_itself() {
        let err = resolve(&["--cpu", "z80"]).err().unwrap();
        assert!(err.contains("'z80'"), "{err}");
    }

    #[test]
    fn trace_file_works_with_the_6809() {
        let plan = resolve(&[
            "--cpu",
            "6809",
            "--device",
            "ram@0x0000,size=256",
            "--trace-file",
            "/tmp/x",
        ])
        .unwrap();
        assert_eq!(plan.family, Family::Mc6809);
        assert!(plan.app.trace_file.is_some());
    }

    #[test]
    fn a_toml_source_setting_both_spellings_is_an_error() {
        let toml = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        std::fs::write(toml.path(), "cpu = \"6809\"\ncpu-variant = \"6809\"\n").unwrap();
        let err = loaded(&["--config", toml.path().to_str().unwrap()])
            .err()
            .unwrap();
        assert!(err.contains("sets both cpu and cpu-variant"), "{err}");
    }

    #[test]
    fn different_spellings_in_different_sources_follow_precedence() {
        // TOML says `cpu = "6809"`; the command line's --cpu-variant wins.
        let toml = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        std::fs::write(toml.path(), "cpu = \"6809\"\n").unwrap();
        let plan = resolve(&[
            "--config",
            toml.path().to_str().unwrap(),
            "--cpu-variant",
            "65c02",
            "--device",
            "ram@0x0000,size=256",
        ])
        .unwrap();
        assert_eq!(plan.family, Family::Mos6502);
        assert_eq!(plan.app.cpu_variant.as_deref(), Some("65c02"));
    }

    #[test]
    fn every_template_names_a_variant_in_its_family() {
        for family in FAMILIES {
            for t in family.templates() {
                let mut variant = None;
                let mut app = AppConfig::default();
                let _dir = apply_template(&mut app, t, &mut variant).unwrap();
                let variant = variant.unwrap_or_else(|| panic!("'{}' sets no cpu-variant", t.id));
                assert_eq!(Family::of(&variant), Ok(family), "template '{}'", t.id);
            }
        }
    }

    #[test]
    fn an_unquoted_numeric_variant_reads_as_text() {
        let toml = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        std::fs::write(toml.path(), "cpu = 6809\n").unwrap();
        let loaded = loaded(&["--config", toml.path().to_str().unwrap()]).unwrap();
        assert_eq!(loaded.variant.as_deref(), Some("6809"));
    }

    #[test]
    fn cli_args_reject_config_and_profile_together() {
        assert!(loaded(&["--config", "/tmp/x.toml", "--profile", "taliforth"]).is_err());
    }
}
