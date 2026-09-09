//! Throwaway benchmark comparing clock-throttling accuracy/ceiling between
//! the full default device complement and a minimal RAM+ROM+console-only
//! configuration running the same TaliForth image.
//!
//! Run with `cargo run --release --example clock_bench_minimal`.
//!
//! This file is not part of the crate — the clock-speed-benchmark skill copies
//! it into examples/ for a one-off run and deletes it afterward. Do not commit
//! it or add it to Cargo.toml.

use emma65::emulator::config::templates;
use emma65::emulator::{Config, DeviceRegistry};
use figment::Figment;
use figment::providers::{Format, Toml};
use std::time::{Duration, Instant};

async fn bench(
    label: &str,
    base: Config,
    devices: Vec<String>,
    targets_mhz: &[f64],
    run_secs: f64,
) {
    let registry = DeviceRegistry::with_builtins();
    println!("\n== {label} ==");
    println!("{:>10} | {:>12} | {:>9}", "target MHz", "actual MHz", "error %");
    println!("{:->10}-+-{:->12}-+-{:->9}", "", "", "");

    let mut base = base;
    base.devices = Some(devices.iter().map(|s| s.parse().expect("device spec")).collect());

    for &mhz in targets_mhz {
        let hz = (mhz * 1_000_000.0).round() as u64;
        let mut config = base.clone();
        config.clock_speed_hz = Some(hz);

        let session = config.build(&registry).await.expect("build session");
        let mut cpu = session.cpu;
        cpu.reset().expect("reset");

        let handle = emma65::emulator::run(cpu);
        let start = Instant::now();
        tokio::time::sleep(Duration::from_secs_f64(run_secs)).await;
        let cpu = handle.take_cpu().await;
        let elapsed = start.elapsed().as_secs_f64();

        let actual_hz = cpu.cycles() as f64 / elapsed;
        let actual_mhz = actual_hz / 1_000_000.0;
        let err_pct = (actual_mhz - mhz) / mhz * 100.0;

        println!("{mhz:>10.3} | {actual_mhz:>12.4} | {err_pct:>8.2}%");
    }

    // Unthrottled ceiling.
    let mut config = base.clone();
    config.clock_speed_hz = None;
    let session = config.build(&registry).await.expect("build session");
    let mut cpu = session.cpu;
    cpu.reset().expect("reset");
    let handle = emma65::emulator::run(cpu);
    let start = Instant::now();
    tokio::time::sleep(Duration::from_secs_f64(run_secs)).await;
    let cpu = handle.take_cpu().await;
    let elapsed = start.elapsed().as_secs_f64();
    let actual_mhz = (cpu.cycles() as f64 / elapsed) / 1_000_000.0;
    println!("unthrottled ceiling: {actual_mhz:.4} MHz");
}

#[tokio::main]
async fn main() {
    let dir = tempfile::tempdir().expect("tempdir");
    let toml_path = templates::materialize_default(dir.path()).expect("materialize default profile");
    let base: Config = Figment::new()
        .merge(Toml::file(&toml_path))
        .extract()
        .expect("parse default profile");

    let rom_path = dir.path().join("program.bin");
    let lbl_path = dir.path().join("program.lbl");

    let run_secs = 3.0_f64;

    // Full default complement, as a same-run baseline against the minimal config below.
    // Bracket around the last known ~34 MHz ceiling (see clock_bench.rs).
    bench(
        "full default complement (ram, rom, via, 2x acia, lfsr, console)",
        base.clone(),
        vec![
            "ram@0x0000,size=32768,fill=0".to_string(),
            format!(
                "rom@0x8000,size=32768,image={},labels={}",
                rom_path.display(),
                lbl_path.display()
            ),
            "via/6522@0xff80".to_string(),
            "acia/6551@0xfff0".to_string(),
            "acia/6850@0xfff4".to_string(),
            "lfsr@0xfff6,mode=step".to_string(),
            "console@0xfff8,break=0x3".to_string(),
        ],
        &[28.0, 30.0, 32.0, 34.0, 35.0, 36.0, 38.0, 40.0],
        run_secs,
    )
    .await;

    // Minimal complement — bracket around the last known ~85 MHz ceiling.
    bench(
        "minimal (ram, rom, console only) — near ceiling",
        base,
        vec![
            "ram@0x0000,size=32768,fill=0".to_string(),
            format!(
                "rom@0x8000,size=32768,image={},labels={}",
                rom_path.display(),
                lbl_path.display()
            ),
            "console@0xfff8,break=0x3".to_string(),
        ],
        &[70.0, 75.0, 80.0, 83.0, 85.0, 86.0, 87.0, 88.0, 90.0, 95.0, 100.0],
        run_secs,
    )
    .await;
}
