//! Ordinary MOV store latch, captured before any auto-update or buffer grant.
use super::*;
use crate::bus::{HIGH_WRAM, Memory};

pub(super) struct Store {
    pub address: u32,
    pub width: u32,
    pub value: u32,
    pub update: Option<(usize, u32)>,
}
impl Store {
    pub fn decode(opcode: u16, state: &CpuState) -> Option<Self> {
        // System-register memory transfers have separate MDU/control timing.
        if opcode >> 12 == 4 {
            return None;
        }
        let address = timing::mov_store_address(opcode, state)?;
        let width = match opcode >> 12 {
            0 | 2 => 1 << (opcode & 3),
            1 => 4,
            8 => 1 << (opcode >> 8 & 1),
            0xC => 1 << (opcode >> 8 & 3),
            _ => return None,
        };
        let source = if matches!(opcode >> 12, 0..=2) {
            (opcode >> 4 & 15) as usize
        } else {
            0
        };
        let update = (opcode >> 12 == 2 && opcode & 4 != 0)
            .then_some(((opcode >> 8 & 15) as usize, address));
        Some(Self {
            address,
            width,
            value: state.r[source],
            update,
        })
    }
}

/// Original owned-store/production-board differential, also callable in Wasm.
pub fn verify_board() -> Vec<u32> {
    let mut count = 0;
    let mut hash = 2166136261u32;
    for owner in [Processor::Master, Processor::Slave] {
        for alias in [0, 0x20000000] {
            for (select, area2_little) in [(1, false), (2, false), (3, false), (2, true), (3, true)]
            {
                for control in [0, 1] {
                    for waits in [0, 0xC0] {
                        for offset in [0, 2] {
                            for opcode in [
                                0x2420, 0x2421, 0x2422, 0x2424, 0x2425, 0x2426, 0x0424, 0x0425,
                                0x0426, 0x1421, 0x8041, 0x8141, 0xC001, 0xC101, 0xC201, 0x2446,
                            ] {
                                let setup = || {
                                    let (mut bus, mut state) = fixture::board(
                                        owner, control, select, waits, alias, offset,
                                    );
                                    state.r[0] = 4;
                                    state.r[2] = 0xA1B2C3D4;
                                    state.r[4] = HIGH_WRAM + 0x2414 + alias;
                                    state.gbr = state.r[4];
                                    let load = Store::decode(opcode, &state)
                                        .unwrap()
                                        .width
                                        .trailing_zeros()
                                        as u16;
                                    for (i, word) in [
                                        0x6342,
                                        opcode,
                                        0x6640 | load,
                                        opcode,
                                        0x6740 | load,
                                        9,
                                        9,
                                        9,
                                        9,
                                        9,
                                        9,
                                        9,
                                    ]
                                    .into_iter()
                                    .enumerate()
                                    {
                                        bus.write16(HIGH_WRAM + offset + i as u32 * 2, word)
                                            .unwrap();
                                    }
                                    if area2_little {
                                        bus.for_cpu(owner).write32(0xFFFFFFE0, 0xA55A13F0).unwrap();
                                        // H §7.3.2/BCR1: conversion applies only to CS2.
                                        // This control must not swap WRAM code or data.
                                    }
                                    (bus, state)
                                };
                                let (actual, state) = setup();
                                let (reference, _) = setup();
                                let value =
                                    fixture::compare_pair(actual, reference, state, owner, control);
                                hash = (hash ^ value).wrapping_mul(16777619);
                                count += 2;
                            }
                        }
                    }
                }
            }
        }
    }
    vec![count, hash]
}
