//! Preserve the already aged physical-bus clock during deferred device catch-up.
use super::*;

pub(crate) struct KernelBusClock {
    access: Option<Access>,
    #[cfg(debug_assertions)]
    rows: [Option<u32>; 2],
}

impl Bsc {
    pub(crate) fn kernel_bus_clock(&self) -> KernelBusClock {
        KernelBusClock {
            access: self.access.get(),
            #[cfg(debug_assertions)]
            rows: self.rows.get(),
        }
    }
    pub(crate) fn restore_kernel_bus_clock(&self, clock: KernelBusClock) {
        #[cfg(debug_assertions)]
        assert_eq!(
            self.rows.get(),
            clock.rows,
            "WRAM segment crossed a BSC row event"
        );
        self.access.set(clock.access);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_device_catchup_keeps_literal_read_write_gap() {
        for elapsed in [0, 1, 2, 3] {
            let mut bsc = Bsc::default();
            bsc.write(BSC + 8, 0xA55A0000).unwrap();
            assert_eq!(bsc.access_timing(0x06001000, 4, false, true, true), (2, 0));
            bsc.advance_bus(elapsed);
            let physical_clock = bsc.kernel_bus_clock();
            bsc.advance(40); // Other BSC counters still advance during catch-up.
            bsc.restore_kernel_bus_clock(physical_clock);
            bsc.advance_bus(2u32.saturating_sub(elapsed));
            // H §7.4: a completed read needs one empty clock before a write.
            // T1/T2 are two clocks; an already elapsed gap pays that clock.
            assert_eq!(
                bsc.access_timing(0x06001004, 4, true, true, true),
                (2, u32::from(elapsed <= 2))
            );
        }
    }
}
