//! Train only a private CPU/cache copy, preserving the real field endpoint.
use super::*;
use crate::bus::Processor;

impl Machine {
    pub(crate) fn maybe_learn_loop(&mut self) {
        let Some(probe) = self.bus.loop_probe.as_ref() else {
            return;
        };
        let now = self.bus.main_clocks;
        if !probe.replaying
            || !probe.learning
            || now < probe.retry
            || !probe.headers.contains(&self.cpu.state().pc)
            || self.fault.is_some()
            || self.nmi_pending
            || self.cpu_boundary[0].is_some()
            || self.cached_issue[0].is_some()
            || self.cpu_reset_vectors[0].is_some()
            || !self.bus.cached_master_admissible(&self.cpu)
        {
            return;
        }
        let mut probe = self.bus.loop_probe.take().unwrap();
        probe.retry = now.saturating_add(500_000);
        let current_count = probe.replayed.get();
        let progressed = current_count.saturating_sub(probe.last_training_count);
        probe.last_training_count = current_count;
        if probe.captured.is_some() && progressed >= 50_000 {
            self.bus.loop_probe = Some(probe);
            return;
        }
        let mut scratch = Box::new(Machine::new());
        scratch.cpu = self.cpu.clone();
        scratch.cpu_ready[0] = self.cpu_ready[0];
        scratch.cpu_ready[1] = u64::MAX;
        scratch.clocks = self.clocks;
        scratch.bus.main_clocks = now;
        scratch.bus.system_clock = self.bus.system_clock.clone();
        scratch.bus.pending_main_clocks = self.bus.pending_main_clocks;
        scratch.bus.external_owner.set(Some(Processor::Master));
        scratch.bus.onchip[0] = self.bus.onchip[0].clone();
        // Interrupt admission remains real at execution time. Training only
        // reconstructs the straight CPU/cache path between those deadlines.
        scratch.bus.onchip[0]
            .write16(crate::onchip::IPRA, 0)
            .unwrap();
        scratch.bus.onchip[0]
            .write16(crate::onchip::IPRB, 0)
            .unwrap();
        let cache = self.bus.cache[0].borrow();
        scratch.bus.cache[0].get_mut().copy_private(&cache, 0);
        scratch.bus.cache[0]
            .get_mut()
            .commit_replacement_state(cache.replacement_state());
        drop(cache);
        scratch.bus.instruction_word[0].set(self.bus.instruction_word[0].get());
        scratch.bus.loop_probe = Some(Box::new(LoopProbe {
            best: Some((self.cpu.state().pc, 512)),
            ..LoopProbe::default()
        }));
        if scratch.capture_loop_trace_with_overlap(4096, true).is_ok()
            && let Some(data) = scratch.bus.loop_probe.as_mut().unwrap().captured.take()
            && data.complete
            && !data.compiled.is_empty()
            && data.external_reads == 0
            && data.unsupported == 0
        {
            probe.captured = Some(data);
        }
        self.bus.loop_probe = Some(probe);
    }
}
