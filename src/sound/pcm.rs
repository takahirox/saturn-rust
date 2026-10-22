//! ST-077 §3.2 pp.20–21/fig.3.2: two distinct DRAM reads per slot.
//! Even memory cycles serve PCM; odd ones serve DSP. One slot spans four
//! cycles. Addresses/format/DRAM geometry are latched at each grant, data at
//! completion. This prevents later writes from replacing an earlier operand.
use super::*;

pub(super) struct Slot {
    pub slot: usize,
    pub regs: [u16; 12],
    pub attenuation: u32,
    pub lfo: synthesis::Lfo,
    pub addresses: [usize; 2],
    pub fraction: i64,
    pub samples: [i16; 2],
    pub fetch: bool,
}
pub(super) struct Word {
    operand: usize,
    address: usize,
    control: u16,
    config: RamConfig,
}
pub(super) fn decode(mut sample: i16, control: u16) -> i16 {
    if control & 0x200 != 0 {
        sample ^= 0x7FFF;
    }
    if control & 0x400 != 0 {
        sample ^= i16::MIN;
    }
    sample
}
impl Scsp {
    pub(super) fn begin_slot(&mut self, i: usize) {
        let regs = self.slots[i];
        self.stack.begin_slot(i);
        let lfo = self.lfo[i];
        self.lfo[i].advance(regs[9]);
        self.stack.write(i, 0, regs[6] & 0x200 != 0);
        if !self.voices[i].active {
            return;
        }
        let voice = &mut self.voices[i];
        let attenuation = voice.envelope.advance(
            regs[4],
            regs[5],
            regs[8],
            voice.phase >= (i64::from(regs[2]) << 16),
        );
        let Some(attenuation) = attenuation else {
            voice.active = false;
            return;
        };
        let start = (usize::from(regs[0] & 15) << 16) | usize::from(regs[1]);
        let lsa = i64::from(regs[2]) << 16;
        let lea = i64::from(regs[3]) << 16;
        if lea == 0 {
            voice.active = false;
            return;
        }
        let looping = (regs[0] >> 5) & 3;
        let span = lea - lsa;
        if looping != 0 && span <= 0 {
            voice.active = false;
            return;
        }
        // ST-077 figure 4.10: reverse mode skips from the attack's LSA
        // crossing to the loop end; alternative mode first traverses forward.
        if looping == 2 && !voice.reverse && voice.phase >= lsa {
            voice.phase = lea - 1 - (voice.phase - lsa);
            voice.reverse = true;
        }
        if voice.phase >= lea {
            match looping {
                0 => {
                    voice.active = false;
                    self.slots[i][0] &= !0x800;
                    return;
                }
                1 => voice.phase = lsa + (voice.phase - lsa).rem_euclid(span),
                3 => {
                    let n = (voice.phase - lea).rem_euclid(2 * span);
                    voice.reverse = n < span;
                    voice.phase = if voice.reverse {
                        lea - 1 - n
                    } else {
                        lsa + n - span
                    };
                }
                _ => unreachable!(),
            }
        }
        if voice.reverse && voice.phase < lsa {
            if looping == 2 {
                voice.phase = lea - 1 - (lsa - 1 - voice.phase).rem_euclid(span);
            } else {
                let n = (lsa - 1 - voice.phase).rem_euclid(2 * span);
                voice.reverse = n >= span;
                voice.phase = if voice.reverse {
                    lea - 1 - (n - span)
                } else {
                    lsa + n
                };
            }
        }
        let pcm8 = regs[0] & 16 != 0;
        // ST-077 §§2.2/3.1, fig.4.30 OP4: first-order interpolation
        // follows waveform reads. Interpolate decoded signed samples, not
        // addresses or mixed output. Q16 arithmetic rounds towards -infinity.
        let shift = self.stack.displacement(i, regs[7]);
        let position = voice.phase + shift;
        let index = position >> 16;
        // CA counts samples for both PCM widths. The first interpolation
        // address is the current output position, including FM displacement;
        // neither SA nor the pitch step for the next sample is part of CA.
        voice.call_address = (index as u16 >> 12) & 15;
        // A reverse phase names the upper edge of its sample interval.
        // Measure interpolation distance from that edge toward the previous
        // sample, wrapping only reverse loops and holding ping-pong ends.
        let fraction = if voice.reverse {
            0xFFFF - (position & 0xFFFF)
        } else {
            position & 0xFFFF
        };
        let next = if shift != 0 {
            index + if voice.reverse { -1 } else { 1 }
        } else if voice.reverse {
            if index <= i64::from(regs[2]) {
                if looping == 2 {
                    i64::from(regs[3]) - 1
                } else {
                    index
                }
            } else {
                index - 1
            }
        } else if index + 1 >= i64::from(regs[3]) {
            if looping == 1 {
                i64::from(regs[2])
            } else {
                index
            }
        } else {
            index + 1
        };
        let addresses = [index, next].map(|sample| {
            (start as i64 + sample * if pcm8 { 1 } else { 2 }).rem_euclid(0x10_0000) as usize
        });
        let source = (regs[0] >> 7) & 3;
        let mut samples = [if source == 1 { lfo.noise() } else { 0 }; 2];
        if source != 0 {
            samples = samples.map(|value| decode(value, regs[0]));
        }
        self.pcm_slot = Some(pcm::Slot {
            slot: i,
            regs,
            attenuation,
            lfo,
            addresses,
            fraction,
            samples,
            // SDIR bypasses the attenuation path (ST-077 p.72); otherwise
            // EG=3FF suppresses both reads, independently of output routing.
            fetch: source == 0 && (regs[6] & 0x100 != 0 || attenuation < 1023 << 16),
        });
        let oct = ((regs[8] >> 11) & 15) as i8;
        let oct = if oct >= 8 { oct - 16 } else { oct };
        let mut step = i64::from(1024 + (regs[8] & 1023)) << 6;
        if oct >= 0 {
            step <<= oct;
        } else {
            step >>= -oct;
        }
        step = lfo.step(regs[9], step);
        voice.phase += if voice.reverse { -step } else { step };
    }
    fn end_slot(&mut self) {
        let Some(slot) = self.pcm_slot.take() else {
            return;
        };
        let pcm::Slot {
            slot: i,
            regs,
            attenuation,
            lfo,
            fraction,
            samples: [a, b],
            ..
        } = slot;
        let mut value =
            ((i64::from(a) * (65536 - fraction) + i64::from(b) * fraction) >> 16) as i16;
        if regs[6] & 0x100 == 0 {
            value = synthesis::enveloped(value, attenuation);
            value = synthesis::attenuate(value, regs[6] as u8, lfo.attenuation(regs[9]));
        }
        self.stack.write(i, value, regs[6] & 0x200 != 0);
        self.dsp.add_slot(regs[10], value);
        mix_sample(
            &mut self.frame_mix,
            value,
            (regs[11] >> 13) & 7,
            (regs[11] >> 8) & 31,
        );
    }

