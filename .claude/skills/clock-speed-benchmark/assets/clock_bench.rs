//! Throwaway benchmark for measuring where clock-speed throttling accuracy
//! actually falls off, using the bundled default device complement (32K RAM,
//! TaliForth ROM, VIA, two ACIAs, LFSR, console).
//!
//! Run with `cargo run --release --example clock_bench`.
//!
//! This file is not part of the crate — the clock-speed-benchmark skill copies
//! it into examples/ for a one-off run and deletes it afterward. Do not commit
//! it or add it to Cargo.toml.

use emma65::emulator::config::templates;
use emma65::emulator::{Config, DeviceRegistry};
use figment::Figment;
use figment::providers::{Format, Toml};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let registry = DeviceRegistry::with_builtins();
    let dir = tempfile::tempdir().expect("tempdir");
    let toml_path = templates::materialize_default(dir.path()).expect("materialize default profile");

    // Bracket the last known accurate-throttling ceiling for the bundled default
    // complement (~34 MHz as of 2026-09-09, see doc/src/the-emulator-core.md).
    // If this run's numbers fall off (or hold steady) well outside this range,
    // widen/shift this list and rerun before drawing conclusions.
    let targets_mhz: Vec<f64> = vec![
        1.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 30.0, 32.0, 33.0,
        34.0, 34.5, 35.0, 35.5, 36.0, 37.0, 38.0, 40.0, 45.0, 50.0,
    ];
    let run_secs = 3.0_f64;

    println!("{:>10} | {:>12} | {:>9}", "target MHz", "actual MHz", "error %");
    println!("{:->10}-+-{:->12}-+-{:->9}", "", "", "");

    for mhz in targets_mhz {
        let hz = (mhz * 1_000_000.0).round() as u64;
        let mut config: Config = Figment::new()
            .merge(Toml::file(&toml_path))
            .extract()
            .expect("parse default profile");
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

    // Reference: unthrottled ceiling, same device complement.
    let mut config: Config = Figment::new()
        .merge(Toml::file(&toml_path))
        .extract()
        .expect("parse default profile");
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
    println!("\nunthrottled ceiling: {actual_mhz:.4} MHz");
}
