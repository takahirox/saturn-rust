//! ST-077 §§3.1–3.2: buffered CPU requests share the internal DRAM allocator.
//! See docs/scsp-cpu-memory.md for eligibility and equal-edge conventions.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    pub address: usize,
    pub width: usize,
    pub write: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Owner {
    Pcm,
    Dsp,
    Refresh,
    Dma,
    Main,
    Cpu,
}
#[derive(Default)]
struct Port {
    request: Option<Request>,
    result: Option<u32>,
    offset: usize,
    sampled: u32,
}
struct Admitted {
    owner: Owner,
    request: Request,
    config: RamConfig,
}
#[derive(Default)]
pub(super) struct Controller {
    main: Port,
    cpu: Port,
    admitted: Option<Admitted>,
    #[cfg(test)]
    // boundary, owner, interval end (CPU/PCM/DMA: memory completion;
    // DSP: ownership release, preserving its existing instruction sampling).
    pub trace: Vec<(u64, Owner, bool)>,
}
impl Controller {
    fn port(&mut self, owner: Owner) -> &mut Port {
        match owner {
            Owner::Main => &mut self.main,
            Owner::Cpu => &mut self.cpu,
            _ => unreachable!(),
        }
    }
    pub fn cancel(&mut self, owner: Owner) {
        *self.port(owner) = Port::default();
        if self.admitted.as_ref().is_some_and(|a| a.owner == owner) {
            self.admitted = None;
        }
    }
    pub fn reset_main(&mut self) {
        // RESET drops the unfinished DRAM grant, while the external master
        // still holds its buffered request. Retain its completed halves and
        // re-admit only the unfinished word after release; never replay them.
        if self
            .admitted
            .as_ref()
            .is_some_and(|a| a.owner == Owner::Main)
        {
            self.admitted = None;
        }
    }
    pub fn pending(&self) -> bool {
        self.admitted.is_some()
            || [&self.main, &self.cpu]
                .iter()
                .any(|p| p.request.is_some() && p.result.is_none())
    }
    pub fn submit(&mut self, owner: Owner, request: Request) {
        let port = self.port(owner);
        if let Some(old) = port.request {
            assert_eq!(old, request, "request changed before acknowledgement");
        } else {
            port.request = Some(request);
        }
    }
    pub fn take(&mut self, owner: Owner) -> Option<u32> {
        let port = self.port(owner);
        let value = port.result.take()?;
        *port = Port::default();
        Some(value)
    }
    pub fn ready(&self, owner: Owner) -> bool {
        match owner {
            Owner::Main => self.main.result.is_some(),
            Owner::Cpu => self.cpu.result.is_some(),
            _ => false,
        }
    }
    pub fn complete(&mut self, ram: &mut [u8], tick: u64) {
        if let Some(a) = self.admitted.take() {
            let r = a.request;
            let result = if let Some(value) = r.write {
                if r.width == 1 {
                    a.config.write8(ram, r.address, value as u8);
                } else {
                    a.config.write16(ram, r.address, value as u16);
                }
                value
            } else if r.width == 1 {
                u32::from(a.config.read8(ram, r.address))
            } else {
                u32::from(a.config.read16(ram, r.address))
            };
            let port = self.port(a.owner);
            let original = port.request.expect("admitted request retained");
            port.offset += r.width;
            port.sampled = if port.offset == r.width {
                result
            } else {
                (port.sampled << (r.width * 8)) | result
            };
            if port.offset == original.width {
                port.result = Some(original.write.unwrap_or(port.sampled));
            }
            self.record(tick, a.owner, true);
        }
    }
    pub fn grant(&mut self, config: RamConfig, tick: u64) -> Option<Owner> {
        assert!(self.admitted.is_none());
        for owner in [Owner::Main, Owner::Cpu] {
            let port = self.port(owner);
            if port.result.is_none()
                && let Some(mut request) = port.request
            {
                if request.width == 4 {
                    request.address += port.offset;
                    request.width = 2;
                    request.write = request
                        .write
                        .map(|value| (value >> ((2 - port.offset) * 8)) & 65535);
                }
                self.admitted = Some(Admitted {
                    owner,
                    request,
                    config,
                });
                self.record(tick, owner, false);
                return Some(owner);
            }
        }
        None
    }
    pub fn record(&mut self, tick: u64, owner: Owner, completion: bool) {
        #[cfg(test)]
        self.trace.push((tick, owner, completion));
        #[cfg(not(test))]
        let _ = (tick, owner, completion);
    }
}

impl Sound {
    #[cfg(test)]
    pub(crate) fn memory_trace(&self) -> &[(u64, Owner, bool)] {
        &self.memory.trace
    }

    pub(crate) fn main_ram_request(&mut self, request: Option<Request>) {
        if let Some(request) = request {
            self.memory.submit(Owner::Main, request);
        } else {
            self.memory.cancel(Owner::Main);
        }
    }
    pub(crate) fn main_ram_ready(&self) -> bool {
        self.memory.ready(Owner::Main)
    }
    pub(crate) fn complete_main_ram(&mut self) -> Result<u32, BusError> {
        Ok(self
            .memory
            .take(Owner::Main)
            .expect("DRAM completion before interface acknowledgement"))
    }
}

#[cfg(test)]
mod tests;
