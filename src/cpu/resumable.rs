//! Experimental continuation executor. ISA semantics are shared, execution control is not.
use super::{Branch, CpuState, CpuStatus, commit_mov_load, scalar, timing};
use crate::kernel::{Error, Fabric, Operation};

pub(crate) fn reference_from_state(state: CpuState) -> super::Cpu {
    let mut cpu = super::Cpu::start(state.pc, state.r[15]);
    cpu.state = state;
    cpu
}

enum Effect {
    Scalar(scalar::Kind),
    Branch(Branch),
    Load { width: u32, value: u32 },
    Store { update: Option<(usize, u32)> },
    MoveAddress(u32),
    Sleep,
}

enum Phase {
    Ready,
    Fetch,
    Operand {
        opcode: u16,
        states: u32,
        effect: Effect,
    },
    Retire {
        at: u64,
        opcode: u16,
        effect: Effect,
    },
}

/// No CPU snapshot or replay journal: an accepted operand has one continuation.
/// This executor deliberately does not implement Clone.
pub struct Executor {
    state: CpuState,
    phase: Phase,
    delay_slot: bool,
}

impl Executor {
    pub fn new(state: CpuState) -> Self {
        let delay_slot = state.delay_target.is_some();
        Self {
            state,
            phase: Phase::Ready,
            delay_slot,
        }
    }

    pub fn state(&self) -> &CpuState {
        &self.state
    }

    pub(crate) fn deadline(&self) -> Option<u64> {
        match self.phase {
            Phase::Retire { at, .. } => Some(at),
            _ => None,
        }
    }

    pub(crate) fn poll(
        &mut self,
        owner: usize,
        now: u64,
        fabric: &mut Fabric,
        target: Option<u64>,
    ) -> Result<(), Error> {
        loop {
            match self.phase {
                Phase::Ready => {
                    if target.is_some_and(|target| fabric.retired[owner] >= target)
                        || self.state.status != CpuStatus::Running
                    {
                        return Ok(());
                    }
                    fabric.request(
                        owner,
                        now,
                        Operation::Fetch {
                            address: self.state.pc,
                        },
                    )?;
                    self.phase = Phase::Fetch;
                    return Ok(());
                }
                Phase::Fetch => {
                    let Some(opcode) = fabric.reply(owner) else {
                        return Ok(());
                    };
                    let opcode = opcode as u16;
                    let descriptor = timing::instruction(opcode);
                    let states = descriptor.states(self.state.sr & super::T != 0).max(1);
                    let effect = if let Some(kind) = descriptor.prepared_scalar() {
                        Effect::Scalar(kind)
                    } else if let Some(branch) = Branch::decode(opcode, &self.state) {
                        if self.delay_slot {
                            return Err(self.unsupported(opcode));
                        }
                        Effect::Branch(branch)
                    } else if let Some((address, width)) =
                        descriptor.prepared_load(opcode, &self.state)
                    {
                        fabric.request(owner, now, Operation::Read { address, width })?;
                        self.phase = Phase::Operand {
                            opcode,
                            states,
                            effect: Effect::Load { width, value: 0 },
                        };
                        return Ok(());
                    } else if let Some(address) = timing::mov_store_address(opcode, &self.state) {
                        // STS.L and other system-register memory instructions are outside this slice.
                        if opcode & 0xF0FF == 0x4022 {
                            return Err(self.unsupported(opcode));
                        }
                        let n = usize::from(opcode >> 8 & 15);
                        let m = usize::from(opcode >> 4 & 15);
                        let width = if opcode >> 12 == 1 {
                            4
                        } else if opcode >> 12 == 8 {
                            1 << (opcode >> 8 & 1)
                        } else if opcode >> 12 == 0xC {
                            1 << (opcode >> 8 & 3)
                        } else {
                            1 << (opcode & 3)
                        };
                        let value = self.state.r[if matches!(opcode >> 12, 8 | 0xC) {
                            0
                        } else {
                            m
                        }];
                        let update = (opcode >> 12 == 2 && opcode & 4 != 0).then_some((n, address));
                        // Address and payload are latched before granting the transfer.
                        fabric.request(
                            owner,
                            now,
                            Operation::Write {
                                address,
                                width,
                                value,
                            },
                        )?;
                        self.phase = Phase::Operand {
                            opcode,
                            states,
                            effect: Effect::Store { update },
                        };
                        return Ok(());
                    } else if opcode >> 8 == 0xC7 {
                        let pc = self
                            .state
                            .delay_target
                            .map_or(self.state.pc.wrapping_add(4), |v| v.wrapping_add(2));
                        Effect::MoveAddress((pc & !3).wrapping_add(u32::from(opcode as u8) * 4))
                    } else if opcode == 0x001B {
                        Effect::Sleep
                    } else {
                        return Err(self.unsupported(opcode));
                    };
                    self.phase = Phase::Retire {
                        at: now
                            .checked_add(u64::from(states))
                            .ok_or(Error::InvalidTime)?,
                        opcode,
                        effect,
                    };
                    return Ok(());
                }
                Phase::Operand { .. } => {
                    let Some(value) = fabric.reply(owner) else {
                        return Ok(());
                    };
                    let Phase::Operand {
                        opcode,
                        states,
                        mut effect,
                    } = std::mem::replace(&mut self.phase, Phase::Ready)
                    else {
                        unreachable!()
                    };
                    if let Effect::Load {
                        value: destination, ..
                    } = &mut effect
                    {
                        *destination = value;
                    }
                    self.phase = Phase::Retire {
                        at: now
                            .checked_add(u64::from(states))
                            .ok_or(Error::InvalidTime)?,
                        opcode,
                        effect,
                    };
                    return Ok(());
                }
                Phase::Retire { at, .. } if at > now => return Ok(()),
                Phase::Retire { .. } => {
                    let Phase::Retire { opcode, effect, .. } =
                        std::mem::replace(&mut self.phase, Phase::Ready)
                    else {
                        unreachable!()
                    };
                    let mut sequential = self.state.pc.wrapping_add(2);
                    let mut next_branch = None;
                    self.delay_slot = false;
                    match effect {
                        Effect::Scalar(kind) => scalar::execute(kind, opcode, &mut self.state),
                        Effect::Load { width, value } => {
                            let value = match width {
                                1 => value as i8 as i32 as u32,
                                2 => value as i16 as i32 as u32,
                                _ => value,
                            };
                            commit_mov_load(&mut self.state, opcode, value);
                        }
                        Effect::Store { update } => {
                            if let Some((register, value)) = update {
                                self.state.r[register] = value;
                            }
                        }
                        Effect::Branch(branch) => {
                            if branch.link {
                                self.state.pr = self.state.pc.wrapping_add(4);
                            }
                            self.delay_slot = branch.delayed;
                            if let Some(target) = branch.target {
                                if branch.delayed {
                                    next_branch = Some(target);
                                } else {
                                    sequential = target;
                                }
                            }
                        }
                        Effect::MoveAddress(value) => self.state.r[0] = value,
                        Effect::Sleep => self.state.status = CpuStatus::Sleeping,
                    }
                    self.state.pc = self.state.delay_target.take().unwrap_or(sequential);
                    self.state.delay_target = next_branch;
                    self.state.instructions += 1;
                    fabric.retired[owner] += 1;
                }
            }
        }
    }

    fn unsupported(&self, opcode: u16) -> Error {
        Error::Unsupported {
            pc: self.state.pc,
            opcode,
        }
    }
}
