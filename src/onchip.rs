//! CPU-local SH7604 FRT, WDT, SCI, DIVU, DMAC and interrupt arbitration.
//! Original implementation from SH7604 §§5/9–13; no external emulator core.
use crate::bus::BusError;
use std::cell::{Cell, RefCell};
mod bsc;
mod divu;
mod dma;
mod sci;
mod ubc;
mod wdt;
use divu::Division;
use dma::Dma;
use sci::Serial;
pub use wdt::ResetRequest;
use wdt::Watchdog;

pub const SBYCR: u32 = 0xFFFF_FE91;
pub const SCI: u32 = 0xFFFF_FE00;
pub const WDT: u32 = 0xFFFF_FE80;
pub const ICR: u32 = 0xFFFF_FEE0;
pub const IPRA: u32 = 0xFFFF_FEE2;
pub const VCRA: u32 = 0xFFFF_FE62;
pub const VCRB: u32 = 0xFFFF_FE64;
pub const VCRWDT: u32 = 0xFFFF_FEE4;
pub const DIVU: u32 = 0xFFFF_FF00;
pub const DMAC: u32 = 0xFFFF_FF80;

pub const FRT: u32 = 0xFFFF_FE10;
pub const IPRB: u32 = 0xFFFF_FE60;
pub const VCRC: u32 = 0xFFFF_FE66;
pub const VCRD: u32 = 0xFFFF_FE68;

pub struct OnChip {
    ubc: RefCell<ubc::Ubc>,
    bsc: bsc::Bsc,
    module_stop: u8,
    standby: bool,
    wake_remaining: Option<u32>,
    division: RefCell<Division>,
    instruction_epoch: Cell<u64>,
    division_write_epoch: Cell<Option<u64>>,
    pub(super) dma: Dma,
    serial: Serial,
    watchdog: Watchdog,
    icr: u16,
    nmi_high: bool,
    ipra: u16,
    vcra: u16,
    vcrb: u16,
    vcrwdt: u16,
    wait_clocks: Cell<u32>,
    tier: u8,
    status: u8,
    // Reads of FTCSR must precede a software flag clear (§11.2.5).
    observed: Cell<u8>,
    counter: u16,
    compare: [u16; 2],
    capture: u16,
    temp: Cell<u8>,
    control: u8,
    output_control: u8,
    outputs: [bool; 2],
    divider: u32,
    iprb: u16,
    vcrc: u16,
    vcrd: u16,
}

impl Default for OnChip {
    fn default() -> Self {
        Self {
            ubc: RefCell::new(ubc::Ubc::default()),
            bsc: bsc::Bsc::default(),
            module_stop: 0,
            standby: false,
            wake_remaining: None,
            division: RefCell::new(Division::default()),
            instruction_epoch: Cell::new(0),
            division_write_epoch: Cell::new(None),
            dma: Dma::default(),
            serial: Serial::default(),
            watchdog: Watchdog::default(),
            icr: 0,
            nmi_high: true,
            ipra: 0,
            vcra: 0,
            vcrb: 0,
            vcrwdt: 0,
            wait_clocks: Cell::new(0),
            tier: 1,
            status: 0,
            observed: Cell::new(0),
            counter: 0,
            compare: [0xFFFF; 2],
            capture: 0,
            temp: Cell::new(0),
            control: 0,
            output_control: 0xE0,
            outputs: [false; 2],
            divider: 0,
            iprb: 0,
            vcrc: 0,
            vcrd: 0,
        }
    }
}

impl OnChip {
    pub(crate) fn external_bus_width(&self, physical_address: u32) -> usize {
        self.bsc.bus_width(physical_address)
    }
    pub(crate) fn external_little_endian(&self, physical_address: u32) -> bool {
        self.bsc.little_endian(physical_address)
    }
    pub(crate) fn shared_external_access(&self, physical_address: u32) -> bool {
        self.bsc.shared_access(physical_address)
    }
    pub(crate) fn release_external_bus(&self) {
        self.bsc.release_shared_bus();
    }

    pub(crate) fn external_access(
        &self,
        physical_address: u32,
        write: bool,
        first: bool,
        last: bool,
    ) -> u32 {
        let (base, waits) = self.bsc.access_timing(physical_address, write, first, last);
        self.wait_clocks
            .set(self.wait_clocks.get().saturating_add(waits));
        base
    }
    pub(crate) fn add_wait_clocks(&self, clocks: u32) {
        self.wait_clocks
            .set(self.wait_clocks.get().saturating_add(clocks));
    }
    pub(crate) fn pending_wait_clocks(&self) -> u32 {
        self.wait_clocks.get()
    }

