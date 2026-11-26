//! SMPC ST-169 §§1.2, 2.2/2.4/2.5, 3.2/3.3. Original functional model.
//! RTC advances only with emulated clocks. Direct digital and standard analog
//! joystick inputs are electrical levels, never writes into guest game state.
use crate::bus::{BusError, Processor};
use crate::devices::SMPC;
mod controller;
pub mod diagnostics;
use controller::{Analog, DigitalMux, Handshake};
use diagnostics::Rejection;
const REFERENCE_HZ: u64 = 26_874_100;
pub const BATTERY_SNAPSHOT_SIZE: usize = 16;

/// SMPC ST-169 §2.4.2 OREG9: physical console area straps. Region is
/// independent of mounted media and survives CPU/system resets. Television
/// timing is a separate board property, not inferred from the disc region.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Region {
    #[default]
    Japan = 1,
    AsiaNtsc = 2,
    NorthAmerica = 4,
    CentralSouthAmericaNtsc = 5,
    Korea = 6,
    AsiaPal = 10,
    EuropePal = 12,
    CentralSouthAmericaPal = 13,
}
impl TryFrom<u8> for Region {
    type Error = &'static str;
    fn try_from(code: u8) -> Result<Self, Self::Error> {
        Ok(match code {
            1 => Self::Japan,
            2 => Self::AsiaNtsc,
            4 => Self::NorthAmerica,
            5 => Self::CentralSouthAmericaNtsc,
            6 => Self::Korea,
            10 => Self::AsiaPal,
            12 => Self::EuropePal,
            13 => Self::CentralSouthAmericaPal,
            _ => return Err("Reserved or prohibited SMPC area code"),
        })
    }
}

