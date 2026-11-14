//! Original looping guests on the actual board and production Machine.
//! No private media, replacement latency, frame skipping or game FPS estimate.
use super::*;
use crate::Machine;
use crate::bus::{HIGH_WRAM, Memory};
use crate::cpu::resumable::reference_from_state;

pub struct Experiment {
    reference: Machine,
    board: Bus,
    cpus: [TimedExecutor; 2],
    dual: bool,
    clocks: u64,
}

impl Experiment {
    fn private_span(&mut self, end: u64) -> bool {
        if self
            .cpus
            .iter()
            .take(if self.dual { 2 } else { 1 })
            .any(|cpu| cpu.transfer.is_some() || cpu.state.pc >= 0x20000000 || !cpu.port.enabled())
        {
            return false;
        }
        let start = self.board.main_clocks;
        let span = u64::from(self.board.kernel_private_horizon()).min(end - start);
        if span < 2 {
            return false;
        }
        let limit = start + span;
        let retired = self.cpus.each_ref().map(|cpu| cpu.state.instructions);
        let mut now = start;
        for _ in 0..4096 {
            let mut committed = false;
            let mut blocked = false;
            let mut next = u64::MAX;
            for cpu in self.cpus.iter_mut().take(if self.dual { 2 } else { 1 }) {
                committed |= cpu.poll_private(&mut self.board, now).unwrap();
                match cpu.deadline() {
                    Some(at) => next = next.min(at),
                    None => blocked = true,
                }
            }
            if blocked {
                break;
            }
            if committed && next <= now {
                continue;
            }
            if next >= limit {
                now = limit;
                break;
            }
            assert!(next > now, "a private continuation must make time progress");
            now = next;
        }
        for (i, cpu) in self.cpus.iter().enumerate() {
            self.board
                .kernel_retired(cpu.owner, cpu.state.instructions - retired[i]);
        }
        if now == start {
            return false;
        }
        // CPU state/cache ownership is already committed through this edge.
        // The admission horizon proves no device can observe it earlier.
        while self.board.main_clocks < now {
            let (reference, command) = self
                .board
                .advance_cpu_clocks_with_sound((now - self.board.main_clocks) as u32, true)
                .unwrap();
            assert!(command.is_none());
            self.clocks += u64::from(reference);
        }
        assert_eq!(self.board.main_clocks, now);
        true
    }

