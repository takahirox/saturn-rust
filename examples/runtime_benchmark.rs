//! Optional host-throughput measurement; never a timing oracle or CI threshold.
use saturn_rust::{Boot, Machine};
use std::{collections::BTreeMap, env, error::Error, fs, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let mut frames = 60u64;
    let mut runs = 3u32;
    let mut bios = None;
    let mut vdp1_stores = false;
    let mut draw = false;
    let mut skip = 0u64;
    let mut buttons = BTreeMap::new();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--frames" => frames = args.next().ok_or("missing frame count")?.parse()?,
            "--runs" => runs = args.next().ok_or("missing run count")?.parse()?,
            "--bios" => bios = Some(fs::read(args.next().ok_or("missing local BIOS path")?)?),
            "--draw" => draw = true,
            "--vdp1-stores" => vdp1_stores = true,
            "--skip" => skip = args.next().ok_or("missing skipped fields")?.parse()?,
            "--buttons" => {
                let path = args.next().ok_or("missing button events")?;
                let source = fs::read_to_string(path)?;
                let mut previous = None;
                for line in source
                    .lines()
                    .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
                {
                    let fields = line.split_whitespace().collect::<Vec<_>>();
                    if fields.len() != 2 {
                        return Err("expected field and hexadecimal button mask".into());
                    }
                    let field = fields[0].parse::<u64>()?;
                    let mask = u16::from_str_radix(fields[1], 16)?;
                    if previous.is_some_and(|old| field <= old) {
                        return Err("button fields must increase".into());
                    }
                    buttons.insert(field, mask);
                    previous = Some(field);
                }
            }
            _ => {
                return Err(
                    "usage: runtime_benchmark [--frames N] [--runs N] [--bios FILE] [--vdp1-stores] [--draw] [--skip N] [--buttons FILE]"
                        .into(),
                );
            }
        }
    }
    if frames == 0 || runs == 0 {
        return Err("frames and runs must be positive".into());
    }
    if vdp1_stores && bios.is_some() {
        return Err("--vdp1-stores uses the original guest; omit --bios".into());
    }
    let bytes = if let Some(bios) = bios {
        bios
    } else {
        let mut rom = vec![0u8; 524288];
        for line in include_str!("runtime-loop.hex")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let mut words = line.split_whitespace();
            let Some(address) = words.next() else {
                continue;
            };
            let address = usize::from_str_radix(address, 16)?;
            for (i, word) in words.enumerate() {
                let word = u16::from_str_radix(word, 16)?.to_be_bytes();
                rom[address + i * 2..address + i * 2 + 2].copy_from_slice(&word);
            }
        }
        if vdp1_stores {
            if rom[0x224..0x228] != 0x0602_0000u32.to_be_bytes() {
                return Err("original loop target changed".into());
            }
            rom[0x224..0x228].copy_from_slice(&0x05C0_2000u32.to_be_bytes());
        }
        rom
    };
    let mut previous = None;
    for run in 0..runs {
        let mut machine = Machine::new();
        machine.reset(Boot::ExternalBios(&bytes))?;
        for _ in 0..skip {
            if let Some(&mask) = buttons.get(&machine.frames()) {
                machine.set_buttons(mask);
            }
            machine.advance_frame()?;
        }
        let started = Instant::now();
        for _ in 0..frames {
            if let Some(&mask) = buttons.get(&machine.frames()) {
                machine.set_buttons(mask);
            }
            machine.advance_frame()?;
            if draw {
                std::hint::black_box(machine.framebuffer()?);
            }
        }
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let cpu = machine.cpu_state();
        let state = format!(
            "{{\"frames\":{},\"clocks\":{},\"instructions\":{},\"exceptions\":{},\"registers\":{:?},\"special\":{:?},\"mailbox\":{},\"commandWord\":{},\"pixels\":{}}}",
            machine.frames(),
            machine.clocks(),
            cpu.instructions,
            cpu.exceptions,
            cpu.r,
            [cpu.pc, cpu.sr, cpu.pr, cpu.gbr, cpu.vbr, cpu.mach, cpu.macl],
            machine.debug_read32(0x06020000)?,
            machine.debug_read32(0x05C02000)?,
            machine
                .framebuffer()?
                .iter()
                .fold(2166136261u32, |hash, &byte| (hash ^ u32::from(byte))
                    .wrapping_mul(16777619)),
        );
        if let Some(previous) = &previous {
            assert_eq!(&state, previous, "cold runs changed guest results");
        }
        println!(
            "{{\"run\":{run},\"ms\":{ms:.3},\"fps\":{:.3},\"state\":{state}}}",
            frames as f64 * 1000.0 / ms
        );
        previous = Some(state);
    }
    Ok(())
}
