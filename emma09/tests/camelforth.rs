//! Boots the CamelForth starter profile through `emma09::Config` and talks to
//! it over the console: arithmetic, a new word, and Ctrl+C. This is also the
//! 6809's broadest end-to-end test, since CamelForth exercises most of the
//! instruction set and the IRQ path.

use emma6k_core::{InstantiationContext, InternalPipeTransport, Transport, TransportReporter};
use emma09::cpu::StepResult;
use emma09::{Config, device_registry, templates};
use figment::Figment;
use figment::providers::{Format, Toml};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A running CamelForth machine and the far end of its console.
struct Forth {
    cpu: emma09::Cpu,
    console: InternalPipeTransport,
    output: String,
    _profile: tempfile::TempDir,
}

impl Forth {
    async fn boot() -> Self {
        let profile = tempfile::tempdir().unwrap();
        let template = templates::TEMPLATES
            .iter()
            .find(|t| t.id == "camelforth")
            .expect("camelforth template");
        let toml = template.materialize(profile.path()).unwrap();
        let config: Config = Figment::new().merge(Toml::file(&toml)).extract().unwrap();

        let reporter = TransportReporter::pending(None);
        let ((local, relay), console) = InternalPipeTransport::pair(reporter.clone()).unwrap();
        let context = InstantiationContext {
            console_transport: Some(Arc::new(Mutex::new(Some((
                Box::new(local) as Box<dyn Transport>,
                relay,
                reporter,
            ))))),
            ..Default::default()
        };
        let session = config
            .build_with_context(&device_registry(), context)
            .await
            .unwrap();
        let mut cpu = session.cpu;
        cpu.reset().unwrap();
        Self {
            cpu,
            console,
            output: String::new(),
            _profile: profile,
        }
    }

    /// Runs the CPU until the console output contains `text`, returning the
    /// output gathered since the last call. Fails after ten seconds.
    fn run_until(&mut self, text: &str) -> String {
        let start = self.output.len();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.output[start..].contains(text) {
            assert!(
                Instant::now() < deadline,
                "no {text:?} in the console output: {:?}",
                &self.output[start..]
            );
            for _ in 0..10_000 {
                match self.cpu.step(None, false) {
                    StepResult::Executed(_) => {}
                    StepResult::Error(e) => panic!("CPU error: {e}"),
                    _ => panic!("the CPU stopped running"),
                }
            }
            while let Some(byte) = self.console.try_recv() {
                self.output.push(byte as char);
            }
        }
        self.output[start..].to_string()
    }

    /// Types `text` at the console.
    fn type_text(&mut self, text: &str) {
        for byte in text.bytes() {
            self.console.send(byte);
        }
    }
}

#[tokio::test]
async fn camelforth_boots_and_evaluates() {
    let mut forth = Forth::boot().await;
    let banner = forth.run_until("6809 CamelForth");
    assert!(banner.contains("6809 CamelForth"), "{banner:?}");

    forth.type_text("2 3 + .\r");
    let reply = forth.run_until("OK");
    assert!(reply.contains("5 "), "{reply:?}");

    forth.type_text(": SQUARE DUP * ; 7 SQUARE .\r");
    let reply = forth.run_until("OK");
    assert!(reply.contains("49 "), "{reply:?}");
}

#[tokio::test]
async fn ctrl_c_abandons_a_runaway_word() {
    let mut forth = Forth::boot().await;
    forth.run_until("6809 CamelForth");

    // An endless loop never reaches the next prompt...
    forth.type_text(": SPIN BEGIN AGAIN ; SPIN\r");
    for _ in 0..200_000 {
        forth.cpu.step(None, false);
    }
    // ...until Ctrl+C returns to the interpreter, which still works.
    forth.type_text("\x03");
    forth.type_text("6 7 * .\r");
    let reply = forth.run_until("OK");
    assert!(reply.contains("42 "), "{reply:?}");
}
