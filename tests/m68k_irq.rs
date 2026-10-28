//! Original guest-owned records; MC68000UM §§3.5/6.3.2.
use saturn_rust::{Boot, Machine, bus::Memory};

fn rom() -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in include_str!("fixtures/m68k-irq.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            image[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    image
}

fn records(machine: &Machine) -> Vec<u16> {
    (0..14)
        .map(|i| machine.bus().read16(0x05a00900 + i * 2).unwrap())
        .collect()
}
fn variant(operand: u32, omit: Option<u16>) -> Vec<u8> {
    let mut image = rom();
    image[0x2202..0x2206].copy_from_slice(&operand.to_be_bytes());
    if let Some(opcode) = omit {
        let stores: Vec<_> = image[..0x2000]
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
            .filter_map(|(i, b)| (*b == opcode.to_be_bytes()).then_some(i * 2))
            .collect();
        assert_eq!(stores.len(), if opcode == 0x2461 { 2 } else { 1 });
        let at = *stores.last().unwrap();
        image[at..at + 2].copy_from_slice(&9u16.to_be_bytes());
    }
    image
}

#[test]
fn sh2_rearms_level_seven_during_handler_division() {
    let image = rom();
    let mut machine = Machine::new();
    machine.reset(Boot::ExternalBios(&image)).unwrap();
    let mut events = Vec::new();
    let mut old = 0;
    for _ in 0..30000 {
        machine.run_steps(1).unwrap();
        let pending = machine.bus().read16(0x05b00420).unwrap() & 32;
        if pending != old {
            let cpu = machine.sound_state();
            assert_eq!(cpu.sr & 0x700, 0x700);
            events.push((
                pending,
                cpu.pc,
                cpu.instructions,
                cpu.d[0],
                cpu.exceptions,
                machine.clocks(),
            ));
            old = pending;
        }
    }
    assert_eq!(events.len(), 3);
    assert_eq!((events[0].0, events[0].4), (32, 0));
    for event in &events[1..] {
        assert_eq!((event.1, event.2, event.3, event.4), (0x320, 14, 7, 1));
    }
    assert_eq!((events[1].0, events[2].0), (0, 32));
    assert!(events[2].5 - events[1].5 >= 64, "low interval: {events:?}");
    assert_eq!(
        machine.bus().read32(0x05a00940),
        Ok(1),
        "first handler must execute"
    );
    assert_eq!(machine.sound_state().d[0], 0x10003, "DIV must complete");
    assert_eq!(
        machine.sound_state().exceptions,
        2,
        "{:?}",
        machine.sound_state()
    );
}

#[test]
fn guest_controls_records_cold_boots_and_advancement_chunks() {
    for (operand, omit, result) in [
        (7, None, 0x10003),
        (9, None, 0x10004),
        (7, Some(0x2561), 0x10003),
        (7, Some(0x2461), 0x10003),
    ] {
        let image = variant(operand, omit);
        let mut expected = vec![0x2700, 0, 0x21e, 0, operand as u16, 0, 1];
        if omit.is_none() {
            expected.extend([0x2700, 0, 0x322, 1, (result & 65535) as u16, 0, 2]);
        } else {
            expected.extend([0; 7]);
        }
        for chunk in [1, 3, 17, 10000] {
            for inspect in [false, true] {
                let mut machine = Machine::new();
                for _ in 0..2 {
                    machine.reset(Boot::ExternalBios(&image)).unwrap();
                    let mut remaining = 30000;
                    while remaining > 0 {
                        let steps = remaining.min(chunk);
                        machine.run_steps(steps).unwrap();
                        if inspect {
                            let _ = records(&machine);
                            let _ = machine.sound_state();
                        }
                        remaining -= steps;
                    }
                    assert_eq!(
                        records(&machine),
                        expected,
                        "operand={operand} omit={omit:?} chunk={chunk} inspect={inspect}"
                    );
                    assert_eq!(
                        machine.sound_state().exceptions,
                        if omit.is_none() { 2 } else { 1 }
                    );
                    assert_eq!(machine.sound_state().last_vector, Some(31));
                    assert_eq!(machine.sound_state().d[0], result);
                    assert!(machine.sound_state().stopped && !machine.sound_state().halted);
                    assert_eq!(machine.cpu_state().exceptions, 0);
                    assert_eq!(
                        machine.bus().read32(0x05a00944),
                        Ok(if omit.is_none() { 0 } else { result })
                    );
                }
            }
        }
    }
}
