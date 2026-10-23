//! SH-2 P §7.7.2 figs7.23–44/7.53–73, §7.7.6 figs7.88–91.
//! See docs/sh2-multiplier.md for the independently derived timing table.
use crate::bus::{BusError, Memory};

#[derive(Debug, Default, Clone)]
pub(super) struct Multiplier {
    ready: u64,
    /// Clock for architectural-only Memory implementations. Clocked adapters
    /// instead supply the PLL's monotonically advancing SH-2 clock count.
    pub clock: u64,
}

pub(super) struct Access {
    end: u64,
    stall: u32,
    tail: u64,
    pub memory_mac: bool,
}

impl Multiplier {
    pub fn access(&self, op: u16, pc: u32, now: u64, fetch: bool) -> Option<(Access, u32)> {
        let word = matches!(op & 0xF00F, 0x200E | 0x200F);
        let long = matches!(op & 0xF00F, 0x0007 | 0x3005 | 0x300D);
        let mac = matches!(op & 0xF00F, 0x000F | 0x400F);
        let transfer = op == 0x0028
            || matches!(
                op & 0xF0FF,
                0x000A | 0x001A | 0x400A | 0x401A | 0x4002 | 0x4012 | 0x4006 | 0x4016
            );
        if !(word || long || mac || transfer) {
            return None;
        }
        let two = long || mac;
        // The one-slot ID stall puts IF(PC+4) beside the first MA.
        // Its split delays the second MA, but not the first MA itself.
        let first_split = u64::from(fetch && two && pc & 2 == 0);
        let end = now + if two { 3 + first_split } else { 2 };
        let stall = self.ready.saturating_sub(end) as u32;
        let split = u32::from(fetch && (two || pc & 2 != 0));
        Some((
            Access {
                end,
                stall,
                tail: if word || op & 0xF00F == 0x400F {
                    2
                } else if long || mac {
                    4
                } else {
                    0
                },
                memory_mac: mac,
            },
            split,
        ))
    }

    pub fn wait(
        &self,
        access: &mut Access,
        bus: &mut impl Memory,
        transfer: Option<(u32, bool)>,
    ) -> Result<(), BusError> {
        if access.memory_mac {
            // The first operand is independent of the multiplier. Any time
            // spent fetching it also advances the older operation.
            if let Some(now) = bus.sh2_clock() {
                access.end = access.end.max(now + 1);
                access.stall = self.ready.saturating_sub(access.end) as u32;
            }
        }
        let clocks = if access.memory_mac || transfer.is_some() {
            // A dependent memory cycle may start in the last mm clock,
            // but must not complete before it. Include its EX/first-MA
            // lead-in even with no extra stall; these clocks are already
            // in last_clocks (P figs7.63/7.71/7.73). Only a scheduled
            // transfer supplies the last clock; synchronous cache accesses
            // must wait through the completion boundary before taking effect.
            bus.sh2_clock().map_or(access.stall, |now| {
                // MAC's second operand is an input to its multiplication;
                // retain its existing lead-in/overlap. LDS.L/STS.L instead
                // transfer MACH/MACL at the MA completion boundary.
                let latency = transfer.map_or(1, |(address, write)| {
                    bus.sh2_memory_access_latency(address, write)
                });
                (access.end + u64::from(access.stall)).saturating_sub(now + u64::from(latency))
                    as u32
            })
        } else {
            access.stall
        };
        if clocks != 0 {
            bus.sh2_pipeline_wait(clocks)?;
        }
        Ok(())
    }

    pub fn finish(&mut self, access: Access, bus: &impl Memory) -> u32 {
        if access.tail != 0 {
            let end = access.end + u64::from(access.stall);
            self.ready = bus.sh2_clock().map_or(end, |now| end.max(now)) + access.tail;
        }
        access.stall
    }
}