#[derive(Clone)]
struct Battery {
    // Binary calendar internally; the interface uses the manual's BCD layout.
    year: u16,
    month: u8,
    day: u8,
    weekday: u8,
    hour: u8,
    minute: u8,
    second: u8,
    phase: u64,
    set: bool,
    memory: [u8; 4],
    revision: u32,
}
impl Default for Battery {
    fn default() -> Self {
        Self {
            year: 2000,
            month: 1,
            day: 1,
            weekday: 6,
            hour: 0,
            minute: 0,
            second: 0,
            phase: 0,
            set: false,
            memory: [0; 4],
            revision: 0,
        }
    }
}
impl Battery {
    fn bcd(v: u8) -> u8 {
        v / 10 * 16 + v % 10
    }
    fn binary(v: u8) -> Option<u8> {
        if v & 15 > 9 || v >> 4 > 9 {
            None
        } else {
            Some((v >> 4) * 10 + (v & 15))
        }
    }
    fn days(year: u16, month: u8) -> u8 {
        match month {
            4 | 6 | 9 | 11 => 30,
            2 => {
                if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
                {
                    29
                } else {
                    28
                }
            }
            _ => 31,
        }
    }
    fn packet(&self) -> [u8; 7] {
        [
            Self::bcd((self.year / 100) as u8),
            Self::bcd((self.year % 100) as u8),
            self.weekday << 4 | self.month,
            Self::bcd(self.day),
            Self::bcd(self.hour),
            Self::bcd(self.minute),
            Self::bcd(self.second),
        ]
    }
    fn parse(bytes: [u8; 7]) -> Option<Self> {
        let year = u16::from(Self::binary(bytes[0])?) * 100 + u16::from(Self::binary(bytes[1])?);
        let month = bytes[2] & 15;
        let weekday = bytes[2] >> 4;
        let day = Self::binary(bytes[3])?;
        let hour = Self::binary(bytes[4])?;
        let minute = Self::binary(bytes[5])?;
        let second = Self::binary(bytes[6])?;
        if month == 0
            || month > 12
            || weekday > 6
            || day == 0
            || day > Self::days(year, month)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return None;
        }
        Some(Self {
            year,
            month,
            weekday,
            day,
            hour,
            minute,
            second,
            set: true,
            ..Self::default()
        })
    }

    fn snapshot(&self) -> [u8; BATTERY_SNAPSHOT_SIZE] {
        let mut bytes = [0; BATTERY_SNAPSHOT_SIZE];
        bytes[..7].copy_from_slice(&self.packet());
        bytes[7] = u8::from(self.set);
        bytes[8..12].copy_from_slice(&self.memory);
        bytes[12..].copy_from_slice(&(self.phase as u32).to_be_bytes());
        bytes
    }

    fn from_snapshot(bytes: &[u8], elapsed_ms: u64) -> Result<Self, &'static str> {
        const INVALID: &str = "Invalid SMPC battery snapshot";
        let bytes: &[u8; BATTERY_SNAPSHOT_SIZE] = bytes.try_into().map_err(|_| INVALID)?;
        let mut battery = Self::parse(bytes[..7].try_into().unwrap()).ok_or(INVALID)?;
        battery.phase = u64::from(u32::from_be_bytes(bytes[12..].try_into().unwrap()));
        if bytes[7] > 1 || battery.phase >= REFERENCE_HZ {
            return Err(INVALID);
        }
        battery.set = bytes[7] != 0;
        battery.memory.copy_from_slice(&bytes[8..12]);
        battery.advance_seconds(elapsed_ms / 1000);
        battery.advance(((elapsed_ms % 1000) * REFERENCE_HZ / 1000) as u32);
        Ok(battery)
    }

    fn advance(&mut self, clocks: u32) {
        self.phase += u64::from(clocks);
        if self.phase >= REFERENCE_HZ {
            self.advance_seconds(self.phase / REFERENCE_HZ);
            self.phase %= REFERENCE_HZ;
        }
    }

    fn advance_seconds(&mut self, seconds: u64) {
        // A complete 10,000-year calendar has a whole number of weeks.
        const CALENDAR_SECONDS: u64 = 3_652_425 * 86_400;
        let day_seconds =
            u64::from(self.hour) * 3600 + u64::from(self.minute) * 60 + u64::from(self.second);
        let total = day_seconds + seconds % CALENDAR_SECONDS;
        self.second = (total % 60) as u8;
        self.minute = (total / 60 % 60) as u8;
        self.hour = (total / 3600 % 24) as u8;
        let mut days = total / 86_400;
        self.weekday = (self.weekday + (days % 7) as u8) % 7;
        loop {
            let remaining = u64::from(Self::days(self.year, self.month) - self.day + 1);
            if days < remaining {
                self.day += days as u8;
                break;
            }
            days -= remaining;
            self.day = 1;
            if self.month == 12 {
                self.month = 1;
                self.year = (self.year + 1) % 10000;
            } else {
                self.month += 1;
            }
        }
    }
}

#[derive(Clone, Copy)]
struct QuietInterval {
    deadline: u32,
    elapsed: u32,
}