    pub fn new(memory: bool, dual: bool, bypass: bool) -> Self {
        let setup = || {
            let mut bus = Bus::default();
            bus.set_slave_enabled(dual);
            let alias = if bypass { 0x20000000 } else { 0 };
            let states = [Processor::Master, Processor::Slave].map(|owner| {
                bus.for_cpu(owner).write8(crate::cache::CCR, 1).unwrap();
                bus.for_cpu(owner).write32(0xFFFFFFE8, 0xA55A0000).unwrap();
                let pc = HIGH_WRAM + owner as u32 * 0x100;
                let words = [
                    if memory { 0x6042 } else { 0x7001 },
                    0x7001,
                    0x310C,
                    0x6203,
                    0xAFFA,
                    0x0009,
                ];
                for (i, word) in words.into_iter().enumerate() {
                    bus.write16(pc + i as u32 * 2, word).unwrap();
                }
                let mut state = super::super::Cpu::start(pc | alias, HIGH_WRAM + 0xF000)
                    .state()
                    .clone();
                state.r[4] = (HIGH_WRAM + 0x2000 + owner as u32 * 0x1000) | alias;
                bus.write32(state.r[4], 0x81234567).unwrap();
                state
            });
            (bus, states)
        };
        let (board, states) = setup();
        let (old_board, _) = setup();
        let [master, slave] = states.clone().map(reference_from_state);
        let mut reference = Machine::new();
        reference.bus = old_board;
        reference.cpu = master;
        reference.slave = slave;
        reference.cpu_ready = [0, if dual { 0 } else { u64::MAX }];
        let mut cpus = states
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                TimedExecutor::new(
                    if i == 0 {
                        Processor::Master
                    } else {
                        Processor::Slave
                    },
                    s,
                )
            })
            .collect::<Vec<_>>();
        for cpu in &mut cpus {
            cpu.set_cache_control(1).unwrap();
        }
        Self {
            reference,
            board,
            cpus: cpus.try_into().ok().unwrap(),
            dual,
            clocks: 0,
        }
    }

    /// Stop at an exact main-clock edge. A separately prepared production
    /// control supplies the endpoint outside either timed window.
    pub fn run_candidate_until(&mut self, end: u32) {
        let end = u64::from(end);
        assert!(end >= self.board.main_clocks && end - self.board.main_clocks <= 5_000_000);
        for _ in 0..10_000_000 {
            let now = self.board.main_clocks;
            if now < end && self.private_span(end) {
                continue;
            }
            let mut committed = self.cpus[0].poll(&mut self.board).unwrap();
            if self.dual {
                committed |= self.cpus[1].poll(&mut self.board).unwrap();
            }
            let next = self
                .cpus
                .iter()
                .take(if self.dual { 2 } else { 1 })
                .filter_map(TimedExecutor::deadline)
                .min()
                .unwrap_or(u64::MAX);
            self.board.grant_bus_transaction();
            if now == end {
                if committed {
                    continue;
                }
                return;
            }
            if committed && next <= now {
                continue;
            }
            let delta = next
                .saturating_sub(now)
                .min(end - now)
                .min(u64::from(self.board.kernel_deadline()))
                .max(1) as u32;
            let (reference, command) = self.board.advance_cpu_clocks(delta).unwrap();
            assert!(command.is_none(), "original guest has no SMPC commands");
            self.clocks += u64::from(reference);
        }
        panic!("bounded actual-board guest did not reach its endpoint");
    }

    pub fn reference_clock(&self) -> u32 {
        self.reference.bus.main_clocks as u32
    }

    pub fn run_reference(&mut self, fields: u32) {
        for _ in 0..fields {
            self.reference.advance_frame().unwrap();
        }
        // Both variants finish all zero-time CPU work at the endpoint. The
        // normal frame runner can return before or after issuing at that edge.
        loop {
            let mut committed = self.reference.issue_cpu(Processor::Master).unwrap();
            if self.dual {
                committed |= self.reference.issue_cpu(Processor::Slave).unwrap();
            }
            if !committed {
                break;
            }
        }
        self.reference.bus.grant_bus_transaction();
    }

    pub fn verify(&mut self) {
        assert_eq!(self.board.main_clocks, self.reference.bus.main_clocks);
        assert_eq!(self.clocks, self.reference.clocks);
        assert_eq!(self.board.raster.fields, self.reference.frames());
        assert_eq!(self.cpus[0].state(), self.reference.cpu.state());
        if self.dual {
            assert_eq!(self.cpus[1].state(), self.reference.slave.state());
        }
        assert_eq!(
            self.board.sound.scsp.generated,
            self.reference.audio_generated()
        );
        assert_eq!(
            self.board.sound.scsp.drain(16384),
            self.reference.drain_audio(16384)
        );
    }

    pub fn digest(&self) -> Vec<u32> {
        let mut digest = vec![
            self.board.main_clocks as u32,
            self.clocks as u32,
            self.board.raster.fields as u32,
        ];
        for cpu in self.cpus.iter().take(if self.dual { 2 } else { 1 }) {
            digest.extend([cpu.state().pc, cpu.state().instructions as u32]);
            digest.extend(cpu.state().r);
        }
        digest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_board_field_loops_match_production_cache_batches_and_fallback() {
        for memory in [false, true] {
            for dual in [false, true] {
                for bypass in [false, true] {
                    let mut e = Experiment::new(memory, dual, bypass);
                    e.run_reference(1);
                    e.run_candidate_until(e.reference_clock());
                    e.verify();
                    assert_eq!(e.digest()[2], 1);
                    e.run_reference(1);
                    let end = e.reference_clock();
                    // Host-requested splits must not move a sampling/retirement
                    // edge, even when the cut bisects an IF or MA continuation.
                    while e.board.main_clocks < u64::from(end) {
                        e.run_candidate_until((e.board.main_clocks as u32 + 37).min(end));
                    }
                    e.verify();
                    assert_eq!(e.digest()[2], 2);
                }
            }
        }
    }

    #[test]
    fn private_execution_yields_at_code_and_streaming_operand_misses() {
        for dual in [false, true] {
            for bypass in [false, true] {
                let mut e = Experiment::new(true, dual, bypass);
                let alias = if bypass { 0x20000000 } else { 0 };
                for i in 0..if dual { 2 } else { 1 } {
                    let pc = HIGH_WRAM + i as u32 * 0x100;
                    for (j, word) in [
                        0x6042, 0x7001, 0x310C, 0x6203, 0x7404, 0x2459, 9, 9, 0xAFF6, 9,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        e.board.write16(pc + j as u32 * 2, word).unwrap();
                        e.reference.bus.write16(pc + j as u32 * 2, word).unwrap();
                    }
                    e.cpus[i].state.r[5] = HIGH_WRAM | alias | 0xFFFFC;
                    if i == 0 {
                        e.reference.cpu.state.r[5] = e.cpus[i].state.r[5];
                    } else {
                        e.reference.slave.state.r[5] = e.cpus[i].state.r[5];
                    }
                }
                e.run_reference(1);
                e.run_candidate_until(e.reference_clock());
                e.verify();
            }
        }
    }
}
