//! SMPC ST-169 §§1.2, 2.2/2.4/2.5, 3.2/3.3. Original functional model.
//! RTC advances only with emulated clocks. Direct digital and standard analog
//! joystick inputs are electrical levels, never writes into guest game state.
use crate::bus::BusError;
use crate::devices::SMPC;
mod controller;
use controller::{Analog, Handshake};
const REFERENCE_HZ: u64 = 26_874_100;

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
    pdr: [u8; 2],
    ddr: [u8; 2],
    iosel: u8,
    exle: u8,
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
    await_continue: bool,
    await_vblank: bool,
    peripheral_phase: bool,
    irq: bool,
    measured_collection: bool,
}
impl Default for Smpc {
    fn default() -> Self {
        Self {
            buttons: [0; 2],
            analog: [None; 2],
            handshake: std::array::from_fn(|_| Handshake::default()),
            pdr: [0; 2],
            ddr: [0; 2],
            iosel: 0,
            exle: 0,
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
            await_continue: false,
            await_vblank: false,
            peripheral_phase: false,
            irq: false,
            measured_collection: false,
        }
    }
}
impl Smpc {
    // ST-169 §2.3: 30 µs at the fixed NTSC 320 reference clock, rounded up.
    pub const CPU_COMMAND_CLOCKS: u32 = 807;
    const STATUS_CLOCKS: u32 = 8600; // §2.4: 320 µs status collection upper bound.
    pub(crate) fn busy(&self) -> bool {
        self.pending.is_some() || self.await_continue || self.await_vblank
    }
    pub(crate) fn reset_volatile(&mut self) {
        let battery = self.battery.clone();
        *self = Self {
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
    }
    pub(crate) fn take_irq(&mut self) -> bool {
        std::mem::take(&mut self.irq)
    }
    pub(crate) fn vblank_out(&mut self) {
        if self.await_vblank {
            self.await_vblank = false;
            // ST-169 §3.2: first measure collection; optimization then places
            // readiness one millisecond before VBlank-in in NTSC 320 mode.
            let clocks = if self.measured_collection && self.input[1] & 2 == 0 {
                224 * crate::timing::CLOCKS_PER_LINE - 26_875
            } else {
                Self::STATUS_CLOCKS
            };
            self.pending = Some((0x10, clocks));
        }
    }
    pub(crate) fn advance_clocks(&mut self, clocks: u32) -> Option<u8> {
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
                    self.output[9] = 1; // Japan region, no cartridge.
                    self.output[10] = 0x34 | u8::from(!self.sound_enabled);
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
    fn peripherals(&mut self) {
        self.output = [0xFF; 32];
        let mut offset = 0;
        for port in 0..2 {
            self.output[offset] = 0xF1;
            offset += 1;
            if (self.input[1] >> (4 + port * 2)) & 3 == 3 {
                self.output[offset - 1] = 0xF0;
                continue;
            }
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
                if self.ddr[port] != 0x60 || self.iosel & bit == 0 || self.exle & bit != 0 {
                    return Err(Self::unsupported(address));
                }
                if self.analog[port].is_some() {
                    return Ok((self.pdr[port] & 0x60) | self.handshake[port].pins());
                }
                let nibble = match self.pdr[port] & 0x60 {
                    0x60 => 0x04 | u8::from(self.buttons[port] & (1 << 12) == 0) << 3,
                    0x20 => !self.buttons[port] as u8 & 15,
                    0x40 => !(self.buttons[port] >> 4) as u8 & 15,
                    _ => !(self.buttons[port] >> 8) as u8 & 15,
                };
                Ok((self.pdr[port] & 0x60) | 0x10 | nibble)
            }
            _ => Err(Self::unsupported(address)),
        }
    }
    pub fn write(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        match address - SMPC {
            offset @ 1..=0x0D if offset & 1 != 0 => {
                let i = ((offset - 1) / 2) as usize;
                if i == 0 && self.await_continue {
                    if value & 0x40 != 0 {
                        self.await_continue = false;
                        self.sf = false;
                        self.status &= !0x20;
                    } else if (value ^ self.input[0]) & 0x80 != 0 {
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
                    2 | 3 | 6 | 7 | 0x18 | 0x19 | 0x1A => Self::CPU_COMMAND_CLOCKS,
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
        Ok(())
    }
}
#[cfg(test)]
mod tests;