pub struct Smpc {
    buttons: [u16; 2],
    analog: [Option<Analog>; 2],
    handshake: [Handshake; 2],
    digital: [DigitalMux; 2],
    pdr: [u8; 2],
    ddr: [u8; 2],
    iosel: u8,
    exle: u8,
    th: [bool; 2],
    sf: bool,
    command: u8,
    pending: Option<(u8, u32)>,
    quiet_interval: Option<QuietInterval>,
    #[cfg(any(test, feature = "kernel-experiment"))]
    pub(crate) quiet_interval_enabled: bool,
    #[cfg(any(test, feature = "kernel-experiment"))]
    pub(crate) quiet_interval_steps: u64,
    // One parameterless category-B command behind INTBACK status only.
    // Explicit compatibility mailbox; ST-169 prohibits software double issue
    // but does not specify the electrical outcome. See bios-startup.md.
    deferred_control: Option<u8>,
    input: [u8; 7],
    output: [u8; 32],
    status: u8,
    battery: Battery,
    reset_disabled: bool,
    reset_button: bool,
    latched_reset_button: bool,
    sound_enabled: bool,
    cd_enabled: bool,
    region: Region,
    await_continue: bool,
    await_vblank: bool,
    peripheral_phase: bool,
    irq: bool,
    measured_collection: bool,
    clock_352: bool,
}
impl Default for Smpc {
    fn default() -> Self {
        Self {
            buttons: [0; 2],
            analog: [None; 2],
            handshake: std::array::from_fn(|_| Handshake::default()),
            digital: std::array::from_fn(|_| DigitalMux::default()),
            pdr: [0; 2],
            ddr: [0; 2],
            iosel: 0,
            exle: 0,
            th: [true; 2],
            sf: false,
            command: 0,
            pending: None,
            quiet_interval: None,
            #[cfg(any(test, feature = "kernel-experiment"))]
            quiet_interval_enabled: false,
            #[cfg(any(test, feature = "kernel-experiment"))]
            quiet_interval_steps: 0,
            deferred_control: None,
            input: [0; 7],
            output: [0; 32],
            status: 0,
            battery: Battery::default(),
            reset_disabled: true,
            reset_button: false,
            latched_reset_button: false,
            sound_enabled: false,
            cd_enabled: true,
            region: Region::default(),
            await_continue: false,
            await_vblank: false,
            peripheral_phase: false,
            irq: false,
            measured_collection: false,
            clock_352: false,
        }
    }
}
impl Smpc {
    // ST-169 §2.3: 30 µs at the fixed NTSC 320 reference clock, rounded up.
    pub const CPU_COMMAND_CLOCKS: u32 = 807;
    // ST-169 §2.3.8: 100 ms + oscillator settling. The functional model
    // completes at 100 ms of reference time; no host wall clock is involved.
    pub const SYSTEM_RESET_CLOCKS: u32 = 2_687_410;
    const STATUS_CLOCKS: u32 = 8600; // §2.4: 320 µs status collection upper bound.
    pub(crate) fn busy(&self) -> bool {
        self.pending.is_some() || self.await_continue || self.await_vblank
    }
    #[cfg(any(test, feature = "sound-worker"))]
    fn controller_only(&self) -> bool {
        self.deferred_control.is_none() && self.pending.is_none_or(|(command, _)| command == 0x10)
    }
    /// Controller transactions have no Sound/CD/clock/reset output. Keep all
    /// pending or deferred controls on the original conservative deadline.
    #[cfg(any(test, feature = "sound-worker"))]
    pub(crate) fn sound_controller_deadline(&self) -> u32 {
        if self.controller_only() {
            u32::MAX
        } else {
            let deadline = self.clocks_to_event();
            if deadline == u32::MAX { 1 } else { deadline }
        }
    }
    #[cfg(any(test, feature = "sound-worker"))]
    pub(crate) fn sound_independent_controller_write(&self, address: u32, value: u32) -> bool {
        if !self.controller_only() {
            return false;
        }
        match address - SMPC {
            offset @ 1..=0x0D if offset & 1 != 0 => true,
            0x63 => value == 1,
            0x1F => value == 0x10,
            0x75 | 0x77 | 0x79 | 0x7B | 0x7D | 0x7F => true,
            _ => false,
        }
    }
    pub(crate) fn clocks_to_event(&self) -> u32 {
        if let Some(interval) = self.quiet_interval {
            if interval.deadline == u32::MAX {
                return u32::MAX;
            }
            return interval.deadline - interval.elapsed;
        }
        self.digital
            .iter()
            .map(DigitalMux::clocks_to_event)
            .chain(self.handshake.iter().map(Handshake::clocks_to_event))
            .fold(self.pending.map_or(u32::MAX, |(_, n)| n), u32::min)
    }
    pub(crate) fn clocks_to_cpu_reset(&self) -> Option<u32> {
        let elapsed = self.quiet_interval.map_or(0, |interval| interval.elapsed);
        self.pending
            .filter(|&(command, _)| matches!(command, 0x03 | 0x0D..=0x0F))
            .map(|(_, clocks)| clocks - elapsed)
    }
    pub fn set_region(&mut self, region: Region) {
        self.region = region;
    }
    pub(crate) fn battery_revision(&self) -> u32 {
        self.battery.revision
    }

