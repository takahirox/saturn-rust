//! SMPC ST-169 §§1.2, 2.2/2.4/2.5, 3.2/3.3. Original functional model.
//! RTC advances only with emulated clocks. Direct digital and standard analog
//! joystick inputs are electrical levels, never writes into guest game state.
use crate::bus::BusError;
use crate::devices::SMPC;
mod controller;
use controller::{Analog, DigitalMux, Handshake};
const REFERENCE_HZ: u64 = 26_874_100;

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
    fn advance(&mut self, clocks: u32) {
        self.phase += u64::from(clocks);
        while self.phase >= REFERENCE_HZ {
            self.phase -= REFERENCE_HZ;
            self.second += 1;
            if self.second < 60 {
                continue;
            }
            self.second = 0;
            self.minute += 1;
            if self.minute < 60 {
                continue;
            }
            self.minute = 0;
            self.hour += 1;
            if self.hour < 24 {
                continue;
            }
            self.hour = 0;
            self.weekday = (self.weekday + 1) % 7;
            self.day += 1;
            if self.day <= Self::days(self.year, self.month) {
                continue;
            }
            self.day = 1;
            self.month += 1;
            if self.month <= 12 {
                continue;
            }
            self.month = 1;
            self.year = (self.year + 1) % 10000;
        }
    }
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
    pub(crate) fn clocks_to_event(&self) -> u32 {
        self.digital
            .iter()
            .map(DigitalMux::clocks_to_event)
            .chain(self.handshake.iter().map(Handshake::clocks_to_event))
            .fold(self.pending.map_or(u32::MAX, |(_, n)| n), u32::min)
    }
    pub fn set_region(&mut self, region: Region) {
        self.region = region;
    }
    pub(crate) fn system_reset(&mut self) {
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
            ..Self::default()
        };
        self.output[31] = 0x0D;
    }
    pub(crate) fn reset_volatile(&mut self) {
        let region = self.region;
        let battery = self.battery.clone();
        *self = Self {
            region,
            battery,
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
                let memory = self.battery.memory;
                self.battery = Battery::parse(self.input).expect("validated SETTIME");
                self.battery.memory = memory;
            }
            0x17 => self.battery.memory.copy_from_slice(&self.input[..4]),
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
                    self.await_continue = self.input[1] & 8 != 0;
                    self.sf = self.await_continue;
                    self.status = 0x40 | u8::from(self.await_continue) << 5;
                }
                self.irq = true;
            }
            _ => (),
        }
        self.battery.advance(clocks - elapsed);
        Some(command)
    }
    /// External peripheral TH pin (port bit6). ST-169 §1.2/table1.5 routes
    /// a falling input edge to the PAD IRQ and VDP2 latch only with EXLE=1.
    pub(crate) fn set_th(&mut self, port: usize, high: bool) -> bool {
        if port >= 2 {
            return false;
        }
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
                self.handshake[port] = Handshake::default();
            }
        }
    }
    /// STN-43: analog 3D pad X/Y and independent right/left trigger levels.
    pub fn set_3d_pad(&mut self, port: usize, axes: Option<[u8; 4]>) {
        if port < 2 {
            self.analog[port] = axes.map(|a| Analog::pad(self.analog[port], a));
            if axes.is_none() {
                self.handshake[port] = Handshake::default();
            }
        }
    }
    fn end_peripherals(&mut self) {
        self.await_continue = false;
        self.await_vblank = false;
        self.peripheral_phase = false;
        self.pending = None;
        self.sf = false;
        self.status &= !0x20;
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
    fn unsupported(address: u32) -> BusError {
        BusError::UnsupportedDevice { address, width: 1 }
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
                    return Err(Self::unsupported(address));
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
            _ => Err(Self::unsupported(address)),
        }
    }
    pub fn write(&mut self, address: u32, value: u8) -> Result<(), BusError> {
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
                    return Err(Self::unsupported(address));
                }
                self.input[i] = value;
            }
            0x63 if value == 1 => self.sf = true,
            0x1F if self.sf && !self.busy() => {
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
                    _ => return Err(Self::unsupported(address)),
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
            _ => return Err(Self::unsupported(address)),
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