    fn complete_pcm(&mut self, ram: &[u8]) {
        if let Some(word) = self.pcm_word.take() {
            let sample = if word.control & 16 != 0 {
                i16::from(word.config.read8(ram, word.address) as i8) << 8
            } else {
                word.config.read16(ram, word.address & !1) as i16
            };
            self.pcm_slot
                .as_mut()
                .expect("PCM grant retains its slot")
                .samples[word.operand] = decode(sample, word.control);
        }
    }
    pub(super) fn grant_pcm(&mut self, step: usize) -> bool {
        if step & 1 != 0 {
            return false;
        }
        let Some(slot) = &self.pcm_slot else {
            return false;
        };
        if !slot.fetch {
            return false;
        }
        let operand = (step & 3) / 2;
        self.pcm_word = Some(Word {
            operand,
            address: slot.addresses[operand],
            control: slot.regs[0],
            config: self.ram_config(),
        });
        true
    }
    /// Completion precedes the next grant. In particular DMA carried across
    /// the frame edge publishes before the first PCM address is accepted.
    pub(super) fn sample_cycle(&mut self, step: usize, ram: &mut [u8]) -> Result<bool, BusError> {
        self.complete_pcm(ram);
        self.complete_dma(ram)?;
        if step == 0 {
            self.begin_sample();
        }
        if self.pcm_clocked && step & 3 == 0 {
            self.begin_slot(step / 4);
        }
        let busy = self.memory_cycle(step)?;
        if self.pcm_clocked && step & 3 == 3 {
            self.end_slot();
        }
        self.dsp_step(step, ram)?;
        if step == 127 {
            self.end_sample();
        }
        Ok(busy)
    }
}

#[cfg(test)]
mod tests;