    pub(crate) fn export_battery(&self) -> [u8; BATTERY_SNAPSHOT_SIZE] {
        // Any deferred clocks are strictly before the next SMPC edge.
        let mut battery = self.battery.clone();
        if let Some(interval) = self.quiet_interval {
            battery.advance(interval.elapsed);
        }
        battery.snapshot()
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn advance_battery_snapshot(
        bytes: &[u8],
        elapsed_ms: u64,
    ) -> Result<[u8; BATTERY_SNAPSHOT_SIZE], &'static str> {
        Ok(Battery::from_snapshot(bytes, elapsed_ms)?.snapshot())
    }

    pub(crate) fn restore_battery(
        &mut self,
        bytes: &[u8],
        elapsed_ms: u64,
    ) -> Result<(), &'static str> {
        let mut battery = Battery::from_snapshot(bytes, elapsed_ms)?;
        self.settle_quiet_interval();
        battery.revision = self.battery.revision.wrapping_add(1);
        self.battery = battery;
        Ok(())
    }

    pub(crate) fn system_reset(&mut self) {
        self.settle_quiet_interval();
        let region = self.region;
        let battery = self.battery.clone();
        let buttons = self.buttons;
        let analog = self.analog;
        let th = self.th;
        *self = Self {
            region,
            battery,
            buttons,
            analog,
            th,
            #[cfg(any(test, feature = "kernel-experiment"))]
            quiet_interval_enabled: self.quiet_interval_enabled,
            #[cfg(any(test, feature = "kernel-experiment"))]
            quiet_interval_steps: self.quiet_interval_steps,
            ..Self::default()
        };
        self.output[31] = 0x0D;
    }
    pub(crate) fn reset_volatile(&mut self) {
        self.settle_quiet_interval();
        let region = self.region;
        let battery = self.battery.clone();
        *self = Self {
            region,
            battery,
            #[cfg(any(test, feature = "kernel-experiment"))]
            quiet_interval_enabled: self.quiet_interval_enabled,
            #[cfg(any(test, feature = "kernel-experiment"))]
            quiet_interval_steps: self.quiet_interval_steps,
            ..Self::default()
        };
    }
    pub fn set_reset_button(&mut self, pressed: bool) -> bool {
        let request = pressed && !self.reset_button && !self.reset_disabled;
        self.reset_button = pressed;
        request
    }
    pub(crate) fn vblank_in(&mut self) {
        self.latched_reset_button = self.reset_button;
        if self.peripheral_phase && self.pending.is_some_and(|(command, _)| command == 0x10) {
            self.settle_quiet_interval();
            // ST-169 §3.2, termination condition 2: collection cannot spill
            // past VBlank-in and later publish a stale packet. Re-measure
            // collection next time after a timeout (§3.3).
            self.end_peripherals();
            self.measured_collection = false;
        }
    }
    pub(crate) fn take_irq(&mut self) -> bool {
        std::mem::take(&mut self.irq)
    }
    #[cfg(test)]
    pub(crate) fn vblank_out(&mut self) {
        let active = if self.clock_352 {
            (224u64 * 1820 * REFERENCE_HZ).div_ceil(28_636_400) as u32
        } else {
            224 * crate::timing::CLOCKS_PER_LINE
        };
        self.vblank_out_after(active);
    }
    pub(crate) fn vblank_out_after(&mut self, active_clocks: u32) {
        if self.await_vblank {
            self.settle_quiet_interval();
            self.await_vblank = false;
            // ST-169 §3.2: first measure collection; optimization then places
            // readiness one millisecond before the selected VBlank-in deadline.
            let clocks = if self.measured_collection && self.input[1] & 2 == 0 {
                active_clocks.saturating_sub(26_875)
            } else {
                Self::STATUS_CLOCKS
            };
            self.pending = Some((0x10, clocks));
        }
    }
    pub(crate) fn advance_clocks(&mut self, clocks: u32) -> Option<u8> {
        if let Some(interval) = &mut self.quiet_interval {
            if clocks < interval.deadline - interval.elapsed {
                interval.elapsed += clocks;
                #[cfg(any(test, feature = "kernel-experiment"))]
                {
                    self.quiet_interval_steps += u64::from(clocks != 0);
                }
                return None;
            }
            self.settle_quiet_interval();
        } else if self.quiet_interval_enabled() && clocks != 0 {
            let deadline = self.clocks_to_event();
            if clocks < deadline {
                // Before any controller or command edge, only private clocks
                // and RTC phase change. Guest packet bytes and pins are held.
                self.quiet_interval = Some(QuietInterval {
                    deadline,
                    elapsed: clocks,
                });
                return None;
            }
        }
        self.advance_exact(clocks)
    }

    pub(crate) fn quiet_interval_enabled(&self) -> bool {
        #[cfg(any(test, feature = "kernel-experiment"))]
        {
            self.quiet_interval_enabled
        }
        #[cfg(not(any(test, feature = "kernel-experiment")))]
        {
            true
        }
    }

    #[cfg(any(test, all(target_arch = "wasm32", feature = "kernel-experiment")))]
    pub(crate) fn set_quiet_interval_enabled(&mut self, enabled: bool) {
        self.settle_quiet_interval();
        self.quiet_interval_enabled = enabled;
    }

    fn settle_quiet_interval(&mut self) {
        if let Some(interval) = self.quiet_interval.take() {
            let command = self.advance_exact(interval.elapsed);
            debug_assert!(command.is_none());
        }
    }

    fn advance_exact(&mut self, clocks: u32) -> Option<u8> {
        for port in &mut self.digital {
            port.advance(clocks);
        }
        for port in &mut self.handshake {
            port.advance(clocks);
        }
        let elapsed = self
            .pending
            .map_or(clocks, |(_, remaining)| remaining.min(clocks));
        self.battery.advance(elapsed);
        let (command, remaining) = self.pending?;
        if clocks < remaining {
            self.pending = Some((command, remaining - clocks));
            return None;
        }
        self.pending = None;
        self.sf = false;
        match command {
            6 => self.sound_enabled = true,
            7 => self.sound_enabled = false,
            8 => self.cd_enabled = true,
            9 => self.cd_enabled = false,
            0x0E | 0x0F => {
                self.clock_352 = command == 0x0E;
                self.sound_enabled = false;
            }
            0x19 => self.reset_disabled = false,
            0x1A => self.reset_disabled = true,
            0x16 => {
                // Parameters are latched at issue, protected from writes while busy.
                let mut updated = Battery::parse(self.input).expect("validated SETTIME");
                updated.memory = self.battery.memory;
                updated.revision = self.battery.revision.wrapping_add(u32::from(
                    !self.battery.set
                        || self.battery.packet() != self.input
                        || self.battery.phase != 0,
                ));
                self.battery = updated;
            }
            0x17 if self.battery.memory != self.input[..4] => {
                self.battery.memory.copy_from_slice(&self.input[..4]);
                self.battery.revision = self.battery.revision.wrapping_add(1);
            }
            0x10 => {
                if self.peripheral_phase {
                    self.peripherals();
                    self.measured_collection = true;
                    self.status = 0xC0 | ((self.input[1] >> 4) & 15);
                    self.peripheral_phase = false;
                } else {
                    self.output = [0; 32];
                    self.output[0] =
                        u8::from(self.battery.set) << 7 | u8::from(self.reset_disabled) << 6;
                    self.output[1..8].copy_from_slice(&self.battery.packet());
                    self.output[9] = self.region as u8;
                    self.output[10] =
                        0x34 | u8::from(self.clock_352) << 6 | u8::from(!self.sound_enabled);
                    self.output[11] = u8::from(!self.cd_enabled) << 6;
                    self.output[12..16].copy_from_slice(&self.battery.memory);
                    // ST-169 §2.4 OREG31: status results include the
                    // INTBACK command code. Clearing the result packet must
                    // not erase the command accepted at issue time (§2.3).
                    self.output[31] = 0x10;
                    self.await_continue = self.input[1] & 8 != 0;
                    self.sf = self.await_continue;
                    self.status = 0x40 | u8::from(self.await_continue) << 5;
                }
                self.irq = true;
            }
            _ => (),
        }
        if command == 0x10 {
            // Publish the real status/packet and IRQ first.
            self.start_deferred_control();
        }
        self.battery.advance(clocks - elapsed);
        Some(command)
    }
    /// Abandon any unused peripheral continuation and start the queued
    /// control command with its full duration.
    fn start_deferred_control(&mut self) {
        if let Some(control) = self.deferred_control.take() {
            self.await_continue = false;
            self.await_vblank = false;
            self.peripheral_phase = false;
            self.status &= !0x20;
            self.command = control;
            self.output[31] = control;
            self.sf = true;
            self.pending = Some((control, Self::control_clocks(control).unwrap()));
        }
    }
    fn control_clocks(command: u8) -> Option<u32> {
        match command {
            2 | 3 | 6 | 7 | 0x19 | 0x1A => Some(Self::CPU_COMMAND_CLOCKS),
            8 | 9 => Some(1075),
            _ => None,
        }
    }
    /// External peripheral TH pin (port bit6). ST-169 §1.2/table1.5 routes
    /// a falling input edge to the PAD IRQ and VDP2 latch only with EXLE=1.
    pub(crate) fn set_th(&mut self, port: usize, high: bool) -> bool {
        if port >= 2 {
            return false;
        }
        self.settle_quiet_interval();
        let previous = std::mem::replace(&mut self.th[port], high);
        self.digital[port].select(self.port_select(port));
        previous && !high && self.exle & (1 << port) != 0 && self.ddr[port] & 0x40 == 0
    }
    pub fn set_buttons(&mut self, buttons: u16) {
        self.buttons[0] = buttons & 0x1FFF;
    }
    pub fn set_port2_buttons(&mut self, buttons: u16) {
        self.buttons[1] = buttons & 0x1FFF;
    }
    /// ST-169 table 3.19: standard analog joystick, ID15, absolute X/Y/Z.
    pub fn set_analog(&mut self, port: usize, axes: Option<[u8; 3]>) {
        if port < 2 {
            self.analog[port] = axes.map(Analog::Joystick);
            if axes.is_none() {
                self.settle_quiet_interval();
                self.handshake[port] = Handshake::default();
            }
        }
    }
    /// STN-43: analog 3D pad X/Y and independent right/left trigger levels.
    pub fn set_3d_pad(&mut self, port: usize, axes: Option<[u8; 4]>) {
        if port < 2 {
            self.analog[port] = axes.map(|a| Analog::pad(self.analog[port], a));
            if axes.is_none() {
                self.settle_quiet_interval();
                self.handshake[port] = Handshake::default();
            }
        }
    }
    /// Digital shoulder overlay, independent of the physical trigger latches.
    pub fn set_3d_pad_digital(&mut self, port: usize, buttons: u16) {
        if let Some(Some(Analog::Pad {
            digital_shoulders, ..
        })) = self.analog.get_mut(port)
        {
            *digital_shoulders = buttons & 0x1800;
        }
    }
    fn end_peripherals(&mut self) {
        self.await_continue = false;
        self.await_vblank = false;
        self.peripheral_phase = false;
        self.pending = None;
        self.sf = false;
        self.status &= !0x20;
        self.start_deferred_control();
    }
    fn peripherals(&mut self) {
        self.output = [0xFF; 32];
        let mut offset = 0;
        for port in 0..2 {
            if (self.input[1] >> (4 + port * 2)) & 3 == 3 {
                // ST-169 fig3.11/table3.4: zero-byte mode removes the
                // entire port record; it does not report an unplugged port.
                continue;
            }
            self.output[offset] = 0xF1;
            offset += 1;
            if let Some(analog) = self.analog[port] {
                let (bytes, count) = analog.report(self.buttons[port]);
                self.output[offset..offset + count].copy_from_slice(&bytes[..count]);
                offset += count;
            } else {
                self.output[offset] = 2;
                self.output[offset + 1..offset + 3]
                    .copy_from_slice(&controller::digital(self.buttons[port]));
                offset += 3;
            }
        }
        self.output[31] = 0x10;
    }
    fn port_select(&self, port: usize) -> u8 {
        (self.pdr[port] & self.ddr[port])
            | (!self.ddr[port] & 0x7F & !(0x40 * u8::from(!self.th[port])))
    }
    pub fn read(&self, address: u32) -> Result<u8, BusError> {
        match address - SMPC {
            offset @ 0x21..=0x5F if offset & 1 != 0 => {
                Ok(self.output[((offset - 0x21) / 2) as usize])
            }
            0x61 => Ok((self.status & !16) | u8::from(self.latched_reset_button) << 4),
            0x63 => Ok(u8::from(self.sf)),
            offset @ (0x75 | 0x77) => {
                let port = ((offset - 0x75) / 2) as usize;
                let bit = 1 << port;
                if self.iosel & bit == 0 {
                    return Err(self.access_fault(
                        address,
                        1,
                        None,
                        Rejection::UnsupportedRegister,
                    ));
                }
                // ST-169 §1.2: input pins and output latches share PDR; DDR
                // controls each bit independently, including TH used by EXLE.
                let select = self.port_select(port);
                let pins = if self.analog[port].is_some() {
                    (select & 0x60) | self.handshake[port].pins()
                } else {
                    let nibble = self.digital[port].nibble(self.buttons[port]);
                    (select & 0x60) | 0x10 | nibble
                };
                Ok((self.pdr[port] & self.ddr[port]) | (pins & !self.ddr[port]))
            }
            offset @ 1..=0x0D if offset & 1 != 0 => {
                Err(self.access_fault(address, 1, None, Rejection::WriteOnly))
            }
            0x1F | 0x79 | 0x7B | 0x7D | 0x7F => {
                Err(self.access_fault(address, 1, None, Rejection::WriteOnly))
            }
            _ => Err(self.access_fault(address, 1, None, Rejection::UnsupportedRegister)),
        }
    }
    /// Host/device tests use the master command interface. CPU bus callers
    /// must retain their issuer through `write_from`.
    pub fn write(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        self.write_from(Processor::Master, address, value)
    }
    pub(crate) fn write_from(
        &mut self,
        issuer: Processor,
        address: u32,
        value: u8,
    ) -> Result<(), BusError> {
        self.settle_quiet_interval();
        match address - SMPC {
            offset @ 1..=0x0D if offset & 1 != 0 => {
                let i = ((offset - 1) / 2) as usize;
                if i == 0
                    && self.command == 0x10
                    && self.busy()
                    && (self.await_continue || self.peripheral_phase)
                {
                    if value & 0x40 != 0 {
                        // ST-169 §3.2/table3.2: BR ends acquisition also
                        // while waiting for VBlank or collecting a packet.
                        self.end_peripherals();
                    } else if self.await_continue && (value ^ self.input[0]) & 0x80 != 0 {
                        self.await_continue = false;
                        self.await_vblank = true;
                        self.peripheral_phase = true;
                    }
                } else if self.busy() {
                    return Err(self.access_fault(
                        address,
                        1,
                        Some(u32::from(value)),
                        Rejection::Busy,
                    ));
                }
                self.input[i] = value;
            }
            0x63 if value == 1 => self.sf = true,
            0x63 => {
                return Err(self.access_fault(
                    address,
                    1,
                    Some(u32::from(value)),
                    Rejection::InvalidSf,
                ));
            }
            // ST-169 §2.2/table2.5 (p.20): these five commands are
            // master-only. Fail before command/report/deadline mutation.
            // This bounded host diagnostic does not assert an electrical
            // response or an SH-2 architectural exception for prohibited use.
            0x1F if issuer == Processor::Slave && matches!(value, 2 | 3 | 0x0E | 0x0F | 0x10) => {
                return Err(self.access_fault(
                    address,
                    1,
                    Some(u32::from(value)),
                    Rejection::ProhibitedIssuer,
                ));
            }
            // One control command may overlap any INTBACK phase; firmware
            // reboot paths issue RESDISA without first ending a peripheral
            // continuation. It runs after the in-flight packet, BREAK or the
            // VBlank-in timeout. A continuation not yet requested is abandoned.
            0x1F if self.command == 0x10
                && self.busy()
                && self.deferred_control.is_none()
                && Self::control_clocks(value).is_some() =>
            {
                self.deferred_control = Some(value);
                if self.pending.is_none() && self.await_continue {
                    self.start_deferred_control();
                }
            }
            0x1F if self.busy() => {
                return Err(self.access_fault(address, 1, Some(u32::from(value)), Rejection::Busy));
            }
            // ST-169 documents SF-before-COMREG for software synchronization.
            // Idle SF=0 acceptance is an emulator compatibility policy (#109),
            // not measured electrical behavior. SF is independent of internal
            // work: never set it implicitly here. Only the bounded INTBACK mailbox above
            // accepts overlap; other busy writes retain diagnostics.
            0x1F => {
                let clocks = match value {
                    0 | 2 | 3 | 6 | 7 | 0x18 | 0x19 | 0x1A => Self::CPU_COMMAND_CLOCKS,
                    8 | 9 => 1075, // ST-169 §§2.3.6/2.3.7: 40 microseconds.
                    0x0D..=0x0F => Self::SYSTEM_RESET_CLOCKS,
                    0x16 if Battery::parse(self.input).is_some() => 1882,
                    0x17 => 1075,
                    0x10 if matches!(self.input[0], 0 | 1)
                        && self.input[2] == 0xF0
                        && (self.input[1] >> 4) & 3 != 2
                        && (self.input[1] >> 6) & 3 != 2
                        && (self.input[0] == 1 || self.input[1] & 8 != 0) =>
                    {
                        Self::STATUS_CLOCKS
                    }
                    _ => {
                        return Err(self.access_fault(
                            address,
                            1,
                            Some(u32::from(value)),
                            if matches!(value, 0x10 | 0x16) {
                                Rejection::InvalidParameters
                            } else {
                                Rejection::UnsupportedCommand
                            },
                        ));
                    }
                };
                self.command = value;
                self.output[31] = value;
                if value == 0x10 && self.input[0] == 0 {
                    self.peripheral_phase = true;
                    self.await_vblank = true;
                } else {
                    self.pending = Some((value, clocks));
                }
            }
            offset @ (0x75 | 0x77) => {
                let port = ((offset - 0x75) / 2) as usize;
                if let Some(analog) = self.analog[port]
                    && self.ddr[port] == 0x60
                    && self.iosel & (1 << port) != 0
                    && self.exle & (1 << port) == 0
                {
                    self.handshake[port].write(self.pdr[port], value, analog, self.buttons[port]);
                }
                self.pdr[port] = value & 0x7F;
            }
            0x79 => self.ddr[0] = value & 0x7F,
            0x7B => self.ddr[1] = value & 0x7F,
            0x7D => self.iosel = value & 3,
            0x7F => self.exle = value & 3,
            offset @ 0x21..=0x61 if offset & 1 != 0 => {
                return Err(self.access_fault(
                    address,
                    1,
                    Some(u32::from(value)),
                    Rejection::ReadOnly,
                ));
            }
            _ => {
                return Err(self.access_fault(
                    address,
                    1,
                    Some(u32::from(value)),
                    Rejection::UnsupportedRegister,
                ));
            }
        }
        for port in 0..2 {
            self.digital[port].select(self.port_select(port));
            if self.ddr[port] != 0x60
                || self.iosel & (1 << port) == 0
                || self.exle & (1 << port) != 0
            {
                // Disconnecting the direct three-wire route cancels its
                // pending electrical response, independently for each port.
                self.handshake[port] = Handshake::default();
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
