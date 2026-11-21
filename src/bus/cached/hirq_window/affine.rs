//! Fold an actual bounded CD interrupt-status poll with one changing register.
use crate::{Cpu, bus::loop_probe::LoopProbe};

const MAX_PERIOD: u32 = 16;

#[derive(Clone, Copy, Default)]
struct Instruction {
    pc: u32,
    opcode: u16,
    hirq_read: bool,
}

/// The live IF/ID pipeline provides these words after their exact operations
/// are admitted. A newly encountered gap drops the previous partial period.
#[derive(Default)]
pub(super) struct Trace {
    from: u32,
    through: u32,
    steps: [Instruction; MAX_PERIOD as usize],
}

impl Trace {
    pub(super) fn record(&mut self, ordinal: u32, pc: u32, opcode: u16, hirq_read: bool) {
        if self.through != ordinal {
            self.from = ordinal;
        }
        self.steps[(ordinal % MAX_PERIOD) as usize] = Instruction {
            pc,
            opcode,
            hirq_read,
        };
        self.through = ordinal + 1;
        self.from = self.from.max(self.through.saturating_sub(MAX_PERIOD));
    }

    fn word(&self, ordinal: u32) -> Instruction {
        self.steps[(ordinal % MAX_PERIOD) as usize]
    }

    // Discovery inserts headers into a bounded set. If the observed cycle
    // just filled and cleared it, a second pass may restore targets from the
    // start of that cycle. At most 16 targets exist and all then stay present.
    pub(super) fn discover_repeated_branches(
        &self,
        probe: &mut LoopProbe,
        first: u32,
        through: u32,
    ) {
        for i in first..through {
            let instruction = self.word(i);
            probe.discover(instruction.pc, instruction.opcode);
        }
    }

    pub(super) fn repeat(
        &self,
        prior: &Cpu,
        current: &mut Cpu,
        first: u32,
        through: u32,
        elapsed: u64,
        maximum: u64,
    ) -> u64 {
        if self.through != through || first < self.from || through - first < 4 {
            return 0;
        }
        let Some(induction) = Induction::decode(self, first, through, prior.state().pc) else {
            return 0;
        };
        let repeats = induction.repeats(prior, current, maximum);
        if repeats != 0
            && current.repeat_cached_cycle_with_delta(
                prior,
                elapsed,
                repeats,
                Some((induction.register, induction.step)),
            )
        {
            repeats
        } else {
            0
        }
    }
}

#[derive(Clone, Copy)]
struct Induction {
    register: usize,
    step: i32,
    left: usize,
    right: usize,
    signed: bool,
    inclusive: bool,
    positive_branch: bool,
}

impl Induction {
    fn decode(trace: &Trace, first: u32, through: u32, header: u32) -> Option<Self> {
        if trace.word(first).pc != header {
            return None;
        }
        let comparison = trace.word(through - 2);
        let branch = trace.word(through - 1);
        let positive_branch = match branch.opcode >> 8 {
            0x89 => true,
            0x8B => false,
            _ => return None,
        };
        let target = branch
            .pc
            .wrapping_add(4)
            .wrapping_add_signed(i32::from(branch.opcode as i8) * 2);
        if target != header || target >= branch.pc || comparison.pc + 2 != branch.pc {
            return None;
        }
        let (signed, inclusive) = match comparison.opcode & 0xF00F {
            0x3002 => (false, true),
            0x3003 => (true, true),
            0x3006 => (false, false),
            0x3007 => (true, false),
            _ => return None,
        };
        let left = usize::from(comparison.opcode >> 8 & 15);
        let right = usize::from(comparison.opcode >> 4 & 15);
        if left == right {
            return None;
        }

        let body = first..through - 2;
        let mut increments = body.clone().filter_map(|ordinal| {
            let op = trace.word(ordinal).opcode;
            let register = usize::from(op >> 8 & 15);
            (op >> 12 == 7 && (register == left || register == right)).then_some((
                ordinal,
                register,
                i32::from(op as i8),
            ))
        });
        let (increment, register, step) = increments.next()?;
        if increments.next().is_some() || step == 0 {
            return None;
        }
        let mut hirq_loads = 0;
        for ordinal in body {
            if ordinal == increment {
                continue;
            }
            let Instruction {
                opcode, hirq_read, ..
            } = trace.word(ordinal);
            hirq_loads += u32::from(hirq_read);
            if register_mask(opcode, hirq_read)? & (1 << register) != 0 {
                return None;
            }
        }
        (hirq_loads == 1).then_some(Self {
            register,
            step,
            left,
            right,
            signed,
            inclusive,
            positive_branch,
        })
    }

    fn repeats(self, prior: &Cpu, current: &Cpu, maximum: u64) -> u64 {
        let old = prior.state().r[self.register];
        let register = &current.state().r;
        if old.wrapping_add_signed(self.step) != register[self.register] {
            return 0;
        }
        let bound = register[if self.left == self.register {
            self.right
        } else {
            self.left
        }];
        self.repeats_from(register[self.register], bound, maximum)
    }

