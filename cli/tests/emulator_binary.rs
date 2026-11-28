use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tempfile::NamedTempFile;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The `emma65` wrapper, which runs `emma6k` for the 6502 family.
fn emulator_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_emma65"))
}

fn emma6k_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_emma6k"))
}

fn emma09_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_emma09"))
}

/// 32 KB ROM image: every byte is STP (0xDB), with the reset vector at
/// 0x7FFC/0x7FFD pointing to 0x8000 (the start of the ROM on the bus).
/// Must be used with --cpu-variant WDC65C02, as STP is a WDC-only opcode.
fn make_stp_rom() -> Vec<u8> {
    let mut rom = vec![0xDBu8; 32768];
    rom[0x7FFC] = 0x00; // reset vector lo → 0x8000
    rom[0x7FFD] = 0x80; // reset vector hi
    rom
}

fn write_rom_tempfile() -> NamedTempFile {
    let f = tempfile::Builder::new().suffix(".bin").tempfile().unwrap();
    std::fs::write(f.path(), make_stp_rom()).unwrap();
    f
}

/// CLI device args that map 32 KB RAM at 0x0000 and a ROM image at 0x8000.
fn device_args(rom_path: &std::path::Path) -> Vec<String> {
    vec![
        "--cpu-variant".to_string(),
        "WDC65C02".to_string(),
        "--device".to_string(),
        "ram@0x0000,size=32768,fill=0".to_string(),
        "--device".to_string(),
        format!("rom@0x8000,size=32768,image={}", rom_path.display()),
        "--device".to_string(),
        "console@0xfff8".to_string(),
    ]
}

// ---------------------------------------------------------------------------
// Group E — emulator binary subprocess tests
// ---------------------------------------------------------------------------

