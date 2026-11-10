//! Transactional sound lookahead for CPU boundaries farther apart than a bus phase.
//! Copy local synthesis/CPU state, retaining RAM with a physical-byte undo log.
use super::*;

#[derive(Default)]
pub(super) struct RamUndo {
    bytes: Vec<(usize, u8)>,
    pub(super) audio: Vec<i16>,
}
impl RamUndo {
    pub(super) fn record(&mut self, ram: &[u8], config: RamConfig, address: usize, width: usize) {
        for offset in 0..width {
            if let Some(index) = config.offset(address + offset) {
                self.bytes.push((index, ram[index]));
            }
        }
    }
}

pub(crate) struct SoundCheckpoint {
    state: Sound,
    undo: RamUndo,
    output_len: usize,
    #[cfg(test)]
    trace_len: usize,
}
impl Sound {
    pub(crate) fn cached_lookahead_checkpoint(
        &mut self,
        clocks: u32,
    ) -> Result<(u32, SoundCheckpoint), BusError> {
        // Snapshot before diagnostic admission, which memoizes DSP validity.
        // Keep host samples in place and journal overwritten ones instead of
        // cloning the 32 KiB queue; independent synthesis state still clones.
        let output = std::mem::take(&mut self.scsp.output);
        let scsp = self.scsp.clone();
        self.scsp.output = output;
        let mut checkpoint = SoundCheckpoint {
            state: Self {
                cpu: self.cpu.clone(),
                // CPU and DSP writes below record physical bytes before changing
                // RAM. DMA and main-CPU RAM requests are excluded by admission.
                ram: Vec::new(),
                scsp,
                enabled: self.enabled,
                time: self.time,
                cpu_ticks: self.cpu_ticks,
                cpu_origin: self.cpu_origin,
                next_cpu: self.next_cpu,
                first_cpu_request: self.first_cpu_request,
                ram_busy_until: self.ram_busy_until,
                memory: self.memory.checkpoint_state(),
                scsp_tick: self.scsp_tick,
                reset_output_start: self.reset_output_start,
                reset_output_end: self.reset_output_end,
                transactions: self.transactions.clone(),
            },
            undo: RamUndo::default(),
            output_len: self.scsp.output.len(),
            #[cfg(test)]
            trace_len: self.memory.trace.len(),
        };
        debug_assert!(self.cached_lookahead_allowed());
        let start = self.time;
        if let Err(error) =
            self.advance_domain_recorded(clocks, None, true, Some(&mut checkpoint.undo))
        {
            self.restore_cached_lookahead(checkpoint);
            return Err(error);
        }
        Ok(((self.time - start) as u32, checkpoint))
    }

