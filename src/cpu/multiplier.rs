//! SH-2 P §7.7.2 figs7.23–44/7.53–73, §7.7.6 figs7.88–91.
//! See docs/sh2-multiplier.md for the independently derived timing table.
use crate::bus::{BusError, Memory};

#[derive(Debug, Default, Clone)]
pub(super) struct Multiplier {
    ready: u64,
    /// Clock for architectural-only Memory implementations. Clocked adapters
    /// instead supply the PLL's monotonically advancing SH-2 clock count.
    pub clock: u64,
    /// An aligned MAC or MACH/MACL memory transfer retires after the
    /// successor's EX starts. Retain its final shared-slot clock for scheduling.
    next_ex: Option<u64>,
    pub issue_overlap: u32,
}

pub(super) struct Access {
    end: u64,
    successor_ex: u64,
    stall: u32,
    tail: u64,
    pub memory_mac: bool,
    memory_access: bool,
}

impl Multiplier {
    pub fn issue_clock(&mut self, now: u64) -> u64 {
        // A waited fetch exhausts the available overlap. Observations are
        // journaled, so replay consumes the same EX boundary exactly once.
        self.issue_overlap = u32::from(self.next_ex.take().is_some_and(|ex| now == ex + 1));
        now - u64::from(self.issue_overlap)
    }

    pub fn exception(&mut self) {
        self.next_ex = None;
        self.issue_overlap = 0;
    }

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
                // MAC has two baseline EX intervals (P figs7.23/34),
                // while LDS.L/STS.L has one (figs7.89/91).
                successor_ex: now + 1 + u64::from(mac) + u64::from(split),
                stall,
                tail: if word || op & 0xF00F == 0x400F {
                    2
                } else if long || mac {
                    4
                } else {
                    0
                },
                memory_mac: mac,
                memory_access: false,
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
            // lead-in even with no extra stall (P figs7.63/7.71/7.73).
            // finish retains the successor's EX overlap. Only a scheduled
            // transfer supplies the last clock; synchronous cache accesses
            // must wait through the completion boundary before taking effect.
            bus.sh2_clock().map_or(access.stall, |now| {
                // MAC's second operand and LDS.L/STS.L effects must all
                // reach their MA completion boundary (P figs7.64/65/89/91).
                // Journal the addressed access's latency before the wait.
                let latency = transfer.map_or(0, |(address, write)| {
                    bus.sh2_memory_access_latency(address, write)
                });
                access.memory_access = transfer.is_some();
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
        if access.memory_access && access.successor_ex < access.end {
            // P table7.2/figs7.23/34/89/91: MA completion does not delay the
            // successor's EX. Keep effects at MA, but recover its overlap.
            // §7.2.3 stretches the shared slot when the operand takes longer:
            // retain its final clock, rather than expiring at nominal MA end.
            let successor_ex = access.successor_ex + u64::from(access.stall);
            self.next_ex = Some(
                bus.sh2_clock()
                    .map_or(successor_ex, |now| successor_ex.max(now.saturating_sub(1))),
            );
        }
        if access.tail != 0 {
            let end = access.end + u64::from(access.stall);
            self.ready = bus.sh2_clock().map_or(end, |now| end.max(now)) + access.tail;
        }
        access.stall
    }
}
