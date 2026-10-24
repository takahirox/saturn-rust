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
    first_end: u64,
    successor_ex: u64,
    stall: u32,
    tail: u64,
    pub memory_mac: bool,
    memory_access: bool,
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
                first_end: now + 2,
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

    pub fn first_operand(
        &self,
        access: &Access,
        bus: &mut impl Memory,
        address: u32,
    ) -> Result<(), BusError> {
        if let Some(now) = bus.sh2_clock() {
            // P figs7.23/34: the independent first MA follows EX. A
            // scheduled read starts at MA; a synchronous read samples at
            // its end. Its actual completion is observed in wait() below.
            let scheduled = bus.sh2_schedule_memory_access(address, false, access.first_end);
            let boundary = access.first_end - u64::from(scheduled);
            bus.sh2_pipeline_wait(boundary.saturating_sub(now) as u32)?;
        }
        Ok(())
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
                // §7.2.3 stretches the first MA's slot by its actual
                // latency. Preserve any following IF half of that split
                // slot before the second MA and successor EX can start.
                let elapsed = now.saturating_sub(access.first_end);
                access.end += elapsed;
                access.successor_ex += elapsed;
                access.stall = self.ready.saturating_sub(access.end) as u32;
            }
        }
        let clocks = if access.memory_mac || transfer.is_some() {
            // P §7.2.3, figs7.64/65/73: the operand cycle and multiplier
            // wait occupy the same extended MA. Start a scheduled cycle at
            // its ordinary MA boundary and constrain its completion, rather
            // than serializing its remaining latency after the mm wait.
            bus.sh2_clock().map_or(access.stall, |now| {
                let completion = access.end + u64::from(access.stall);
                let scheduled = transfer.is_some_and(|(address, write)| {
                    bus.sh2_schedule_memory_access(address, write, completion)
                });
                access.memory_access = transfer.is_some();
                let boundary = if scheduled && transfer.is_some_and(|(_, write)| write) {
                    // STS supplies its accumulator value to the write buffer
                    // at MA completion; preserve its one-clock admission.
                    completion - 1
                } else if scheduled {
                    access.end - 1
                } else {
                    completion
                };
                boundary.saturating_sub(now) as u32
            })
        } else {
            access.stall
        };
        if clocks != 0 {
            bus.sh2_pipeline_wait(clocks)?;
        }
        Ok(())
    }

    pub fn finish(
        &mut self,
        access: Access,
        bus: &impl Memory,
        pipeline: &mut super::timing::LoadPipeline,
    ) -> u32 {
        if access.memory_access && access.successor_ex < access.end {
            // P table7.2/figs7.23/34/89/91: MA completion does not delay the
            // successor's EX. Keep effects at MA, but recover its overlap.
            // §7.2.3 stretches the shared slot when the operand takes longer:
            // retain its final clock, rather than expiring at nominal MA end.
            let successor_ex = access.successor_ex + u64::from(access.stall);
            pipeline.retain_ex(
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