    fn repeats_from(self, counter: u32, fixed: u32, maximum: u64) -> u64 {
        let value = |n: u32| {
            if self.signed {
                i64::from(n as i32)
            } else {
                i64::from(n)
            }
        };
        let initial = value(counter);
        let (min, max) = if self.signed {
            (i64::from(i32::MIN), i64::from(i32::MAX))
        } else {
            (0, i64::from(u32::MAX))
        };
        let bound = value(fixed);
        let takes_branch = |periods| {
            let changing = initial + i64::from(self.step) * periods;
            if !(min..=max).contains(&changing) {
                return false;
            }
            let (left, right) = if self.left == self.register {
                (changing, bound)
            } else {
                (bound, changing)
            };
            let condition = if self.inclusive {
                left >= right
            } else {
                left > right
            };
            condition == self.positive_branch
        };
        if !takes_branch(0) {
            return 0;
        }
        // The comparison of one monotone counter against the stable bound
        // cannot become true again before wrapping. Stop before that wrap too.
        let (mut lo, mut hi) = (0, i64::try_from(maximum).expect("bounded cached span") + 1);
        while lo + 1 < hi {
            let mid = lo + (hi - lo) / 2;
            if takes_branch(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo as u64
    }
}

fn register_mask(op: u16, passive_word: bool) -> Option<u16> {
    let rm = 1 << (op >> 4 & 15);
    let rn = 1 << (op >> 8 & 15);
    if passive_word {
        return match op & 0xF00F {
            0x6001 | 0x6005 => Some(rm | rn),
            0x000D => Some(rm | rn | 1),
            _ if op >> 8 == 0x85 => Some(rm | 1),
            _ if op >> 8 == 0xC5 => Some(1),
            _ => None,
        };
    }
    match op & 0xF00F {
        0x6003 | 0x600C..=0x600F | 0x2008..=0x200B => Some(rm | rn),
        _ if matches!(op >> 12, 7 | 0xE) => Some(rn),
        _ if matches!(op >> 8, 0xC8..=0xCB) => Some(1),
        _ if matches!(op >> 8, 0x89 | 0x8B | 0x8D | 0x8F) || op >> 12 == 0xA || op == 9 => Some(0),
        _ => None,
    }
}

#[cfg(test)]
pub(super) mod tests {

    #[test]
    fn repeated_hirq_discovery_preserves_headers_after_saturation() {
        use super::{LoopProbe, Trace};
        let prior_pc = 0x600_1000;
        let new_pc = 0x600_9000;
        let branch = 0x8be0;
        let mut trace = Trace::default();
        trace.record(0, prior_pc, branch, false);
        trace.record(1, new_pc, branch, false);
        let make = || {
            let mut probe = LoopProbe::automatic();
            for i in 0..128 {
                probe.discover(prior_pc + i * 2, branch);
            }
            // The tail of the just-observed literal cycle introduces a new
            // header and clears the full set, including the first branch.
            probe.discover(prior_pc, branch);
            probe.discover(new_pc, branch);
            assert!(!probe.is_header(prior_pc + 4 - 64));
            probe
        };
        let (mut literal, mut folded) = (make(), make());
        for _ in 0..32 {
            literal.discover(prior_pc, branch);
            literal.discover(new_pc, branch);
        }
        trace.discover_repeated_branches(&mut folded, 0, 2);
        assert_eq!(folded.discovered_headers(), literal.discovered_headers());
    }

    #[test]
    fn hirq_induction_stops_at_signed_unsigned_and_wrap_exits() {
        use super::Induction;

        let loop_until_greater = Induction {
            register: 0,
            step: 1,
            left: 0,
            right: 1,
            signed: true,
            inclusive: false,
            positive_branch: false,
        };
        assert_eq!(loop_until_greater.repeats_from(2, 5, 10), 3);
        assert_eq!(loop_until_greater.repeats_from(6, 5, 10), 0);
        let limit_on_left = Induction {
            left: 1,
            right: 0,
            step: 2,
            positive_branch: true,
            ..loop_until_greater
        };
        assert_eq!(limit_on_left.repeats_from(6, 10, 20), 1);
        let signed_wrap = Induction {
            positive_branch: true,
            ..loop_until_greater
        };
        assert_eq!(signed_wrap.repeats_from((i32::MAX - 2) as u32, 0, 10), 2);
        assert_eq!(signed_wrap.repeats_from((i32::MAX - 2) as u32, 0, 1), 1);
        let unsigned_down = Induction {
            step: -3,
            signed: false,
            ..signed_wrap
        };
        assert_eq!(unsigned_down.repeats_from(6, 0, 10), 1);
    }

    fn literal_poll(
        fast: bool,
        branch_true: bool,
        reads_counter: bool,
        audible: bool,
    ) -> crate::Machine {
        use super::super::{HIGH_WRAM, Memory, Processor};
        use crate::{Cpu, Machine};

        // The status path increments one counter in its branch's delay slot.
        // The other path, counter exit, and following reset run normally.
        let copy = if reads_counter { 0x6133 } else { 0x6103 };
        let (compare, branch) = if branch_true {
            (0x3437, 0x89F5)
        } else {
            (0x3347, 0x8BF5)
        };
        let words = [
            0xE47F, 0xE300, 0x60F1, 0x600D, copy, 0xC901, 0xC800, 0x8D01, 0x7301, 0xE200, compare,
            branch, 0xE300, 0xAFF3, 9,
        ];
        let mut machine = Machine::new();
        machine.bus.change_system_clock(fast);
        for (i, word) in words.into_iter().enumerate() {
            machine.bus.write16(HIGH_WRAM + 2 * i as u32, word).unwrap();
        }
        let mut port = machine.bus.for_cpu(Processor::Master);
        port.write32(0xFFFF_FFE8, 0xA55A_0000).unwrap();
        port.write8(crate::cache::CCR, 0x11).unwrap();
        assert_eq!(port.fetch16(HIGH_WRAM), Ok(words[0]));
        assert_eq!(port.fetch16(HIGH_WRAM + 16), Ok(words[8]));
        machine.bus.cpu_main_delay(Processor::Master, 0);
        machine.bus.advance_clocks(48).unwrap();
        machine.clocks = 48;
        machine.cpu_ready = [machine.bus.main_clocks; 2];
        machine.cpu = Cpu::start(HIGH_WRAM, (crate::cd::REGISTERS + 8) | 0x2000_0000);
        machine
            .cpu
            .prime_fetch(&machine.bus.for_cpu(Processor::Master));
        let chip = &mut machine.bus.onchip[0];
        chip.write32(0xFFFF_FFE0, 0xA55A_0004).unwrap();
        chip.write32(0xFFFF_FFEC, 0xA55A_0048).unwrap();
        chip.write32(0xFFFF_FFF8, 0xA55A_007F).unwrap();
        chip.write32(0xFFFF_FFF4, 0xA55A_0010).unwrap();
        chip.write32(0xFFFF_FFF0, 0xA55A_0008).unwrap();
        machine
            .bus
            .scu
            .write32(super::super::SCU + 0xB4, 0x60000000)
            .unwrap();
        machine
            .bus
            .scu
            .write32(super::super::SCU + 0xB8, 0x1F)
            .unwrap();
        #[cfg(test)]
        {
            machine.bus.hirq_refresh_plan = true;
            machine.bus.hirq_refresh_overlap = true;
            machine.bus.hirq_refresh_wait = true;
            machine.bus.hirq_two_refreshes = true;
            machine.bus.hirq_sound_refresh = true;
        }
        if audible {
            let sound = &mut machine.bus.sound;
            sound.ram[..4].copy_from_slice(&0x1000u32.to_be_bytes());
            sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
            sound.ram[0x100..0x104].copy_from_slice(&[0x4e, 0x71, 0x60, 0xfc]);
            sound.set_enabled(true).unwrap();
        }
        machine.bus.claim_external_bus(Processor::Master);
        machine
    }

    impl crate::Machine {
        fn verify_hirq_affine_refresh(fast: bool, reads_counter: bool, audible: bool) -> [u64; 4] {
            let mut literal = literal_poll(fast, !fast, reads_counter, audible);
            let mut folded = literal_poll(fast, !fast, reads_counter, audible);
            literal.bus.hirq_regional_folds = false;
            for _ in 0..2 {
                literal.advance_frame().unwrap();
                folded.advance_frame().unwrap();
                assert_eq!(folded.cpu, literal.cpu);
                assert_eq!(folded.cpu_ready, literal.cpu_ready);
                assert_eq!(folded.clocks, literal.clocks);
                assert_eq!(folded.bus.main_clocks, literal.bus.main_clocks);
                assert_eq!(folded.loop_state_hashes(), literal.loop_state_hashes());
                assert_eq!(
                    folded.bus.loop_probe.as_ref().unwrap().discovered_headers(),
                    literal
                        .bus
                        .loop_probe
                        .as_ref()
                        .unwrap()
                        .discovered_headers()
                );
                assert!(folded.bus.sound.domain_snapshot() == literal.bus.sound.domain_snapshot());
                assert_eq!(folded.drain_audio(8192), literal.drain_audio(8192));
                assert!(folded.bus.scu.plan_cached_abus() == literal.bus.scu.plan_cached_abus());
                #[cfg(test)]
                folded.bus.onchip[0].assert_cached_read_bus_matches(&literal.bus.onchip[0]);
            }
            assert_eq!(literal.bus.hirq_regional_fold_counts, [0; 4]);
            folded.bus.hirq_regional_fold_counts
        }
    }

    #[test]
    fn hirq_poll_folds_only_the_exact_induction_register_in_real_refresh_regions() {
        for fast in [false, true] {
            for audible in [false, true] {
                let [before, between, after, multiple] =
                    crate::Machine::verify_hirq_affine_refresh(fast, false, audible);
                assert!(
                    before > 0 && between + after > 0 && multiple > 0,
                    "retire exact cycles on both sides of a physical refresh {before} {between} {after} {multiple}"
                );
                assert_eq!(
                    crate::Machine::verify_hirq_affine_refresh(fast, true, audible),
                    [0; 4]
                );
            }
        }
    }
}