    pub(crate) fn restore_cached_lookahead(&mut self, mut checkpoint: SoundCheckpoint) {
        // Reverse order preserves overlapping and aliased byte/word stores.
        let lost = checkpoint.undo.audio.len().min(checkpoint.output_len);
        let mut output = std::mem::take(&mut self.scsp.output);
        output.truncate(checkpoint.output_len - lost);
        for value in checkpoint.undo.audio[..lost].iter().rev() {
            output.push_front(*value);
        }
        checkpoint.state.scsp.output = output;
        for (index, value) in checkpoint.undo.bytes.into_iter().rev() {
            self.ram[index] = value;
        }
        checkpoint.state.ram = std::mem::take(&mut self.ram);
        #[cfg(test)]
        {
            let mut trace = std::mem::take(&mut self.memory.trace);
            trace.truncate(checkpoint.trace_len);
            checkpoint.state.memory.trace = trace;
        }
        *self = checkpoint.state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(program: &[u16], four_mbit: bool) -> Sound {
        let mut sound = Sound::default();
        sound.ram[..4].copy_from_slice(&0x7000u32.to_be_bytes());
        sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
        for (i, word) in program.iter().enumerate() {
            sound.ram[0x100 + i * 2..0x102 + i * 2].copy_from_slice(&word.to_be_bytes());
        }
        sound
            .scsp
            .write16(0x400, if four_mbit { 0x200 } else { 0 })
            .unwrap();
        sound.scsp.write16(0x780, 0x1000).unwrap();
        sound.scsp.write16(0x80C, 0xC000).unwrap(); // odd-step table MWT
        for at in [0x800, 0x2000, 0x20800] {
            sound.ram[at..at + 2].copy_from_slice(&0xBEEFu16.to_be_bytes());
        }
        sound.set_enabled(true).unwrap();
        sound
    }
    fn same(a: &mut Sound, b: &mut Sound) {
        assert_eq!(a.ram, b.ram);
        assert_eq!(a.cpu.state, b.cpu.state);
        assert_eq!(a.scsp.dsp, b.scsp.dsp);
        assert_eq!(
            (
                a.time,
                a.cpu_ticks,
                a.cpu_origin,
                a.next_cpu,
                a.first_cpu_request,
                a.ram_busy_until,
                a.scsp_tick,
                a.reset_output_start,
                a.reset_output_end
            ),
            (
                b.time,
                b.cpu_ticks,
                b.cpu_origin,
                b.next_cpu,
                b.first_cpu_request,
                b.ram_busy_until,
                b.scsp_tick,
                b.reset_output_start,
                b.reset_output_end
            )
        );
        assert_eq!(a.memory.trace, b.memory.trace);
        assert_eq!(
            (a.scsp.generated, a.scsp.dropped),
            (b.scsp.generated, b.scsp.dropped)
        );
        for at in (0..=0xEDE).step_by(2) {
            assert_eq!(a.scsp.read16(at), b.scsp.read16(at));
        }
        assert_eq!(a.scsp.drain(BUFFER_FRAMES), b.scsp.drain(BUFFER_FRAMES));
    }

    #[test]
    fn settled_lookahead_needs_no_repeated_zero_clock_advance() {
        let programs: &[&[u16]] = &[
            &[0x4E71, 0x60FC],
            &[0x33FC, 0x1234, 0, 0x800, 0x60F6],
            &[0x33FC, 0x0020, 0x0010, 0x042A, 0x60FE],
            &[0x4E70, 0x60FC],
            &[0x33FC, 0x8000, 0x0010, 0x0800, 0x60FE],
        ];
        let (mut skipped, mut retained) = (0, 0);
        for &program in programs {
            for phase in [0, 1, 3, 9, 17] {
                for clocks in [0, 1, 4, 13, 63, 509, 3000] {
                    let (mut a, mut b) = (make(program, true), make(program, true));
                    for sound in [&mut a, &mut b] {
                        sound.scsp.write16(0x406, 0x5A).unwrap();
                        sound.scsp.write16(0x418, 0xFE).unwrap();
                        sound.advance(phase).unwrap();
                    }
                    assert_eq!(a.cached_lookahead(clocks), b.cached_lookahead(clocks));
                    if a.cached_lookahead_settled() {
                        skipped += 1;
                    } else {
                        a.advance(0).unwrap();
                        retained += 1;
                    }
                    b.advance(0).unwrap();
                    same(&mut a, &mut b);
                    for delta in [0, 1, 13, 101] {
                        let actual = a.advance(delta);
                        assert_eq!(actual, b.advance(delta));
                        same(&mut a, &mut b);
                        if actual.is_err() {
                            break;
                        }
                    }
                }
            }
        }
        assert!(skipped > 0 && retained > 0);
    }

    #[test]
    fn rollback_restores_overlapping_cpu_bytes_dsp_stores_and_dram_geometry() {
        let program = [
            0x33FC, 0x1234, 0, 0x800, 0x13FC, 0x0056, 0, 0x801, 0x33FC, 0x789A, 2, 0x800, 0x60E6,
        ];
        for four_mbit in [false, true] {
            let (mut trial, mut reference) = (make(&program, four_mbit), make(&program, four_mbit));
            for phase in [0, 1, 9, 61, 509, 1123] {
                trial.advance(phase).unwrap();
                reference.advance(phase).unwrap();
                let (advanced, checkpoint) = trial.cached_lookahead_checkpoint(3000).unwrap();
                assert_eq!(advanced, 3000);
                assert!(!checkpoint.undo.bytes.is_empty());
                if phase == 0 {
                    assert_ne!(&trial.ram[0x800..0x802], &reference.ram[0x800..0x802]);
                    assert_ne!(&trial.ram[0x2000..0x2002], &reference.ram[0x2000..0x2002]);
                }
                trial.restore_cached_lookahead(checkpoint);
                same(&mut trial, &mut reference);
                for delta in [1, 11, 317] {
                    trial.advance(delta).unwrap();
                    reference.advance(delta).unwrap();
                    same(&mut trial, &mut reference);
                }
            }
        }
    }

    #[test]
    fn output_rollback_preserves_empty_partial_full_wrapped_and_overrun_queues() {
        for frames in [0, 1, BUFFER_FRAMES - 1, BUFFER_FRAMES] {
            for clocks in [1, 17_000, 5_100_000] {
                let mut sound = Sound::default();
                for i in 0..frames {
                    sound.scsp.output.push_back(i as i16);
                    sound.scsp.output.push_back(!(i as i16));
                }
                if frames != 0 {
                    // Exercise both physical slices of VecDeque as well.
                    sound.scsp.output.pop_front();
                    sound.scsp.output.push_back(-1234);
                }
                let output = sound.scsp.output.clone();
                let (generated, dropped) = (sound.scsp.generated, sound.scsp.dropped);
                let (_, checkpoint) = sound.cached_lookahead_checkpoint(clocks).unwrap();
                sound.restore_cached_lookahead(checkpoint);
                assert_eq!(sound.scsp.output, output, "frames={frames} clocks={clocks}");
                assert_eq!(
                    (sound.scsp.generated, sound.scsp.dropped),
                    (generated, dropped)
                );
                assert_eq!(sound.time, 0);
            }
        }
    }

    #[test]
    fn rollback_before_control_writes_reset_and_invalid_programs_preserves_future_work() {
        for program in [
            vec![0x33FC, 0x0020, 0x0010, 0x042A, 0x60FE],
            vec![0x4E70, 0x60FC],
            vec![0x33FC, 0x8000, 0x0010, 0x0800, 0x60FE],
            vec![0x33FC, 0x1002, 0x0010, 0x0416, 0x60FE],
        ] {
            let (mut trial, mut reference) = (make(&program, true), make(&program, true));
            let (advanced, checkpoint) = trial.cached_lookahead_checkpoint(3000).unwrap();
            assert!(advanced < 3000);
            trial.restore_cached_lookahead(checkpoint);
            same(&mut trial, &mut reference);
            for delta in [1, 13, 101, 509] {
                assert_eq!(trial.advance(delta), reference.advance(delta));
                same(&mut trial, &mut reference);
            }
        }
    }
}
