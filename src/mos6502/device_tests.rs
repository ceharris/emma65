//! 6502-driven device integration tests: a 65C02 program talking to a console
//! over a transport, and the PIC's vector resolver redirecting an IRQ. They
//! moved here from `emma6k_core` when the core became its own crate, since the
//! core can't depend on the 6502.

use crate::mos6502::cpu::status::StatusRegister;
use crate::mos6502::cpu::variant::CpuVariant;
use crate::mos6502::cpu::{Cpu, StepResult};
use crossbeam_channel::{Sender, unbounded};
use emma6k_core::bus::DeviceIdAllocator;
use emma6k_core::config::{DeviceModule, InstantiationContext, PicFinchModule};
use emma6k_core::device::Console;
use emma6k_core::transport::{ChannelRelay, TransportRelay};
use emma6k_core::{AddressRange, BusConfig, DeviceId, InternalPipeTransport, IrqSource};
use figment::value::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn device() -> Console {
    Console::new("console")
}

/// A `ChannelRelay<u8>` fed by a plain, unbounded channel, so the test's own
/// `tx.send()` never blocks (see the same helper in the console's own tests).
fn spawn_byte_relay(capacity: usize) -> (Sender<u8>, ChannelRelay<u8>) {
    let (tx, rx) = unbounded();
    (tx, ChannelRelay::spawn(rx, capacity))
}

#[test]
fn integration_cpu_program_writes_appear_on_transport() {
    let (local, mut remote) = InternalPipeTransport::pair_direct().unwrap();
    let (_tx, relay) = spawn_byte_relay(256);
    let mut console = device().with_address(0xF000);
    console.attach_transport(Box::new(local), TransportRelay::Byte(relay));

    // Map all of RAM (including reset vector region) plus console at 0xF000.
    // Using RAM for 0xFF00–0xFFFF lets us write the reset vector after build().
    let bus = BusConfig::new()
        .ram_with_fill(AddressRange::new(0x0000, 0xEFFF), 0)
        .unwrap()
        .device(
            AddressRange::new(0xF000, 0xF001),
            DeviceId(1),
            Box::new(console),
        )
        .unwrap()
        .ram_with_fill(AddressRange::new(0xFF00, 0xFFFF), 0)
        .unwrap()
        .build();

    let mut cpu = Cpu::builder(CpuVariant::Wdc65C02).bus(bus).build().unwrap();

    // Write program into RAM at 0x0200:
    //   LDA #$41   ; A9 41
    //   STA $F000  ; 8D 00 F0  -- write 'A' to console output
    //   LDA #$42   ; A9 42
    //   STA $F000  ; 8D 00 F0  -- write 'B'
    //   STP        ; DB
    let prog: &[u8] = &[
        0xA9, 0x41, 0x8D, 0x00, 0xF0, 0xA9, 0x42, 0x8D, 0x00, 0xF0, 0xDB,
    ];
    for (i, &b) in prog.iter().enumerate() {
        let _ = cpu.bus_mut().write(0x0200 + i as u16, b);
    }
    // Reset vector → 0x0200.
    let _ = cpu.bus_mut().write(0xFFFC, 0x00);
    let _ = cpu.bus_mut().write(0xFFFD, 0x02);

    let _ = cpu.reset();
    loop {
        match cpu.step(None, true) {
            StepResult::Stopped => break,
            StepResult::Error(e) => panic!("CPU error: {:?}", e),
            _ => {}
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(1));
    assert_eq!(remote.try_recv(), Some(0x41));
    assert_eq!(remote.try_recv(), Some(0x42));
}

#[test]
fn integration_transport_input_readable_by_cpu() {
    let (local, _remote) = InternalPipeTransport::pair_direct().unwrap();
    let (tx, relay) = spawn_byte_relay(256);
    let mut console = device().with_address(0xF000);
    console.attach_transport(Box::new(local), TransportRelay::Byte(relay));

    let bus = BusConfig::new()
        .ram_with_fill(AddressRange::new(0x0000, 0xEFFF), 0)
        .unwrap()
        .device(
            AddressRange::new(0xF000, 0xF001),
            DeviceId(1),
            Box::new(console),
        )
        .unwrap()
        .ram_with_fill(AddressRange::new(0xFF00, 0xFFFF), 0)
        .unwrap()
        .build();

    let mut cpu = Cpu::builder(CpuVariant::Wdc65C02).bus(bus).build().unwrap();

    // Program at 0x0200:
    //   NOP        ; EA        -- tick the bus at least once
    //   LDA $F001  ; AD 01 F0  -- latch a byte from transport (latch reg)
    //   STA $0300  ; 8D 00 03  -- store it in RAM
    //   STP        ; DB
    let prog: &[u8] = &[0xEA, 0xAD, 0x01, 0xF0, 0x8D, 0x00, 0x03, 0xDB];
    for (i, &b) in prog.iter().enumerate() {
        let _ = cpu.bus_mut().write(0x0200 + i as u16, b);
    }
    let _ = cpu.bus_mut().write(0xFFFC, 0x00);
    let _ = cpu.bus_mut().write(0xFFFD, 0x02);

    let _ = cpu.reset();

    // Send a byte from the remote end before the CPU starts.
    tx.send(0x5A).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    loop {
        match cpu.step(None, true) {
            StepResult::Stopped => break,
            StepResult::Error(e) => panic!("CPU error: {:?}", e),
            _ => {}
        }
    }
    cpu.bus_mut().tick_devices(1);
    assert_eq!(cpu.bus_mut().read(0x0300).unwrap(), 0x5A);
}

#[tokio::test]
async fn instantiate_installs_a_vector_resolver_that_tracks_ier_writes() {
    let module = PicFinchModule;
    let attributes: HashMap<String, Value> = HashMap::new();
    let id_allocator = Arc::new(Mutex::new(DeviceIdAllocator::new()));
    // RAM everywhere except the PIC's own IER byte at 0xFFFF, so the vector table
    // (backed by ROM in a real system) can actually store the bytes we write.
    let bus_config = BusConfig::new()
        .ram(AddressRange::new(0x0000, 0xFFFE))
        .unwrap();
    let mut bus_config = module
        .instantiate(
            bus_config,
            0xFFFF,
            &attributes,
            &InstantiationContext::default(),
            id_allocator,
        )
        .await
        .unwrap();
    let resolver = bus_config.take_vector_resolver();
    let bus = bus_config.build();
    let mut builder = Cpu::builder(CpuVariant::Wdc65C02).bus(bus);
    if let Some(resolver) = resolver {
        builder = builder.vector_resolver(resolver);
    }
    let mut cpu = builder.build().unwrap();

    cpu.bus_mut().write(0xFFFF, 0x80 | (1 << 3)).unwrap(); // enable slot 3
    cpu.bus_mut().write(0xFFE6, 0x00).unwrap(); // slot 3 vector low byte
    cpu.bus_mut().write(0xFFE7, 0x04).unwrap(); // slot 3 vector high byte
    cpu.interrupts_mut().assert_irq(IrqSource(3));
    cpu.registers_mut().p.remove(StatusRegister::I);

    cpu.step(None, true);
    assert_eq!(cpu.registers().pc, 0x0400);
}
