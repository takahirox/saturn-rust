//! ST-169 §3.4 table 3.37: TH/TR/TL three-wire protocol.
//! STN-43 (1996-05-16), Analog mode: ID16 X,Y,AR,AL and trigger hysteresis.
#[derive(Clone, Copy)]
pub(super) enum Analog {
    Joystick([u8; 3]),
    Pad { axes: [u8; 4], triggers: [bool; 2] },
}
impl Analog {
    pub(super) fn pad(previous: Option<Self>, axes: [u8; 4]) -> Self {
        let mut triggers = match previous {
            Some(Self::Pad { triggers, .. }) => triggers,
            _ => [false; 2],
        };
        for (i, state) in triggers.iter_mut().enumerate() {
            if axes[2 + i] >= 146 {
                *state = true;
            } else if axes[2 + i] <= 85 {
                *state = false;
            }
        }
        Self::Pad { axes, triggers }
    }
    pub(super) fn report(self, mut buttons: u16) -> ([u8; 7], usize) {
        if let Self::Pad { triggers, .. } = self {
            buttons &= !0x1800;
            buttons |= u16::from(triggers[0]) << 11 | u16::from(triggers[1]) << 12;
        }
        let [a, b] = digital(buttons);
        match self {
            Self::Joystick([x, y, z]) => ([0x15, a, b, x, y, z, 0], 6),
            Self::Pad {
                axes: [x, y, r, l], ..
            } => ([0x16, a, b, x, y, r, l], 7),
        }
    }
}
pub(super) fn digital(buttons: u16) -> [u8; 2] {
    [
        !(((buttons & 15) << 4) | ((buttons >> 4) & 15)) as u8,
        !(((buttons >> 8) & 15) << 4 | ((buttons >> 9) & 8)) as u8,
    ]
}

pub(super) struct Handshake {
    data: [u8; 16],
    length: usize,
    index: usize,
    nibble: u8,
    tl: bool,
    active: bool,
    pending: Option<(u8, bool, u32)>,
}
impl Default for Handshake {
    fn default() -> Self {
        Self {
            data: [0; 16],
            length: 0,
            index: 0,
            nibble: 1,
            tl: true,
            active: false,
            pending: None,
        }
    }
}
impl Handshake {
    // 2 µs, rounded up at the NTSC reference clock. This models an emulated
    // device response deadline, without a wall-clock wait or host-injected data.
    pub(super) const RESPONSE_CLOCKS: u32 = 54;
    pub(super) fn pins(&self) -> u8 {
        u8::from(self.tl) << 4 | self.nibble
    }
    pub(super) fn advance(&mut self, clocks: u32) {
        if let Some((nibble, tl, remaining)) = self.pending {
            if clocks >= remaining {
                self.nibble = nibble;
                self.tl = tl;
                self.pending = None;
            } else {
                self.pending = Some((nibble, tl, remaining - clocks));
            }
        }
    }
    pub(super) fn write(&mut self, old: u8, new: u8, analog: Analog, buttons: u16) {
        if new & 0x40 != 0 {
            *self = Self::default();
        } else if old & 0x40 != 0 {
            // Snapshot the complete transaction when TH falls. Later host input
            // changes are visible on the next transaction, never half a packet.
            let (bytes, count) = analog.report(buttons);
            for (i, byte) in bytes[..count].iter().enumerate() {
                self.data[i * 2] = byte >> 4;
                self.data[i * 2 + 1] = byte & 15;
            }
            self.data[count * 2] = 0;
            self.data[count * 2 + 1] = 1;
            self.length = count * 2 + 2;
            self.index = 0;
            self.active = true;
            self.pending = Some((1, true, Self::RESPONSE_CLOCKS));
        } else if self.active && self.pending.is_none() && (old ^ new) & 0x20 != 0 {
            let nibble = self.data[self.index.min(self.length - 1)];
            self.index = (self.index + 1).min(self.length);
            self.pending = Some((nibble, new & 0x20 != 0, Self::RESPONSE_CLOCKS));
        }
    }
}