#[test]
fn run_with_cli_args() {
    let rom = write_rom_tempfile();
    let output = std::process::Command::new(emulator_bin())
        .args(device_args(rom.path()))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn run_with_toml_config() {
    let rom = write_rom_tempfile();
    let toml = format!(
        r#"
cpu-variant = "WDC65C02"

[[devices]]
type = "ram"
address = 0
size = 32768

[[devices]]
type = "rom"
address = 32768
size = 32768
image = "{}"
"#,
        rom.path().display()
    );
    let cfg = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
    std::fs::write(cfg.path(), toml.as_bytes()).unwrap();
    let output = std::process::Command::new(emulator_bin())
        .args(["--config", cfg.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn run_with_cli_overrides_toml() {
    // TOML contains an unknown device type; CLI --device flags should replace it entirely.
    let rom = write_rom_tempfile();
    let toml = r#"
[[devices]]
type = "bogus"
address = 4096
size = 256
"#;
    let cfg = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
    std::fs::write(cfg.path(), toml.as_bytes()).unwrap();
    let mut args = vec![
        "--config".to_string(),
        cfg.path().to_str().unwrap().to_string(),
    ];
    args.extend(device_args(rom.path()));
    let output = std::process::Command::new(emulator_bin())
        .args(&args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn run_with_env_var_cpu_variant() {
    let rom = write_rom_tempfile();
    // Supply devices via CLI but let the env var set the CPU variant.
    let output = std::process::Command::new(emulator_bin())
        .env("EMMA6K_CPU_VARIANT", "WDC65C02")
        .args([
            "--device",
            "ram@0x0000,size=32768,fill=0",
            "--device",
            &format!("rom@0x8000,size=32768,image={}", rom.path().display()),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn run_with_invalid_env_var_cpu_variant() {
    let rom = write_rom_tempfile();
    let output = std::process::Command::new(emulator_bin())
        .env("EMMA6K_CPU_VARIANT", "NOT_A_VARIANT")
        .args([
            "--device",
            "ram@0x0000,size=32768,fill=0",
            "--device",
            &format!("rom@0x8000,size=32768,image={}", rom.path().display()),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("NOT_A_VARIANT"),
        "expected variant name in stderr, got: {stderr}"
    );
}

#[test]
fn run_with_unknown_device_type() {
    let output = std::process::Command::new(emulator_bin())
        .args([
            "--cpu-variant",
            "WDC65C02",
            "--device",
            "bogus@0x1000,size=256",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("bogus"),
        "expected device type name in stderr, got: {stderr}"
    );
}

/// The bundled default config's VIA/PTM/ACIA devices need `~/.emma/sock` and
/// `~/.emma/dev` to already exist (`UnixListener::bind`/`PtyTransport::open`
/// don't create parent directories) — ensure both so this test doesn't
/// depend on the host already having run the debugger or emulator before.
fn ensure_emma_transport_dirs() {
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        let _ = std::fs::create_dir_all(home.join(".emma/sock"));
        let _ = std::fs::create_dir_all(home.join(".emma/dev"));
    }
}

#[test]
fn run_with_no_args_uses_bundled_default_and_keeps_running() {
    ensure_emma_transport_dirs();
    let mut child = std::process::Command::new(emulator_bin())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    std::thread::sleep(Duration::from_millis(500));

    match child.try_wait().unwrap() {
        None => {
            // Still running, as expected for TaliForth's idle loop — kill it.
            let _ = child.kill();
            let _ = child.wait();
        }
        Some(status) => {
            let mut stderr = String::new();
            use std::io::Read;
            let _ = child.stderr.take().unwrap().read_to_string(&mut stderr);
            panic!(
                "expected the bundled default to keep running, but it exited with {status}: {stderr}"
            );
        }
    }
}

#[test]
fn run_with_profile_msbasic_keeps_running() {
    ensure_emma_transport_dirs();
    let mut child = std::process::Command::new(emulator_bin())
        .args(["--profile", "msbasic"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    std::thread::sleep(Duration::from_millis(500));

    match child.try_wait().unwrap() {
        None => {
            let _ = child.kill();
            let _ = child.wait();
        }
        Some(status) => {
            let mut stderr = String::new();
            use std::io::Read;
            let _ = child.stderr.take().unwrap().read_to_string(&mut stderr);
            panic!(
                "expected --profile msbasic to keep running, but it exited with {status}: {stderr}"
            );
        }
    }
}

#[test]
fn run_with_unknown_profile_fails_with_useful_message() {
    let output = std::process::Command::new(emulator_bin())
        .args(["--profile", "nope"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nope"),
        "expected profile id in stderr, got: {stderr}"
    );
}

#[test]
fn run_with_profile_and_config_together_is_rejected() {
    let cfg = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
    let output = std::process::Command::new(emulator_bin())
        .args([
            "--profile",
            "default",
            "--config",
            cfg.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn run_with_missing_rom_image() {
    // Point the ROM image attribute at a path that doesn't exist.
    let output = std::process::Command::new(emulator_bin())
        .args([
            "--cpu-variant",
            "WDC65C02",
            "--device",
            "ram@0x0000,size=32768,fill=0",
            "--device",
            "rom@0x8000,size=32768,image=/nonexistent/path/rom.bin",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// emma6k, the wrappers, and choosing the CPU
// ---------------------------------------------------------------------------

/// 32 KB 6809 ROM for $8000: prints "Hi" on a console at $C000, then loops.
fn make_6809_rom() -> Vec<u8> {
    let program = [
        0x86, b'H', //       LDA #'H'
        0xB7, 0xC0, 0x00, // STA $C000
        0x86, b'i', //       LDA #'i'
        0xB7, 0xC0, 0x00, // STA $C000
        0x20, 0xFE, //       BRA *
    ];
    let mut rom = vec![0x12u8; 32768]; // NOP
    rom[..program.len()].copy_from_slice(&program);
    rom[0x7FFE] = 0x80; // reset vector → $8000, big-endian
    rom[0x7FFF] = 0x00;
    rom
}

fn write_6809_rom_tempfile() -> NamedTempFile {
    let f = tempfile::Builder::new().suffix(".bin").tempfile().unwrap();
    std::fs::write(f.path(), make_6809_rom()).unwrap();
    f
}

/// Device args for the 6809 ROM: RAM below it and the console inside it.
fn device_args_6809(rom_path: &std::path::Path) -> Vec<String> {
    vec![
        "--device".to_string(),
        "ram@0x0000,size=32768,fill=0".to_string(),
        "--device".to_string(),
        format!("rom@0x8000,size=32768,image={}", rom_path.display()),
        "--device".to_string(),
        "console@0xc000".to_string(),
    ]
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Runs `command` for half a second, expecting it to still be running, then
/// stops it and returns what it wrote to stdout. Stdin is an open pipe: the
/// console's stdio transport closes, output included, when stdin ends.
fn run_briefly(mut command: std::process::Command) -> String {
    use std::io::Read;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    if let Some(status) = child.try_wait().unwrap() {
        let mut stderr = String::new();
        let _ = child.stderr.take().unwrap().read_to_string(&mut stderr);
        panic!("expected it to keep running, but it exited with {status}: {stderr}");
    }
    let _ = child.kill();
    let _ = child.wait();
    let mut stdout = String::new();
    let _ = child.stdout.take().unwrap().read_to_string(&mut stdout);
    stdout
}

#[test]
fn emma09_runs_a_6809_rom() {
    let rom = write_6809_rom_tempfile();
    let mut command = std::process::Command::new(emma09_bin());
    command.args(device_args_6809(rom.path()));
    let stdout = run_briefly(command);
    assert!(stdout.contains("Hi"), "stdout: {stdout:?}");
}

#[test]
fn emma6k_runs_a_6809_chosen_on_the_command_line() {
    let rom = write_6809_rom_tempfile();
    let mut command = std::process::Command::new(emma6k_bin());
    command
        .args(["--cpu", "6809"])
        .args(device_args_6809(rom.path()));
    let stdout = run_briefly(command);
    assert!(stdout.contains("Hi"), "stdout: {stdout:?}");
}

#[test]
fn emma6k_cpu_variant_env_var_takes_effect() {
    let rom = write_6809_rom_tempfile();
    let mut command = std::process::Command::new(emma6k_bin());
    command
        .env("EMMA6K_CPU_VARIANT", "6809")
        .args(device_args_6809(rom.path()));
    let stdout = run_briefly(command);
    assert!(stdout.contains("Hi"), "stdout: {stdout:?}");
}

#[test]
fn emma65_env_vars_are_ignored() {
    // Without an EMMA6K_ variable, emma6k has no CPU, whatever EMMA65_ says.
    let rom = write_6809_rom_tempfile();
    let output = std::process::Command::new(emma6k_bin())
        .env("EMMA65_CPU_VARIANT", "6809")
        .args(device_args_6809(rom.path()))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        stderr_of(&output).contains("no CPU selected"),
        "stderr: {}",
        stderr_of(&output)
    );
}

#[test]
fn emma6k_with_no_configuration_has_no_cpu() {
    let output = std::process::Command::new(emma6k_bin()).output().unwrap();
    assert!(!output.status.success());
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("no CPU selected") && stderr.contains("--cpu"),
        "stderr: {stderr}"
    );
}

#[test]
fn emma09_with_no_configuration_boots_camelforth() {
    let stdout = run_briefly(std::process::Command::new(emma09_bin()));
    assert!(stdout.contains("6809 CamelForth"), "stdout: {stdout:?}");
}

#[test]
fn emma6k_profile_camelforth_boots_camelforth() {
    let mut command = std::process::Command::new(emma6k_bin());
    command.args(["--profile", "camelforth"]);
    let stdout = run_briefly(command);
    assert!(stdout.contains("6809 CamelForth"), "stdout: {stdout:?}");
}

#[test]
fn setting_cpu_and_cpu_variant_together_is_an_error() {
    let output = std::process::Command::new(emma6k_bin())
        .args(["--cpu", "6809", "--cpu-variant", "6809"])
        .output()
        .unwrap();
    assert!(!output.status.success());

    let cfg = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
    std::fs::write(cfg.path(), "cpu = \"6809\"\ncpu-variant = \"6809\"\n").unwrap();
    let output = std::process::Command::new(emma6k_bin())
        .args(["--config", cfg.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("sets both cpu and cpu-variant"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_wrapper_rejects_another_familys_variant() {
    let output = std::process::Command::new(emulator_bin())
        .args(["--cpu", "6809"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("emma65 runs only 6502-family CPUs") && stderr.contains("use emma09"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_wrapper_rejects_another_familys_profile() {
    let output = std::process::Command::new(emma09_bin())
        .args(["--profile", "snake"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("emma09 runs only 6809-family CPUs")
            && stderr.contains("profile 'snake' is for the 6502"),
        "stderr: {stderr}"
    );
}

fn tracer_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_emma6k-tracer"))
}

/// Runs `command` (an emulator whose program stops by itself) with
/// `--trace-file`, then decodes the trace with `emma6k-tracer`, returning
/// its listing.
fn record_and_decode(mut command: std::process::Command, extra: &[&str]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let trace = dir.path().join("run.trace");
    let output = command
        .arg("--trace-file")
        .arg(&trace)
        .stdin(Stdio::piped())
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", stderr_of(&output));
    let decoded = std::process::Command::new(tracer_bin())
        .arg(&trace)
        .args(extra)
        .output()
        .unwrap();
    assert!(decoded.status.success(), "stderr: {}", stderr_of(&decoded));
    String::from_utf8(decoded.stdout).unwrap()
}

#[test]
fn a_6502_trace_decodes_with_emma6k_tracer() {
    let rom = write_rom_tempfile();
    let mut command = std::process::Command::new(emulator_bin());
    command.args(device_args(rom.path()));
    let listing = record_and_decode(command, &[]);
    let lines: Vec<&str> = listing.lines().collect();
    assert!(lines[0].contains("NV-BDIZC"), "{listing}");
    assert_eq!(lines.len(), 3, "{listing}");
    assert!(
        lines[2].contains("8000 DB") && lines[2].ends_with("STP"),
        "{listing}"
    );
}

/// A 6809 ROM for $8000 that counts down B and stops in SYNC (interrupts
/// are masked from reset, so nothing wakes it and the run ends).
fn write_6809_counting_rom() -> NamedTempFile {
    let program = [
        0xC6, 0x03, // LDB #3
        0x5A, //       DECB
        0x26, 0xFD, // BNE $8002
        0x13, //       SYNC
    ];
    let mut rom = vec![0x12u8; 32768];
    rom[..program.len()].copy_from_slice(&program);
    rom[0x7FFE] = 0x80;
    rom[0x7FFF] = 0x00;
    let f = tempfile::Builder::new().suffix(".bin").tempfile().unwrap();
    std::fs::write(f.path(), rom).unwrap();
    f
}

#[test]
fn a_6809_trace_decodes_with_emma6k_tracer() {
    let rom = write_6809_counting_rom();
    let labels = tempfile::Builder::new().suffix(".lbl").tempfile().unwrap();
    std::fs::write(labels.path(), "al 8002 .LOOP\n").unwrap();
    let mut command = std::process::Command::new(emma09_bin());
    command.args(device_args_6809(rom.path()));
    let symbol_file = labels.path().to_str().unwrap().to_string();
    let listing = record_and_decode(command, &["--symbol-file", &symbol_file, "--verbose"]);
    let lines: Vec<&str> = listing.lines().collect();
    assert!(lines[0].contains("EFHINZVC"), "{listing}");
    let instructions: Vec<&str> = lines[2..]
        .iter()
        .filter(|l| !l.trim_end().ends_with(':'))
        .map(|l| l[76..].trim_end())
        .collect();
    assert_eq!(
        instructions,
        [
            "LDB   #$03      ; 3 ^C (ETX)",
            "DECB",
            "BNE   LOOP",
            "DECB",
            "BNE   LOOP",
            "DECB",
            "BNE   LOOP",
            "SYNC",
        ],
        "{listing}"
    );
    assert_eq!(lines.iter().filter(|l| l.ends_with("LOOP:")).count(), 3);
    // The first row: 2 cycles, and CC with F and I set from reset.
    assert!(lines[2].starts_with("         1   2 00 00"), "{listing}");
    assert!(lines[2].contains(" 50  F I     8000 C6 03 "), "{listing}");
}

#[test]
fn emma6k_tracer_rejects_a_file_that_isnt_a_trace() {
    let not_a_trace = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(not_a_trace.path(), b"not a trace file").unwrap();
    let output = std::process::Command::new(tracer_bin())
        .arg(not_a_trace.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        stderr_of(&output).contains("magic"),
        "{}",
        stderr_of(&output)
    );
}

#[test]
fn a_wrapper_names_itself_in_usage_errors() {
    let output = std::process::Command::new(emulator_bin())
        .arg("--no-such-flag")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = stderr_of(&output);
    assert!(stderr.contains("emma65"), "stderr: {stderr}");
}

// ---------------------------------------------------------------------------
// The exit device
// ---------------------------------------------------------------------------

/// Runs `command` with stdin an open pipe, first writing `input` to it, and
/// waits up to ten seconds for it to exit by itself. Returns its exit code.
fn exit_code_of(mut command: std::process::Command, input: &[u8]) -> i32 {
    use std::io::Write;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            drop(stdin);
            return status.code().expect("exited by a signal");
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the emulator didn't exit by itself");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A 32 KB ROM for $8000 holding `program`, then a loop, with the reset
/// vector at the end in the given byte order.
fn rom_with(program: &[u8], loop_bytes: &[u8], big_endian: bool) -> NamedTempFile {
    let mut rom = vec![0u8; 32768];
    rom[..program.len()].copy_from_slice(program);
    rom[program.len()..program.len() + loop_bytes.len()].copy_from_slice(loop_bytes);
    if big_endian {
        rom[0x7FFE] = 0x80;
    } else {
        rom[0x7FFD] = 0x80;
    }
    let f = tempfile::Builder::new().suffix(".bin").tempfile().unwrap();
    std::fs::write(f.path(), rom).unwrap();
    f
}

/// Writes `EXIT` and `status` to $7000 with LDA # / STA abs, which both CPUs
/// encode alike except for the store's byte order and opcode.
fn exit_program(status: u8, sta: u8, big_endian: bool) -> Vec<u8> {
    let mut program = Vec::new();
    let lda = if sta == 0x8D { 0xA9 } else { 0x86 };
    let addr: [u8; 2] = if big_endian {
        [0x70, 0x00]
    } else {
        [0x00, 0x70]
    };
    for b in [b'E', b'X', b'I', b'T', status] {
        program.extend_from_slice(&[lda, b, sta, addr[0], addr[1]]);
    }
    program
}

#[test]
fn a_6502_program_exits_with_its_status() {
    // LDA #b; STA $7000, then BRA * (only on the 65C02).
    let rom = rom_with(&exit_program(42, 0x8D, false), &[0x80, 0xFE], false);
    let mut command = std::process::Command::new(emulator_bin());
    command.args(device_args(rom.path()));
    command.args(["--device", "exit@0x7000"]);
    assert_eq!(exit_code_of(command, b""), 42);
}

#[test]
fn a_6809_program_exits_with_its_status() {
    // LDA #b; STA $7000, then BRA *.
    let rom = rom_with(&exit_program(3, 0xB7, true), &[0x20, 0xFE], true);
    let mut command = std::process::Command::new(emma09_bin());
    command.args(device_args_6809(rom.path()));
    command.args(["--device", "exit@0x7000"]);
    assert_eq!(exit_code_of(command, b""), 3);
}

#[test]
fn camelforth_bye_ends_emma09_with_status_0() {
    let command = std::process::Command::new(emma09_bin());
    assert_eq!(exit_code_of(command, b"BYE\r"), 0);
}
