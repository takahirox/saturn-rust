//! Run a resident register and HIRQ sequence with live operands.
//!
//! The parent supplies the ordinary bounded clock horizon and handles peer
//! selection, Sound lookahead, rollback, and publication of real device edges.
use super::*;
mod resident_words;

pub(super) fn is_hirq(passive: &PassiveRead<'_>) -> bool {
    Bus::physical(passive.address) == crate::cd::REGISTERS + 8 && passive.rom.is_none()
}

#[derive(Default)]
pub(super) struct Window {
    pub(super) retired: u32,
    pub(super) at: u64,
    pub(super) ready: u64,
    #[cfg(test)]
    pub(super) loads: u32,
    #[cfg(test)]
    pub(super) reused_fetches: u32,
}

impl Cached<'_> {
    pub(super) fn execute_hirq_window<const REUSE_WORDS: bool>(
        &mut self,
        cpu: &mut Cpu,
        limit: u32,
        mut points: Option<&mut Vec<Retirement>>,
        mut learning: Option<&mut super::super::loop_probe::LoopProbe>,
    ) -> Window {
        let end = self.end.get();
        let mut result = Window::default();
        let mut words = resident_words::Words::default();
        while result.retired < limit && self.clock.get() < end {
            let start = self.clock.get();
            let Some(opcode) = cpu.decoded_opcode().or_else(|| self.opcode(cpu.state().pc)) else {
                break;
            };
            if let Some(probe) = learning.as_deref_mut() {
                probe.discover(cpu.state().pc, opcode);
            }
            let class = Cpu::cached_class(opcode);
            if class == CachedClass::Scalar {
                self.scalar_active.set(true);
                let remaining = (limit - result.retired).min((end - start) as u32);
                let retired = if REUSE_WORDS {
                    cpu.cached_scalar_block_with_fetch(start, remaining, |pc, address| {
                        let word = words.prepare(self, address)?.publish(self);
                        self.instruction_break(pc, false);
                        Some(word)
                    })
                } else {
                    cpu.cached_scalar_block(self, remaining)
                };
                self.scalar_active.set(false);
                if retired != 0 {
                    result.at = start + u64::from(retired) - 1;
                    result.ready = result.at + 1;
                    result.retired += retired;
                    self.clock.set(result.ready);
                    if let Some(points) = points.as_deref_mut() {
                        points.extend((start..=result.at).map(|at| Retirement { at }));
                    }
                    continue;
                }
            }

            let at = match class {
                CachedClass::Scalar | CachedClass::Load => {
                    let Some(plan) = cpu.prepare_cached_instruction(opcode, start) else {
                        break;
                    };
                    let future = if REUSE_WORDS {
                        words.prepare(self, plan.future).map(Some)
                    } else {
                        self.is_resident_window_word(plan.future).then_some(None)
                    };
                    let Some(future) = future else {
                        break;
                    };
                    if let Some((address, width)) = plan.operand {
                        let Some(at) = self.retire_hirq_word(cpu, plan, address, width, future)
                        else {
                            break;
                        };
                        #[cfg(test)]
                        {
                            result.loads += 1;
                        }
                        at
                    } else {
                        if plan.at > end {
                            break;
                        }
                        let at = plan.at;
                        if let Some(future) = future {
                            self.instruction_break(cpu.state().pc, false);
                            let word = future.publish(self);
                            self.clock.set(at);
                            cpu.retire_resident_scalar(plan, word);
                        } else {
                            cpu.execute_cached_instruction(plan, self);
                        }
                        at
                    }
                }
                CachedClass::Control => {
                    let Some(plan) = cpu
                        .prepare_resident_control(start)
                        .filter(|plan| plan.at <= end)
                    else {
                        break;
                    };
                    let Some(reads) = self.resident_control_reads(&plan) else {
                        break;
                    };
                    self.instruction_break(cpu.state().pc, false);
                    for touch in reads.touches.into_iter().flatten() {
                        touch.apply(&mut self.lru.borrow_mut());
                    }
                    self.retained.set(reads.retained);
                    let at = plan.at;
                    cpu.retire_cached_control(plan.finish(reads.values));
                    self.clock.set(at);
                    at
                }
                CachedClass::Other => break,
            };
            debug_assert_eq!(self.clock.get(), at);
            result.at = at;
            result.ready = (cpu.execution_clock().unwrap() + u64::from(cpu.last_clocks())).max(at);
            result.retired += 1;
            self.clock.set(result.ready.min(end));
            if let Some(points) = points.as_deref_mut() {
                points.push(Retirement { at });
            }
        }
        #[cfg(test)]
        {
            result.reused_fetches = words.reused;
        }
        result
    }

    fn is_resident_window_word(&self, address: u32) -> bool {
        ordinary_memory(address)
            && (self.retained.get().is_some_and(|(at, _)| at == address) || self.hit(address))
    }

    fn retire_hirq_word(
        &mut self,
        cpu: &mut Cpu,
        plan: CachedInstruction,
        address: u32,
        width: u32,
        fetch: Option<resident_words::Fetch>,
    ) -> Option<u64> {
        let passive = self
            .passive
            .as_ref()
            .filter(|p| p.address == address && width == 2 && p.allowed_widths & 2 != 0)?;
        let start = plan.issue_at() + u64::from(plan.load_wait(plan.issue_at(), true));
        let mut sequence = passive.sequence.borrow().clone();
        let at = sequence
            .read(Bus::physical(address), 2, start)?
            .max(plan.at);
        if at > self.end.get() {
            return None;
        }

        let [a, b, _, _] = passive.bytes;
        let data = if passive.little {
            i16::from_le_bytes([a, b])
        } else {
            i16::from_be_bytes([a, b])
        };
        self.instruction_break(cpu.state().pc, false);
        let future = fetch.map_or_else(
            || {
                self.fetch16(plan.future)
                    .expect("proven resident window IF")
            },
            |fetch| fetch.publish(self),
        );
        *self.passive.as_mut().unwrap().sequence.get_mut() = sequence;
        self.clock.set(at);
        cpu.retire_prepared_load(plan, future, data as i32 as u32, at);
        Some(at)
    }
}
