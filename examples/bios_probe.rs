//! Local firmware probe. Firmware and generated frames must remain private.
use saturn_rust::{Boot, Machine, MachineError, cpu::CpuError, media::Media};
use std::{env, fs, io::Read, path::Path, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let path = args.next().ok_or(
        "usage: bios_probe BIOS.bin [frames] [output.ppm] [buttons.tsv|-] [--cue CUE TRACK...]",
    )?;
    let frames: u64 = args.next().unwrap_or_else(|| "200".into()).parse()?;
    if frames == 0 {
        return Err("frames must be positive".into());
    }
    let output = args.next();
    let mut inputs = Vec::new();
    if let Some(path) = args.next().filter(|p| p != "-") {
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
    if let Some(flag) = args.next() {
        if flag != "--cue" {
            return Err("expected --cue CUE TRACK... after the button file or -".into());
        }
        let cue = args.next().ok_or("missing CUE path")?;
        if fs::metadata(&cue)?.len() > 1024 * 1024 {
            return Err("CUE exceeds 1 MiB".into());
        }
        let cue = fs::read_to_string(cue)?;
        let tracks: Vec<_> = args.collect();
        if tracks.is_empty() || tracks.len() > 99 {
            return Err("supply 1 to 99 referenced track files".into());
        }
        let mut bytes = Vec::new();
        let mut names = Vec::new();
        for track in &tracks {
            let length = usize::try_from(fs::metadata(track)?.len())?;
            if length > saturn_rust::disc::MAX_MEDIA_BYTES.saturating_sub(bytes.len()) {
                return Err("track files exceed media limits".into());
            }
            let name = Path::new(track)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("track filename must be UTF-8")?;
            let start = bytes.len();
            bytes.try_reserve_exact(length)?;
            fs::File::open(track)?
                .take(length as u64 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() - start != length {
                return Err("track file changed while reading".into());
            }
            names.push((name, start..bytes.len()));
        }
        machine.mount_media(Media::cue_owned(&cue, &names, bytes)?);
    }
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
