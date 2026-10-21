//! ST-097 §4.5 pp.134/136/138/140: B-bus words are two 16-bit cycles.
//! ST-210 #27 restricts B-bus input to add2. These vectors check bus
//! transactions independently of the DSP transfer implementation.
use super::*;
use crate::scu::SCU;
use std::cell::RefCell;

#[derive(Default)]
struct Port {
    cycles: RefCell<Vec<(bool, u32, u32)>>,
    read_value: Cell<u16>,
    fail: Option<u32>,
}
impl Memory for Port {
    fn read8(&self, _: u32) -> Result<u8, BusError> {
        panic!("unexpected byte read")
    }
    fn read16(&self, address: u32) -> Result<u16, BusError> {
        self.cycles
            .borrow_mut()
            .push((false, address, u32::from(self.read_value.get())));
        if self.fail == Some(address) {
            return Err(BusError::Unmapped { address, width: 2 });
        }
        Ok(self.read_value.get())
    }
    fn read32(&self, _: u32) -> Result<u32, BusError> {
        panic!("unexpected longword read")
    }
    fn write8(&mut self, _: u32, _: u8) -> Result<(), BusError> {
        panic!("unexpected byte write")
    }
    fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        self.cycles
            .borrow_mut()
            .push((true, address, u32::from(value)));
        if self.fail == Some(address) {
            return Err(BusError::Unmapped { address, width: 2 });
        }
        Ok(())
    }
    fn write32(&mut self, _: u32, _: u32) -> Result<(), BusError> {
        panic!("unexpected longword write")
    }
}

fn start(write: bool, hold: bool, add_code: u32, count: u32, address: u32) -> Dsp {
    let mut d = Dsp::default();
    // Write MC0 at CT0=63, then the wrapped word at CT0=0.
    d.set_data_address(63, SCU + 0x88).unwrap();
    d.write_data(0x11223344, SCU + 0x8c).unwrap();
    d.write_data(0xaabbccdd, SCU + 0x8c).unwrap();
    for word in [
        0x00001c3f, // MOV #63,CT0
        (if write { 0x9c000000 } else { 0x98000000 }) | (address / 4),
        0xc0000000 | u32::from(write) << 12 | u32::from(hold) << 14 | add_code << 15 | count,
        0xf8000000,
    ] {
        d.write_program(word, SCU + 0x84).unwrap();
    }
    d.set_control(0x18000);
    for _ in 0..3 {
        assert!(!d.step().unwrap());
    }
    assert!(d.dma_pending());
    d
}

#[test]
fn b_bus_output_uses_separate_grants_for_every_stride_and_hold_setting() {
    for (code, stride) in [
        (0, 0),
        (1, 2),
        (2, 4),
        (3, 8),
        (4, 16),
        (5, 32),
        (6, 64),
        (7, 128),
    ] {
        for hold in [false, true] {
            let mut d = start(true, hold, code, 2, 0x05c01000);
            let mut p = Port::default();
            assert!(d.step().unwrap()); // ENDI stops execution, not the DMA.
            for (beat, address, value) in [
                (0, 0x05c01000, 0x1122),
                (1, 0x05c01000 + stride, 0x3344),
                (2, 0x05c01000 + stride * 2, 0xaabb),
                (3, 0x05c01000 + stride * 3, 0xccdd),
            ] {
                d.dma_beat(&mut p).unwrap();
                assert_eq!(p.cycles.borrow().len(), beat + 1);
                assert_eq!(p.cycles.borrow()[beat], (true, address, value));
                assert_eq!(
                    d.ct[0],
                    if beat == 0 {
                        63
                    } else if beat < 3 {
                        0
                    } else {
                        1
                    }
                );
                assert_eq!(d.dma_pending(), beat != 3);
            }
            assert_eq!(
                d.wa0,
                if hold {
                    0xff700400
                } else {
                    (0x05c01000 + stride * 4) / 4
                }
            );
            d.dma_beat(&mut p).unwrap();
            assert_eq!(p.cycles.borrow().len(), 4);
        }
    }
}

#[test]
fn b_bus_output_latches_both_halves_before_a_later_dsp_ram_write() {
    let mut d = start(true, false, 1, 1, 0x05c01000);
    let mut p = Port::default();
    d.dma_beat(&mut p).unwrap();
    d.data[0][63] = 0xdeadbeef;
    d.dma_beat(&mut p).unwrap();
    assert_eq!(
        *p.cycles.borrow(),
        [(true, 0x05c01000, 0x1122), (true, 0x05c01002, 0x3344)]
    );
}

#[test]
fn b_bus_input_publishes_only_a_complete_word_and_retains_the_first_sample() {
    let mut d = start(false, false, 2, 2, 0x05a01000);
    let mut p = Port::default();
    for (beat, sample) in [0x1234, 0x5678, 0x9abc, 0xdef0].into_iter().enumerate() {
        p.read_value.set(sample);
        d.dma_beat(&mut p).unwrap();
        assert_eq!(p.cycles.borrow().len(), beat + 1);
        assert_eq!(
            d.data[0][63],
            if beat == 0 { 0x11223344 } else { 0x12345678 }
        );
        assert_eq!(d.data[0][0], if beat < 3 { 0xaabbccdd } else { 0x9abcdef0 });
        assert_eq!(
            d.ct[0],
            if beat == 0 {
                63
            } else if beat < 3 {
                0
            } else {
                1
            }
        );
        assert_eq!(
            d.ra0,
            if beat == 0 {
                0xff680400 // The MVI's signed 25-bit immediate, not yet updated.
            } else if beat < 3 {
                0x01680402
            } else {
                0x01680404
            }
        );
    }
    assert_eq!(
        *p.cycles.borrow(),
        [
            (false, 0x05a01000, 0x1234),
            (false, 0x05a01002, 0x5678),
            (false, 0x05a01008, 0x9abc),
            (false, 0x05a0100a, 0xdef0),
        ]
    );
    assert!(!d.dma_pending());
}