    fn unsupported(address: u32, width: usize) -> BusError {
        if address == SBYCR
            || (FRT..FRT + 10).contains(&address)
            || (SCI..SCI + 6).contains(&address)
            || (WDT..WDT + 4).contains(&address)
            || (DIVU..DIVU + 0x18).contains(&address)
            || (DMAC..DMAC + 0x34).contains(&address)
            || matches!(
                address & !1,
                ICR | IPRA | IPRB | VCRA | VCRB | VCRC | VCRD | VCRWDT
            )
        {
            BusError::UnsupportedDevice { address, width }
        } else {
            BusError::Unmapped { address, width }
        }
    }

    fn selected(&self) -> usize {
        usize::from(self.output_control & 0x10 != 0)
    }

    fn intc(&self, address: u32) -> Option<(u16, u16)> {
        match address {
            ICR => Some((self.icr | u16::from(self.nmi_high) << 15, 0x0101)),
            IPRA => Some((self.ipra, 0xFFF0)),
            IPRB => Some((self.iprb, 0xFF00)),
            VCRA => Some((self.vcra, 0x7F7F)),
            VCRB => Some((self.vcrb, 0x7F7F)),
            VCRWDT => Some((self.vcrwdt, 0x7F7F)),
            VCRC => Some((self.vcrc, 0x7F7F)),
            VCRD => Some((self.vcrd, 0x7F00)),
            _ => None,
        }
    }

    pub fn read8(&self, address: u32) -> Result<u8, BusError> {
        if address == SBYCR {
            return Ok(self.module_stop);
        }
        if (SCI..SCI + 6).contains(&address) {
            return self
                .serial
                .read(address - SCI)
                .ok_or(Self::unsupported(address, 1));
        }
        if (WDT..WDT + 4).contains(&address) {
            return self
                .watchdog
                .read(address - WDT)
                .ok_or(Self::unsupported(address, 1));
        }
        if matches!(address, 0xFFFF_FE71 | 0xFFFF_FE72) {
            return Ok(self.dma.read_request((address - 0xFFFF_FE71) as usize));
        }
        if let Some((word, _)) = self.intc(address & !1) {
            return Ok(if address & 1 == 0 {
                (word >> 8) as u8
            } else {
                word as u8
            });
        }
        let high = |word: u16| {
            self.temp.set(word as u8);
            (word >> 8) as u8
        };
        Ok(match address.wrapping_sub(FRT) {
            0 => self.tier,
            1 => {
                self.observed.set(self.status & 0x8E);
                self.status
            }
            2 => high(self.counter),
            3 | 9 => self.temp.get(),
            4 => (self.compare[self.selected()] >> 8) as u8,
            5 => self.compare[self.selected()] as u8,
            6 => self.control,
            7 => self.output_control,
            8 => high(self.capture),
            _ => return Err(Self::unsupported(address, 1)),
        })
    }

    fn reset_frt(&mut self) {
        let initial = Self::default();
        self.tier = initial.tier;
        self.status = initial.status;
        self.observed.set(0);
        self.counter = initial.counter;
        self.compare = initial.compare;
        self.capture = initial.capture;
        self.temp.set(0);
        self.control = initial.control;
        self.output_control = initial.output_control;
        self.outputs = initial.outputs;
        self.divider = 0;
    }

