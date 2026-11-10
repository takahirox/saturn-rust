//! Local firmware probe. Firmware and generated frames must remain private.
use saturn_rust::{Boot, Machine, MachineError, cpu::CpuError};
use std::{env, fs, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: bios_probe BIOS.bin [frames] [output.ppm] [buttons.tsv]")?;
    let frames: u64 = args.next().unwrap_or_else(|| "200".into()).parse()?;
    if frames == 0 {
        return Err("frames must be positive".into());
    }
    let output = args.next();
    let mut inputs = Vec::new();
    if let Some(path) = args.next() {
        for line in fs::read_to_string(path)?
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        {
            let mut fields = line.split_whitespace();
            let frame: u64 = fields.next().ok_or("missing frame")?.parse()?;
            let buttons = u16::from_str_radix(
                fields
                    .next()
                    .ok_or("missing hexadecimal button mask")?
                    .trim_start_matches("0x"),
                16,
            )?;
            if fields.next().is_some()
                || inputs
                    .last()
                    .is_some_and(|(previous, _)| *previous >= frame)
            {
                return Err(
                    "button events must have two fields and increasing frame numbers".into(),
                );
            }
            inputs.push((frame, buttons));
        }
    }
    let mut inputs = inputs.into_iter().peekable();
    let bios = fs::read(path)?;
    let mut machine = Machine::new();
    machine.reset(Boot::ExternalBios(&bios))?;
    let started = Instant::now();
    for frame in 0..frames {
        if inputs.peek().is_some_and(|(at, _)| *at == frame) {
            machine.set_buttons(inputs.next().unwrap().1);
        }
        if let Err(error) = machine.advance_frame() {
            if let MachineError::Cpu(CpuError::Bus { pc, .. }) = &error {
                eprintln!(
                    "fault_pc={pc:08X} instruction={:04X?}",
                    machine.debug_read16(*pc)
                );
            }
            eprintln!(
                "frame={frame} clocks={} cpu={:?} slave={:?}",
                machine.clocks(),
                machine.cpu_state(),
                machine.slave_state()
            );
            eprintln!("SMPC={:?}", machine.smpc_fault_diagnostics());
            return Err(error.into());
        }
        if frame % 60 == 0 || frame + 1 == frames {
            println!(
                "frame={} clocks={} pc={:08X} instructions={} elapsed={:.2}s",
                machine.frames(),
                machine.clocks(),
                machine.cpu_state().pc,
                machine.cpu_state().instructions,
                started.elapsed().as_secs_f64()
            );
            println!(
                "sound_pc={:06X} halted={} instructions={} exceptions={}",
                machine.sound_state().pc,
                machine.sound_state().halted,
                machine.sound_state().instructions,
                machine.sound_state().exceptions
            );
        }
    }
    println!(
        "master={:?}\nslave={:?}",
        machine.cpu_state(),
        machine.slave_state()
    );
    println!("sound={:?}", machine.sound_state());
    if let Some(output) = output {
        let (width, height) = machine.video_dimensions();
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        for pixel in machine.framebuffer()?.as_chunks::<4>().0 {
            ppm.extend_from_slice(&pixel[..3]);
        }
        fs::write(output, ppm)?;
    }
    Ok(())
}