#[test]
fn b_bus_second_cycle_fault_preserves_completed_cycle_and_transfer_registers() {
    for write in [false, true] {
        let base = if write { 0x05c01000 } else { 0x05a01000 };
        let mut d = start(write, false, if write { 1 } else { 2 }, 1, base);
        let mut p = Port {
            fail: Some(base + 2),
            ..Port::default()
        };
        p.read_value.set(0x1234);
        d.dma_beat(&mut p).unwrap();
        assert_eq!(
            d.dma_beat(&mut p),
            Err(ScuError::Bus(BusError::Unmapped {
                address: base + 2,
                width: 2
            }))
        );
        assert_eq!(p.cycles.borrow().len(), 2);
        assert_eq!(d.ct[0], 63);
        assert_eq!(d.dma.as_ref().unwrap().remaining, 1);
        assert!(d.dma_pending());
        assert_eq!(d.data[0][63], 0x11223344);
        assert_eq!(if write { d.wa0 } else { d.ra0 }, (base / 4) | 0xfe000000);
    }
}

#[test]
fn b_bus_output_validates_the_second_cycle_before_accessing_a_new_region() {
    let mut d = start(true, false, 7, 1, 0x05fbfffc);
    let mut p = Port::default();
    d.dma_beat(&mut p).unwrap();
    assert_eq!(
        d.dma_beat(&mut p),
        Err(ScuError::DmaParameters { channel: 3 })
    );
    assert_eq!(*p.cycles.borrow(), [(true, 0x05fbfffc, 0x1122)]);
    assert_eq!(d.ct[0], 63);
    assert!(d.dma_pending());
}

#[test]
fn reset_cancels_an_incomplete_b_bus_word() {
    let mut d = start(true, false, 1, 1, 0x05c01000);
    let mut p = Port::default();
    d.dma_beat(&mut p).unwrap();
    d = Dsp::default();
    d.dma_beat(&mut p).unwrap();
    assert_eq!(*p.cycles.borrow(), [(true, 0x05c01000, 0x1122)]);
    assert_eq!(d.control() & 0x810000, 0);
}

#[test]
fn program_dma_cannot_replace_the_prefetched_successor_and_le_refetches_it() {
    use crate::bus::{Bus, HIGH_WRAM};
    let mut d = Dsp::default();
    let mut memory = Bus::default();
    // Register-count DMA into PRAM. Its third word replaces the already
    // fetched MOV #7,MC1 with MOV #9,MC1 before the execution clock.
    d.set_data_address(0, SCU + 0x88).unwrap();
    d.write_data(3, SCU + 0x8c).unwrap();
    for (i, word) in [0, 0, 0x00001109].into_iter().enumerate() {
        memory
            .write32(HIGH_WRAM + 0x1000 + i as u32 * 4, word)
            .unwrap();
    }
    for word in [0x99800400, 0xc000a400, 0x00001107, 0xf8000000] {
        d.write_program(word, SCU + 0x84).unwrap();
    }
    d.set_control(0x18000);
    d.step().unwrap();
    d.step().unwrap();
    for _ in 0..3 {
        d.dma_beat(&mut memory).unwrap();
    }
    // Pause does not discard a fetched instruction.
    d.set_control(0x2010000);
    d.step().unwrap();
    assert_eq!(d.pc, 2);
    d.set_control(0x4010000);
    d.step().unwrap();
    d.step().unwrap();
    assert_eq!(d.data[1][0], 7);
    // Loading PC explicitly discards the old pipeline and reads the changed
    // instruction from PRAM on the next real execution clock.
    d.set_control(0x18002);
    d.step().unwrap();
    d.step().unwrap();
    assert_eq!(d.data[1][1], 9);
}

#[test]
fn program_dma_wraps_the_host_load_address_and_loaded_code_executes_across_ff_to_zero() {
    use crate::bus::{Bus, HIGH_WRAM};
    let mut d = Dsp::default();
    let mut memory = Bus::default();
    d.set_data_address(0, SCU + 0x88).unwrap();
    d.write_data(2, SCU + 0x8c).unwrap();
    for (i, word) in [0x0000112a, 0xf8000000].into_iter().enumerate() {
        memory
            .write32(HIGH_WRAM + 0x1000 + i as u32 * 4, word)
            .unwrap();
    }
    d.set_control(0x80fc);
    for word in [0x99800400, 0xc000a400, 0xf0000000] {
        d.write_program(word, SCU + 0x84).unwrap();
    }
    d.set_control(0x180fc);
    d.step().unwrap();
    d.step().unwrap();
    // LE writes are ignored while EX=1. The host load address was set to FC
    // when execution began; loading it to FF requires a stopped write below.
    d.set_control(0);
    d.set_control(0xff); // Change load port without changing stopped PC FE.
    d.dma_beat(&mut memory).unwrap();
    d.dma_beat(&mut memory).unwrap();
    assert_eq!(d.load_address, 1);
    assert_eq!(d.program[255], 0x0000112a);
    assert_eq!(d.program[0], 0xf8000000);
    d.set_control(0x180ff);
    d.step().unwrap();
    assert_eq!(d.pc, 0);
    assert_eq!(d.data[1][0], 42);
    assert!(d.step().unwrap());
    assert!(!d.running());
}