    pub fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        if address == SBYCR {
            let rising = value & !self.module_stop;
            if rising & 0x10 != 0 && self.dma.active() {
                return Err(Self::unsupported(address, 1));
            }
            if rising & 1 != 0 {
                self.serial = Serial::default();
            }
            if rising & 2 != 0 {
                self.reset_frt();
            }
            // SBY cannot be set while the watchdog is running (§14.2.1).
            self.module_stop = value & 0xDF & if self.watchdog.enabled() { 0x7F } else { 0xFF };
            return Ok(());
        }
        if (SCI..SCI + 6).contains(&address) {
            return if self.serial.write(address - SCI, value) {
                Ok(())
            } else {
                Err(Self::unsupported(address, 1))
            };
        }
        if matches!(address, 0xFFFF_FE71 | 0xFFFF_FE72) {
            self.dma
                .write_request((address - 0xFFFF_FE71) as usize, value);
            return Ok(());
        }
        if self.intc(address & !1).is_some() {
            let (old, _) = self.intc(address & !1).unwrap();
            let new = if address & 1 == 0 {
                (old & 0xFF) | (u16::from(value) << 8)
            } else {
                (old & 0xFF00) | u16::from(value)
            };
            return self.write16(address & !1, new);
        }
        match address.wrapping_sub(FRT) {
            0 => self.tier = (value & 0x8E) | 1,
            1 => {
                self.status &= !(self.observed.get() & !value);
                self.status = (self.status & 0x8E) | (value & 1);
                self.observed.set(0);
            }
            2 | 4 => self.temp.set(value),
            3 => self.counter = u16::from_be_bytes([self.temp.get(), value]),
            5 => self.compare[self.selected()] = u16::from_be_bytes([self.temp.get(), value]),
            6 => self.control = value & 0x83,
            7 => self.output_control = (value & 0x13) | 0xE0,
            _ => return Err(Self::unsupported(address, 1)),
        }
        Ok(())
    }

    pub fn read16(&self, address: u32) -> Result<u16, BusError> {
        if address & 1 != 0 {
            return Err(BusError::Unaligned { address, width: 2 });
        }
        if address >= bsc::BSC {
            return self.bsc.read(address, 2).map(|v| v as u16);
        }
        if (ubc::BASE..ubc::BASE + 0x3C).contains(&address) {
            return self.ubc.borrow().read(address, 2).map(|v| v as u16);
        }
        if matches!(
            address,
            0xFFFF_FF08 | 0xFFFF_FF0A | 0xFFFF_FF0C | 0xFFFF_FF0E
        ) {
            let value = self.read32(address & !3)?;
            return Ok(if address & 2 == 0 {
                (value >> 16) as u16
            } else {
                value as u16
            });
        }
        self.intc(address)
            .map(|(v, _)| v)
            .ok_or(Self::unsupported(address, 2))
    }

    pub fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        if address & 1 != 0 {
            return Err(BusError::Unaligned { address, width: 2 });
        }
        if (ubc::BASE..ubc::BASE + 0x3C).contains(&address) {
            return self.ubc.get_mut().write(address, 2, u32::from(value));
        }
        if matches!(address, WDT | 0xFFFF_FE82) {
            self.watchdog.write(address - WDT, value);
            return Ok(());
        }
        if matches!(
            address,
            0xFFFF_FF08 | 0xFFFF_FF0A | 0xFFFF_FF0C | 0xFFFF_FF0E
        ) {
            // This is a write strobe, not an architectural read. In particular
            // do not invent the §10.4.1 read-after-write interlock while
            // preserving the other lane of a word-accessible register.
            let old = {
                let mut div = self.division.borrow_mut();
                if self.module_stop & 4 != 0 {
                    return Err(Self::unsupported(address, 2));
                }
                self.wait_clocks
                    .set(self.wait_clocks.get().saturating_add(div.remaining()));
                div.finish();
                div.read((address & !3) - DIVU).unwrap()
            };
            let new = if address & 2 == 0 {
                (old & 0xFFFF) | (u32::from(value) << 16)
            } else {
                (old & 0xFFFF_0000) | u32::from(value)
            };
            return self.write32(address & !3, new);
        }
        let (_, mask) = self.intc(address).ok_or(Self::unsupported(address, 2))?;
        let word = value & mask;
        match address {
            ICR => self.icr = word,
            IPRA => self.ipra = word,
            IPRB => self.iprb = word,
            VCRA => self.vcra = word,
            VCRB => self.vcrb = word,
            VCRWDT => self.vcrwdt = word,
            VCRC => self.vcrc = word,
            VCRD => self.vcrd = word,
            _ => unreachable!(),
        }
        Ok(())
    }

    /// Counts CPU phi clocks even while the CPU is in SLEEP.
    /// Pin synchronization, access wait states and clock-switch glitches are
    /// outside this functional model (SH7604 §§11.4/11.7).
    pub fn advance_clocks(&mut self, mut clocks: u32) {
        // Split before a reset or peripheral transition can suppress clocks
        // that preceded it. The same deadlines are exported to the board's
        // cross-clock scheduler, including while either processor sleeps.
        while clocks != 0 {
            let n = clocks.min(self.clocks_to_event());
            self.advance_segment(n);
            clocks -= n;
        }
    }
    fn advance_segment(&mut self, clocks: u32) {
        if self.standby {
            // The bus is released throughout standby/wake, independent of
            // stopped peripheral clocks (H §14.3).
            self.bsc.reset_bus();
            if let Some(remaining) = self.wake_remaining {
                if clocks < remaining {
                    self.wake_remaining = Some(remaining - clocks);
                } else {
                    self.wake_remaining = None;
                    self.standby = false;
                    self.advance_clocks(clocks - remaining);
                }
            }
            return;
        }
        let held_reset = self.watchdog.reset_active();
        self.watchdog.advance(clocks);
        if held_reset {
            self.bsc.advance_bus(clocks);
            return;
        }
        self.ubc.get_mut().advance(clocks);
        self.bsc.advance(clocks);
        if self.module_stop & 4 == 0 {
            self.division.get_mut().advance(clocks);
        }
        if self.module_stop & 1 == 0 {
            self.serial.advance(clocks);
        }
        if self.module_stop & 2 != 0 {
            return;
        }
        let divisor = match self.control & 3 {
            0 => 8,
            1 => 32,
            2 => 128,
            _ => return,
        };
        let clocks = u64::from(self.divider) + u64::from(clocks);
        self.divider = (clocks % divisor) as u32;
        for _ in 0..clocks / divisor {
            self.increment();
        }
    }

    fn increment(&mut self) {
        // Compare the value held BEFORE this count edge. The match flags,
        // outputs and CCLRA clear take effect on the edge leaving that value
        // (SH7604 §§11.4.2/11.4.3/11.4.6, figs. 11.6/11.7/11.11).
        let a = self.counter == self.compare[0];
        let b = self.counter == self.compare[1];
        if a {
            self.status |= 8;
            self.outputs[0] = self.output_control & 2 != 0;
        }
        if b {
            self.status |= 4;
            self.outputs[1] = self.output_control & 1 != 0;
        }
        // OVF describes FFFF -> 0000, not a clear from any other count
        // (§§11.2.5/11.4.7, fig. 11.12). Both compares use the old value.
        if self.counter == 0xFFFF {
            self.status |= 2;
        }
        self.counter = if a && self.status & 1 != 0 {
            0
        } else {
            self.counter.wrapping_add(1)
        };
    }

    /// Saturn's 16-bit communication strobe contains both edges, so either
    /// IEDG selection captures. The strobe data has no data-transfer meaning.
    pub(crate) fn capture_strobe(&mut self) {
        if self.standby || self.module_stop & 2 != 0 {
            return;
        }
        self.capture = self.counter;
        self.status |= 0x80;
    }

    /// Level-sensitive until guest software reads/clears FTCSR. Exception
    /// acceptance itself does not acknowledge the peripheral flag (§11.5).
    pub fn interrupt(&self) -> Option<(u8, u8)> {
        if self.standby {
            return None;
        }
        if self.ubc.borrow().pending() {
            return Some((15, 12));
        }
        // H table 5.4: earlier source wins when programmed levels are equal.
        let div = self.division.borrow();
        let frt = self.tier & self.status & 0x8E;
        let frt_vector = if frt & 0x80 != 0 {
            self.vcrc >> 8
        } else if frt & 0x0C != 0 {
            self.vcrc & 0x7F
        } else {
            self.vcrd >> 8
        };
        let sci_vector = self.serial.irq().map(|source| match source {
            0 => self.vcra >> 8,
            1 => self.vcra & 0x7F,
            2 => self.vcrb >> 8,
            _ => self.vcrb & 0x7F,
        });
        let sources = [
            (self.ipra >> 12, div.irq().then_some(div.vector as u16)),
            ((self.ipra >> 8) & 15, self.dma.interrupt().map(u16::from)),
            (
                (self.ipra >> 4) & 15,
                self.watchdog.irq().then_some(self.vcrwdt >> 8),
            ),
            (
                (self.ipra >> 4) & 15,
                self.bsc.interrupt().then_some(self.vcrwdt & 0x7F),
            ),
            (self.iprb >> 12, sci_vector),
            ((self.iprb >> 8) & 15, (frt != 0).then_some(frt_vector)),
        ];
        let mut winner = None;
        for (level, vector) in sources {
            if level != 0
                && let Some(vector) = vector
                && winner.is_none_or(|(old, _)| level as u8 > old)
            {
                winner = Some((level as u8, vector as u8));
            }
        }
        winner
    }

    pub fn read32(&self, address: u32) -> Result<u32, BusError> {
        if address & 3 != 0 {
            return Err(BusError::Unaligned { address, width: 4 });
        }
        if address >= bsc::BSC {
            return self.bsc.read(address, 4);
        }
        if (ubc::BASE..ubc::BASE + 0x3C).contains(&address) {
            return self.ubc.borrow().read(address, 4);
        }
        if (DIVU..DIVU + 0x18).contains(&address) {
            let mut div = self.division.borrow_mut();
            if self.module_stop & 4 != 0 && div.busy() {
                return Err(Self::unsupported(address, 4));
            }
            // H §10.4.1: an immediately following read instruction extends
            // one cycle after ANY DIVU register write. Independent instruction
            // issue, rather than elapsed clocks, determines this dependency.
            let consecutive = self
                .division_write_epoch
                .take()
                .is_some_and(|epoch| epoch.wrapping_add(1) == self.instruction_epoch.get());
            self.wait_clocks.set(
                self.wait_clocks
                    .get()
                    .saturating_add(div.remaining().max(u32::from(consecutive))),
            );
            div.finish();
            return div
                .read(address - DIVU)
                .ok_or(Self::unsupported(address, 4));
        }
        if (DMAC..DMAC + 0x34).contains(&address) {
            return self
                .dma
                .read(address - DMAC)
                .ok_or(Self::unsupported(address, 4));
        }
        Err(Self::unsupported(address, 4))
    }

    pub fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        if address & 3 != 0 {
            return Err(BusError::Unaligned { address, width: 4 });
        }
        if address >= bsc::BSC {
            return self.bsc.write(address, value);
        }
        if (ubc::BASE..ubc::BASE + 0x3C).contains(&address) {
            return self.ubc.get_mut().write(address, 4, value);
        }
        if (DIVU..DIVU + 0x18).contains(&address) {
            if self.module_stop & 4 != 0 {
                return Err(Self::unsupported(address, 4));
            }
            let div = self.division.get_mut();
            self.wait_clocks
                .set(self.wait_clocks.get().saturating_add(div.remaining()));
            if div.write(address - DIVU, value) {
                self.division_write_epoch
                    .set(Some(self.instruction_epoch.get()));
                return Ok(());
            }
        }
        if (DMAC..DMAC + 0x34).contains(&address) && self.dma.write(address - DMAC, value) {
            return Ok(());
        }
        Err(Self::unsupported(address, 4))
    }

    pub fn receive_serial(&mut self, byte: u8, errors: u8, multiprocessor: bool) {
        if !self.standby && self.module_stop & 1 == 0 {
            self.serial.receive(byte, errors, multiprocessor);
        }
    }
    pub fn take_serial_output(&mut self) -> Option<u8> {
        self.serial.take_transmitted()
    }
    pub fn set_dma_request_pin(&mut self, channel: usize, level: bool) {
        if channel < 2 {
            self.dma.set_request_pin(channel, level);
        }
    }
    pub(crate) fn dma_stopped(&self) -> bool {
        self.standby || self.module_stop & 0x10 != 0
    }
    pub(crate) fn serial_dma_requests(&self) -> [bool; 2] {
        [self.serial.dma_request(1), self.serial.dma_request(2)]
    }
    pub(crate) fn acknowledge_serial_dma(&mut self, source: u8) {
        self.serial.dma_acknowledge(source);
    }
    pub(crate) fn take_wait_clocks(&self) -> u32 {
        self.wait_clocks.replace(0)
    }
    pub(crate) fn reset_active(&self) -> bool {
        self.watchdog.reset_active()
    }
    pub(crate) fn take_reset_request(&mut self) -> Option<ResetRequest> {
        self.watchdog.request.take()
    }
    /// H §5.3.8: IRL auto-vector shares one vector for each pair of levels;
    /// VECMD=1 samples the board-supplied D7–D0 vector (only seven bits valid).
    pub(crate) fn external_vector(&self, level: u8, vector: u8) -> u8 {
        if self.icr & 1 == 0 {
            64 + level / 2
        } else {
            vector & 0x7F
        }
    }
    /// Board NMI electrical input. Repeated levels do not create new edges;
    /// NMIL is read-only and NMIE selects the edge independently of SR.I.
    pub fn set_nmi_pin(&mut self, high: bool) -> bool {
        let previous = std::mem::replace(&mut self.nmi_high, high);
        let request = previous != high && high == (self.icr & 0x100 != 0);
        if request {
            self.nmi_edge();
        }
        request
    }
    pub(crate) fn signal_nmi(&mut self) {
        // SMPC supplies a complete active-low pulse. Either NMIE selection
        // detects exactly one of its two edges, leaving the pin deasserted.
        self.set_nmi_pin(false);
        self.set_nmi_pin(true);
    }
    fn nmi_edge(&mut self) {
        if self.standby && self.wake_remaining.is_none() {
            self.wake_remaining = Some(self.watchdog.settling_clocks());
        }
        self.dma.nmi();
    }
    pub(crate) fn nmi_ready(&self) -> bool {
        !self.standby
    }
    pub(crate) fn clocks_to_event(&self) -> u32 {
        if self.standby {
            return self.wake_remaining.unwrap_or(u32::MAX);
        }
        let watchdog = self.watchdog.clocks_to_event();
        if self.watchdog.reset_active() {
            return watchdog;
        }
        let mut next = watchdog
            .min(self.ubc.borrow().clocks_to_event())
            .min(self.bsc.clocks_to_event());
        if self.module_stop & 4 == 0 && self.division.borrow().busy() {
            next = next.min(self.division.borrow().remaining());
        }
        if self.module_stop & 1 == 0 {
            next = next.min(self.serial.clocks_to_event());
        }
        if self.module_stop & 2 == 0 && self.control & 3 != 3 {
            let divisor = [8, 32, 128][(self.control & 3) as usize];
            for target in [self.compare[0], self.compare[1], 0xFFFF] {
                let ticks = u32::from(target.wrapping_sub(self.counter)) + 1;
                next = next.min((ticks * divisor).saturating_sub(self.divider).max(1));
            }
        }
        next
    }
    pub(crate) fn enter_sleep(&mut self) {
        if self.module_stop & 0x80 == 0 {
            return;
        }
        // H table 14.3: preserve INTC/UBC/BSC and the CPU, reset FRT/SCI
        // and DMA control. DIVU contents are undefined; retain operands under
        // the same deterministic policy used for other undefined reset data.
        self.reset_frt();
        self.serial = Serial::default();
        self.dma.standby();
        self.division.get_mut().reset();
        self.watchdog.enter_standby();
        self.standby = true;
        self.wake_remaining = None;
    }
    pub(crate) fn reset_from_watchdog(&mut self, reset: ResetRequest) {
        // DIVU operand/vector registers retain values (§10.2); RSTCSR retains
        // the watchdog cause (§12.2.3). Other implemented registers reset.
        let slave = self.bsc.read(bsc::BSC, 4).unwrap() & 0x8000 != 0;
        let mut bsc = std::mem::take(&mut self.bsc);
        if reset == ResetRequest::PowerOn {
            bsc = bsc::Bsc::default();
        }
        bsc.set_slave(slave);
        bsc.reset_bus();
        let mut division = std::mem::take(self.division.get_mut());
        division.reset();
        let mut watchdog = std::mem::take(&mut self.watchdog);
        watchdog.reset_preserving_cause();
        *self = Self {
            bsc,
            nmi_high: self.nmi_high,
            division: RefCell::new(division),
            watchdog,
            ..Self::default()
        };
    }

    pub(crate) fn set_slave(&mut self, slave: bool) {
        self.bsc.set_slave(slave);
    }
    pub(crate) fn instruction_break(&self, pc: u32, after: bool) -> bool {
        if self.standby {
            return false;
        }
        if !after {
            self.instruction_epoch
                .set(self.instruction_epoch.get().wrapping_add(1));
        }
        let mut ubc = self.ubc.borrow_mut();
        ubc.observe(pc, 2, 0, false, ubc::Master::Cpu, Some(after));
        ubc.pending()
    }
    pub(crate) fn acknowledge_break(&self) {
        self.ubc.borrow_mut().acknowledge();
    }
    pub(crate) fn observe_access(
        &self,
        address: u32,
        width: usize,
        value: u32,
        write: bool,
        cpu: bool,
        external: bool,
    ) {
        if self.standby || (external && !self.bsc.monitors_external_bus()) {
            return;
        }
        let master = if external {
            ubc::Master::External
        } else if cpu {
            ubc::Master::Cpu
        } else {
            ubc::Master::Dma
        };
        self.ubc
            .borrow_mut()
            .observe(address, width, value, write, master, None);
    }
    pub(crate) fn can_request_interrupt(&self) -> bool {
        if self.standby {
            return self.wake_remaining.is_some();
        }
        (self.ipra & 0xF0 != 0 && self.bsc.can_request_interrupt())
            || (self.iprb & 0x0F00 != 0 && self.tier & 0x8E != 0)
            || (self.ipra & 0x00F0 != 0 && self.watchdog.enabled())
            || self.watchdog.can_reset()
            || (self.iprb & 0xF000 != 0 && self.serial.can_wake())
            || (self.module_stop & 4 == 0 && self.division.borrow().busy())
            || (self.module_stop & 0x10 == 0 && self.dma.active())
            || self.interrupt().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_pair(chip: &mut OnChip, offset: u32, word: u16) {
        chip.write8(FRT + offset, (word >> 8) as u8).unwrap();
        chip.write8(FRT + offset + 1, word as u8).unwrap();
    }
    fn read_pair(chip: &OnChip, offset: u32) -> u16 {
        u16::from_be_bytes([
            chip.read8(FRT + offset).unwrap(),
            chip.read8(FRT + offset + 1).unwrap(),
        ])
    }

    #[test]
    fn reset_values_reserved_bits_widths_and_read_only_capture() {
        let mut chip = OnChip::default();
        assert_eq!(
            (chip.read8(FRT).unwrap(), chip.read8(FRT + 7).unwrap()),
            (1, 0xE0)
        );
        assert_eq!(read_pair(&chip, 2), 0);
        assert_eq!(read_pair(&chip, 4), 0xFFFF);
        assert_eq!(read_pair(&chip, 8), 0);
        assert!(chip.read16(FRT).is_err());
        assert!(chip.write16(FRT + 2, 0x1234).is_err());
        assert!(chip.write8(FRT + 8, 1).is_err());
        chip.write8(FRT, 0xFF).unwrap();
        chip.write8(FRT + 6, 0xFF).unwrap();
        chip.write8(FRT + 7, 0xFF).unwrap();
        assert_eq!(chip.read8(FRT).unwrap(), 0x8F);
        assert_eq!(chip.read8(FRT + 6).unwrap(), 0x83);
        assert_eq!(chip.read8(FRT + 7).unwrap(), 0xF3);
        chip.write16(VCRC, 0xFFFF).unwrap();
        chip.write16(VCRD, 0xFFFF).unwrap();
        assert_eq!(chip.read16(VCRC).unwrap(), 0x7F7F);
        assert_eq!(chip.read16(VCRD).unwrap(), 0x7F00);
        chip.write16(IPRB, 0xF000).unwrap(); // SCI priority is implemented.
        chip.write8(IPRB, 0x0B).unwrap();
        assert_eq!(chip.read16(IPRB).unwrap(), 0x0B00);
        assert!(matches!(
            chip.read16(IPRB + 1),
            Err(BusError::Unaligned { .. })
        ));
    }

    #[test]
    fn temp_register_latches_reads_and_commits_only_on_lower_byte_write() {
        let mut chip = OnChip::default();
        write_pair(&mut chip, 2, 0x12FF);
        assert_eq!(chip.read8(FRT + 2).unwrap(), 0x12);
        chip.advance_clocks(8);
        assert_eq!(chip.read8(FRT + 3).unwrap(), 0xFF); // Latch, not live 0x00.
        assert_eq!(read_pair(&chip, 2), 0x1300);
        chip.write8(FRT + 2, 0x56).unwrap();
        assert_eq!(chip.counter, 0x1300);
        chip.write8(FRT + 5, 0x78).unwrap(); // All pairs share TEMP.
        assert_eq!(read_pair(&chip, 4), 0x5678);
        chip.write8(FRT + 7, 0xF0).unwrap();
        assert_eq!(read_pair(&chip, 4), 0xFFFF); // OCRB independently reset.
        write_pair(&mut chip, 4, 0x2345);
        chip.write8(FRT + 7, 0xE0).unwrap();
        assert_eq!(read_pair(&chip, 4), 0x5678);
    }

    #[test]
    fn clock_divisors_split_advances_and_external_clock_selection() {
        for (select, divisor) in [(0, 8), (1, 32), (2, 128)] {
            let mut chip = OnChip::default();
            chip.write8(FRT + 6, select).unwrap();
            chip.advance_clocks(divisor - 1);
            assert_eq!(read_pair(&chip, 2), 0);
            chip.advance_clocks(1);
            assert_eq!(read_pair(&chip, 2), 1);
            chip.advance_clocks(divisor * 7);
            assert_eq!(read_pair(&chip, 2), 8);
        }
        let mut chip = OnChip::default();
        chip.write8(FRT + 6, 3).unwrap();
        chip.advance_clocks(10_000);
        assert_eq!(read_pair(&chip, 2), 0); // No external clock wire driven.
    }

    #[test]
    fn compare_flags_outputs_and_clear_use_the_value_before_the_count_edge() {
        let mut chip = OnChip::default();
        write_pair(&mut chip, 4, 3); // OCRA=3
        chip.write8(FRT + 7, 0xF3).unwrap();
        write_pair(&mut chip, 4, 2); // OCRB=2
        chip.write8(FRT + 1, 1).unwrap(); // CCLRA
        // SH7604 figs. 11.6/11.7/11.11: reaching N does not yet signal
        // a match; the next count edge signals it and optionally clears FRC.
        for (counter, status, outputs) in [
            (1, 1, [false, false]),
            (2, 1, [false, false]),
            (3, 5, [false, true]),
            (0, 13, [true, true]),
        ] {
            chip.advance_clocks(8);
            assert_eq!(
                (chip.counter, chip.status, chip.outputs),
                (counter, status, outputs)
            );
        }
        chip.read8(FRT + 1).unwrap();
        chip.write8(FRT + 1, 1).unwrap();
        chip.advance_clocks(24);
        assert_eq!((chip.counter, chip.status), (3, 5));
        chip.advance_clocks(8);
        assert_eq!((chip.counter, chip.status), (0, 13)); // Four-edge period.
    }

    #[test]
    fn compare_zero_and_overflow_boundary_vectors() {
        // Independently specified one-edge vectors from SH7604 §§11.2.5,
        // 11.4.3/11.4.6/11.4.7, figs. 11.7/11.11/11.12.
        for (initial, a, b, clear, counter, flags) in [
            (0, 0, 0, false, 1, 0x0C),
            (0, 0, 0, true, 0, 0x0C),
            (0xFFFE, 0xFFFF, 0xFFFF, false, 0xFFFF, 0),
            (0xFFFF, 0, 0, false, 0, 2),
            (0xFFFF, 0, 0, true, 0, 2),
            (0xFFFF, 0xFFFF, 0xFFFF, false, 0, 0x0E),
            (0xFFFF, 0xFFFF, 0xFFFF, true, 0, 0x0E),
            (3, 3, 0, true, 0, 8),    // Clearing to OCRB is not a B match.
            (3, 3, 3, true, 0, 0x0C), // A clear preserves simultaneous B.
        ] {
            let mut chip = OnChip::default();
            write_pair(&mut chip, 2, initial);
            write_pair(&mut chip, 4, a);
            chip.write8(FRT + 7, 0xF3).unwrap();
            write_pair(&mut chip, 4, b);
            chip.write8(FRT + 1, u8::from(clear)).unwrap();
            chip.advance_clocks(7);
            assert_eq!((chip.counter, chip.status), (initial, u8::from(clear)));
            chip.advance_clocks(1);
            assert_eq!(
                (chip.counter, chip.status, chip.outputs),
                (
                    counter,
                    flags | u8::from(clear),
                    [flags & 8 != 0, flags & 4 != 0]
                ),
                "FRC={initial:04X}, OCRA={a:04X}, OCRB={b:04X}, CCLRA={clear}"
            );
        }
        let mut chip = OnChip::default();
        write_pair(&mut chip, 4, 0);
        chip.write8(FRT + 1, 1).unwrap();
        for _ in 0..3 {
            chip.advance_clocks(8);
            assert_eq!((chip.counter, chip.read8(FRT + 1).unwrap()), (0, 9));
            chip.write8(FRT + 1, 1).unwrap(); // Compare=0 repeats every edge.
        }
    }

    #[test]
    fn read_then_zero_flag_acknowledgment_and_irq_priority_are_independent() {
        let mut chip = OnChip::default();
        chip.write8(FRT, 0x8F).unwrap();
        chip.write16(VCRC, 0x4546).unwrap();
        chip.write16(VCRD, 0x4700).unwrap();
        write_pair(&mut chip, 2, 0xFFFE);
        chip.advance_clocks(16); // Compare A/B at FFFF; overflow at 0000.
        chip.capture_strobe();
        assert_eq!(chip.interrupt(), None); // Priority zero masks all requests.
        chip.write16(IPRB, 0x0B00).unwrap();
        assert_eq!(chip.interrupt(), Some((11, 0x45)));
        chip.write8(FRT + 1, 0).unwrap(); // Without read, no flag may clear.
        assert_eq!(chip.interrupt(), Some((11, 0x45)));
        assert_eq!(chip.read8(FRT + 1).unwrap(), 0x8E);
        chip.write8(FRT + 1, 0x0E).unwrap(); // Clear capture only.
        assert_eq!(chip.interrupt(), Some((11, 0x46)));
        assert_eq!(chip.read8(FRT + 1).unwrap(), 0x0E);
        chip.write8(FRT + 1, 2).unwrap(); // Clear both compare flags.
        assert_eq!(chip.interrupt(), Some((11, 0x47)));
        chip.read8(FRT + 1).unwrap();
        chip.write8(FRT + 1, 0).unwrap();
        assert_eq!(chip.interrupt(), None);
        chip.write8(FRT + 1, 0x8E).unwrap(); // Software cannot set flags.
        assert_eq!(chip.read8(FRT + 1).unwrap(), 0);
    }
}

#[cfg(test)]
mod peripheral_tests;
