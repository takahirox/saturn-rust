//! Deferred SCU data phases. Preparing a cycle never reads or writes a device.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) struct DmaCycle {
    pub channel: usize,
    pub address: u32,
    pub width: usize,
    pub value: Option<u32>,
}
struct Probe {
    cycle: Cell<DmaCycle>,
}
impl Probe {
    fn access(&self, address: u32, width: usize, value: Option<u32>) -> Result<u32, BusError> {
        self.cycle.set(DmaCycle {
            address,
            width,
            value,
            ..self.cycle.get()
        });
        Err(BusError::Pending)
    }
}
impl Memory for Probe {
    fn read8(&self, a: u32) -> Result<u8, BusError> {
        self.access(a, 1, None).map(|v| v as u8)
    }
    fn read16(&self, a: u32) -> Result<u16, BusError> {
        self.access(a, 2, None).map(|v| v as u16)
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        self.access(a, 4, None)
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), BusError> {
        self.access(a, 1, Some(u32::from(v))).map(|_| ())
    }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), BusError> {
        self.access(a, 2, Some(u32::from(v))).map(|_| ())
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
        self.access(a, 4, Some(v)).map(|_| ())
    }
}
struct Completed {
    cycle: DmaCycle,
    value: Result<u32, BusError>,
    consumed: Cell<bool>,
}
impl Completed {
    fn access(&self, a: u32, width: usize, write: bool) -> Result<u32, BusError> {
        assert!(
            !self.consumed.replace(true),
            "SCU beat issued more than one data cycle"
        );
        assert_eq!(
            (a, width, write),
            (
                self.cycle.address,
                self.cycle.width,
                self.cycle.value.is_some()
            ),
            "SCU cycle changed while granted"
        );
        self.value
    }
}
impl Memory for Completed {
    fn read8(&self, a: u32) -> Result<u8, BusError> {
        self.access(a, 1, false).map(|v| v as u8)
    }
    fn read16(&self, a: u32) -> Result<u16, BusError> {
        self.access(a, 2, false).map(|v| v as u16)
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        self.access(a, 4, false)
    }
    fn write8(&mut self, a: u32, _: u8) -> Result<(), BusError> {
        self.access(a, 1, true).map(|_| ())
    }
    fn write16(&mut self, a: u32, _: u16) -> Result<(), BusError> {
        self.access(a, 2, true).map(|_| ())
    }
    fn write32(&mut self, a: u32, _: u32) -> Result<(), BusError> {
        self.access(a, 4, true).map(|_| ())
    }
}
impl Scu {
    pub(crate) fn prepare_dma_cycle(&mut self) -> Option<DmaCycle> {
        let channel = (0..3)
            .rev()
            .find(|i| self.channels[*i].pending())
            .or_else(|| self.dsp.dma_pending().then_some(3))?;
        let mut probe = Probe {
            cycle: Cell::new(DmaCycle {
                channel,
                address: 0,
                width: 0,
                value: None,
            }),
        };
        if channel < 3 {
            let _ = self.channels[channel].clone().beat(&mut probe, channel);
        } else {
            self.dsp.grant_dma_cycle();
            let _ = self.dsp.clone().dma_beat(&mut probe);
        }
        Some(probe.cycle.get())
    }
    pub(crate) fn complete_dma_cycle(
        &mut self,
        cycle: DmaCycle,
        value: Result<u32, BusError>,
    ) -> Result<(), ScuError> {
        let mut memory = Completed {
            cycle,
            value,
            consumed: Cell::new(false),
        };
        if cycle.channel < 3 {
            match self.channels[cycle.channel].beat(&mut memory, cycle.channel) {
                Ok(true) => self.signal(11 - cycle.channel as u8),
                Ok(false) => (),
                Err(error) => {
                    let indirect = self.channels[cycle.channel].indirect();
                    self.channels[cycle.channel].cancel();
                    if indirect {
                        return Err(error);
                    }
                    self.signal(12);
                }
            }
        } else {
            self.dsp.dma_beat(&mut memory)?;
        }
        Ok(())
    }
}
