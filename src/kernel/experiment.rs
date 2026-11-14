//! Original fixtures and a bounded architectural interpreter comparison.
//! The reference has immediate RAM access, so this compares dispatch cost, not
//! equivalent Saturn timing or the production Machine's throughput.
use super::*;
use crate::{
    bus::{BusError, Memory},
    cpu::resumable::reference_from_state,
};

struct Ram(Vec<u8>);
impl Ram {
    fn read(&self, address: u32, width: usize) -> u32 {
        let offset = address.wrapping_sub(RAM_BASE) as usize;
        self.0[offset..offset + width]
            .iter()
            .fold(0, |value, byte| value << 8 | u32::from(*byte))
    }
    fn write(&mut self, address: u32, width: usize, value: u32) {
        let offset = address.wrapping_sub(RAM_BASE) as usize;
        self.0[offset..offset + width].copy_from_slice(&value.to_be_bytes()[4 - width..]);
    }
}
impl Memory for Ram {
    fn read8(&self, a: u32) -> Result<u8, BusError> {
        Ok(self.read(a, 1) as u8)
    }
    fn read16(&self, a: u32) -> Result<u16, BusError> {
        Ok(self.read(a, 2) as u16)
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        Ok(self.read(a, 4))
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), BusError> {
        self.write(a, 1, u32::from(v));
        Ok(())
    }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), BusError> {
        self.write(a, 2, u32::from(v));
        Ok(())
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
        self.write(a, 4, v);
        Ok(())
    }
}

pub struct Experiment {
    kernel: Kernel,
    reference: [Cpu; 2],
    memory: Ram,
}
impl Experiment {
    pub fn new(memory: bool, latency: u32) -> Self {
        let states = std::array::from_fn(|owner| {
            let mut state = Cpu::start(
                RAM_BASE + owner as u32 * 0x100,
                RAM_BASE + 0xF000 + owner as u32 * 0x100,
            )
            .state()
            .clone();
            state.r[3] = 1;
            state.r[4] = RAM_BASE + 0x2000 + owner as u32 * 0x1000;
            state
        });
        let reference = states.clone().map(reference_from_state);
        let mut kernel = Kernel::new(states, latency);
        let words: [u16; 6] = if memory {
            [0x6042, 0x7001, 0x2402, 0x310C, 0xAFFA, 0x0009]
        } else {
            [0x7001, 0x310C, 0x6203, 0x4300, 0xAFFA, 0x0009]
        };
        for owner in 0..2 {
            for (i, word) in words.iter().enumerate() {
                kernel
                    .initialize(RAM_BASE + owner * 0x100 + i as u32 * 2, 2, u32::from(*word))
                    .unwrap();
            }
        }
        let memory = Ram(kernel.ram().to_vec());
        Self {
            kernel,
            reference,
            memory,
        }
    }
    /// Retire exactly `count` more instructions on each CPU, then drain.
    pub fn run_kernel(&mut self, count: u32) -> Result<(), Error> {
        self.kernel.run_instructions(u64::from(count))
    }
    /// Run the same instruction counts through the ordinary immediate-RAM ISA interpreter.
    pub fn run_reference(&mut self, count: u32) -> Result<(), String> {
        for cpu in &mut self.reference {
            for _ in 0..count {
                cpu.step(&mut self.memory).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    pub fn verify(&self) -> Result<(), String> {
        for (actual, expected) in self.kernel.states().into_iter().zip(&self.reference) {
            if actual != expected.state() {
                return Err(format!(
                    "architectural state mismatch: {actual:?} != {:?}",
                    expected.state()
                ));
            }
        }
        if self.kernel.ram() != self.memory.0 {
            return Err("RAM mismatch".to_string());
        }
        Ok(())
    }
    /// Small deterministic endpoint for native/Worker comparisons; outside timing.
    pub fn digest(&self) -> Vec<u32> {
        let mut result = Vec::new();
        for s in self.kernel.states() {
            result.extend(s.r);
            result.extend([
                s.pc,
                s.sr,
                s.pr,
                s.gbr,
                s.vbr,
                s.mach,
                s.macl,
                s.instructions as u32,
                (s.instructions >> 32) as u32,
                s.delay_target.unwrap_or(0),
            ]);
        }
        result.push(self.kernel.ram().iter().fold(2166136261u32, |hash, byte| {
            (hash ^ u32::from(*byte)).wrapping_mul(16777619)
        }));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drained_benchmark_matches_reference_for_both_orders_and_waits() {
        for memory in [false, true] {
            for latency in [0, 1, 7] {
                for reverse in [false, true] {
                    let mut e = Experiment::new(memory, latency);
                    for count in [1, 2, 3, 7, 1000] {
                        if reverse {
                            e.run_reference(count).unwrap();
                            e.run_kernel(count).unwrap();
                        } else {
                            e.run_kernel(count).unwrap();
                            e.run_reference(count).unwrap();
                        }
                        e.verify().unwrap();
                    }
                }
            }
        }
    }
}
