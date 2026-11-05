//! Bounded snapshots of rejected transactions, without extra device reads.
use super::Smpc;
use crate::bus::BusError;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    WriteOnly,
    ReadOnly,
    ByteOnly,
    MissingSf,
    InvalidSf,
    Busy,
    UnsupportedCommand,
    InvalidParameters,
    ProhibitedIssuer,
    UnsupportedRegister,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandState {
    pub sf: bool,
    pub busy: bool,
    pub command: u8,
    pub pending: Option<u8>,
    /// Reference clocks until command completion; None while idle or waiting
    /// for INTBACK continue/VBlank. Not host time or an absolute CPU deadline.
    pub remaining: Option<u32>,
    pub await_continue: bool,
    pub await_vblank: bool,
    pub peripheral_phase: bool,
    pub input: [u8; 7],
    pub status: u8,
    pub command_report: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessFault {
    pub address: u32,
    pub width: usize,
    /// None means read; Some is the actual write operand, including zero.
    pub write: Option<u32>,
    pub reason: Rejection,
    pub state: CommandState,
    pub main_clock: Option<u64>,
    pub instruction: Option<u16>,
}

impl AccessFault {
    pub fn register(&self) -> &'static str {
        match (self.address | 1) & 0x7F {
            1..=0x0D => "IREG",
            0x1F => "COMREG",
            0x21..=0x5F => "OREG",
            0x61 => "SR",
            0x63 => "SF",
            _ => "register/lane",
        }
    }
}
impl fmt::Display for AccessFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SMPC {}: {}-byte {} at 0x{:08X}",
            self.register(),
            self.width,
            if self.write.is_some() {
                "write"
            } else {
                "read"
            },
            self.address
        )?;
        if let Some(value) = self.write {
            write!(f, " value=0x{value:08X}")?;
        }
        write!(
            f,
            ": {:?} (SF={}, busy={}, command=0x{:02X})",
            self.reason,
            u8::from(self.state.sf),
            self.state.busy,
            self.state.command
        )
    }
}

impl Smpc {
    pub fn command_state(&self) -> CommandState {
        CommandState {
            sf: self.sf,
            busy: self.busy(),
            command: self.command,
            pending: self.pending.map(|(command, _)| command),
            remaining: self.pending.map(|(_, clocks)| clocks),
            await_continue: self.await_continue,
            await_vblank: self.await_vblank,
            peripheral_phase: self.peripheral_phase,
            input: self.input,
            status: self.status,
            command_report: self.output[31],
        }
    }

    pub(crate) fn access_fault(
        &self,
        address: u32,
        width: usize,
        write: Option<u32>,
        reason: Rejection,
    ) -> BusError {
        BusError::Smpc(AccessFault {
            address,
            width,
            write,
            reason,
            state: self.command_state(),
            main_clock: None,
            instruction: None,
        })
    }
}
