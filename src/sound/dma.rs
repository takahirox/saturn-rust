//! ST-077 §3.2/fig.3.2: 128 shared DRAM cycles/Fs, two refresh cycles,
//! PCM/DSP > refresh > DMA > main CPU > sound CPU. §4.2/fig.4.66/tables4.39–40:
//! a DMA word moves between sound memory and an internal control register.
//! Accepted words retain their address/direction until the cycle completes.
use super::*;

pub(super) struct Word {
    address: usize,
    register: u32,
    gate: bool,
    to_memory: bool,
    config: RamConfig,
}
impl Scsp {
    pub(super) fn dma_active(&self) -> bool {
        self.dma_remaining != 0 || self.dma_word.is_some()
    }
    pub(super) fn complete_dma(&mut self, ram: &mut [u8]) -> Result<(), BusError> {
        if let Some(word) = self.dma_word.take() {
            let value = if word.gate {
                0
            } else if word.to_memory {
                self.read16(word.register)?
            } else {
                word.config.read16(ram, word.address)
            };
            if word.to_memory {
                word.config.write16(ram, word.address, value);
            } else {
                self.write16(word.register, value)?;
            }
            let next = (word.address + 2) & 0xF_FFFF;
            self.common[9] = next as u16;
            self.common[10] = ((next >> 4) as u16 & 0xF000) | ((word.register as u16 + 2) & 0xFFE);
            self.dma_remaining = self.dma_remaining.saturating_sub(2);
            if self.dma_remaining == 0 {
                self.common[11] &= !0x1000;
                self.request(16);
            }
        }
        Ok(())
    }
    pub(super) fn memory_cycle(&mut self, step: usize) -> Result<bool, BusError> {
        self.memory_owner = None;
        if self.reset_held {
            return Ok(false);
        }
        if matches!(step, 124 | 126) {
            self.refresh_pending = self.refresh_pending.saturating_add(1);
        }
        if self.grant_pcm(step) {
            self.memory_owner = Some(memory::Owner::Pcm);
            return Ok(true);
        }
        if self.frame_active && self.dsp.memory_access(step) {
            self.memory_owner = Some(memory::Owner::Dsp);
            return Ok(true);
        }
        // Two refresh requests/Fs. PCM/DSP may defer them; retain a
        // request across frame edges rather than stealing an active PCM read.
        if self.refresh_pending != 0 {
            self.memory_owner = Some(memory::Owner::Refresh);
            self.refresh_pending -= 1;
            return Ok(true);
        }
        if self.dma_remaining == 0 {
            self.common[11] &= !0x1000;
            return Ok(false);
        }
        let register = u32::from(self.common[10] & 0xFFE);
        if (0x412..=0x416).contains(&register) {
            return Err(Self::error(register));
        }
        self.memory_owner = Some(memory::Owner::Dma);
        self.dma_word = Some(Word {
            address: (usize::from(self.common[10] & 0xF000) << 4) | usize::from(self.common[9]),
            register,
            gate: self.common[11] & 0x4000 != 0,
            to_memory: self.common[11] & 0x2000 != 0,
            config: self.ram_config(),
        });
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
