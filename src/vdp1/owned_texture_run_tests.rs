//! Compare owned runs against the original one-clock engine at every host cut.
use super::*;

fn command(v: &mut Vdp1, at: usize, values: &[(usize, u16)]) {
    for &(offset, value) in values {
        v.vram[at + offset..at + offset + 2].copy_from_slice(&value.to_be_bytes());
    }
}
fn scene(mode: u16, kind: u16, runs: bool) -> Vdp1 {
    let mut v = Vdp1 {
        owned_texture_runs: runs,
        ..Vdp1::default()
    };
    command(&mut v, 0, &[(0, 9), (20, 63), (22, 31)]);
    command(
        &mut v,
        32,
        &[
            (0, kind),
            (4, mode),
            (6, 0x200),
            (8, 0x100),
            (10, 0x104),
            (12, 2),
            (14, 2),
            (16, 5),
            (18, 3),
            (20, 4),
            (22, 8),
            (24, 1),
            (26, 6),
            (28, 0x300),
        ],
    );
    command(&mut v, 64, &[(0, 0x8000)]);
    for (i, word) in v.vram[0x800..0x1000]
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .enumerate()
    {
        word.copy_from_slice(&[0x0123u16, 0xF456, 0x789A, 0xFFFF][i % 4].to_be_bytes());
    }
    for i in 0..16 {
        command(&mut v, 0x1000 + i * 2, &[(0, 0x8000 | (i as u16 * 37))]);
    }
    for i in 0..4 {
        command(&mut v, 0x1800 + i * 2, &[(0, 0x4210 + i as u16)]);
    }
    for buffer in &mut v.buffers {
        for (i, word) in buffer.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            word.copy_from_slice(&(if i % 2 == 0 { 0x801Fu16 } else { 0 }).to_be_bytes());
        }
    }
    v.write16(REGISTERS + 2, 2).unwrap();
    v.write16(REGISTERS + 4, 1).unwrap();
    v
}
fn assert_same(candidate: &Vdp1, reference: &mut Vdp1, context: &str) {
    // Compare every retained field, including partial fetches, sampled pixels,
    // erase/blank budgets, both buffers and the ordered display journal.
    reference.owned_texture_runs = candidate.owned_texture_runs;
    if candidate.phase != reference.phase {
        let summary = |v: &Vdp1| {
            (
                v.phase,
                v.drawing.as_ref().map(|d| {
                    (
                        d.row,
                        d.rows,
                        d.texture,
                        d.line.as_ref().map(|l| (l.dot, l.last)),
                    )
                }),
                v.pending_pixel.as_ref().map(|p| p.clocks),
                v.stop_remaining,
                v.blank_remaining,
            )
        };
        eprintln!(
            "candidate={:?}, reference={:?}",
            summary(candidate),
            summary(reference)
        );
    }
    macro_rules! same_field {
        ($($field:ident),*) => {$(assert!(candidate.$field == reference.$field,
            "VDP1 field {} differs: {context}", stringify!($field));)*};
    }
    same_field!(
        phase,
        running,
        blank_remaining,
        drawing,
        pending_pixel,
        erasing,
        draw,
        edsr,
        lopr,
        copr,
        command_words,
        command_read,
        stop_remaining,
        display_journal,
        buffers,
        vram
    );
    assert!(
        candidate == &*reference,
        "full VDP1 state differs: {context}"
    );
    reference.owned_texture_runs = false;
}
#[test]
fn owned_run_crosses_real_texture_fetches_with_the_original_clock_count() {
    let mut candidate = scene(0xC0, 0, true);
    let mut reference = scene(0xC0, 0, false);
    for v in [&mut candidate, &mut reference] {
        v.vblank = true;
        v.blank_remaining = 200;
    }
    candidate.advance(64).unwrap();
    reference.advance(64).unwrap();
    assert!(candidate.drawing.as_ref().unwrap().texture_pending(0));
    assert_eq!(candidate.try_draw_run(23, true, false), 0);
    assert_eq!(candidate.try_draw_run(23, false, true), 0);
    assert_eq!(candidate.try_draw_run(23, false, false), 23);
    for _ in 0..23 {
        reference.advance(1).unwrap();
    }
    assert_same(
        &candidate,
        &mut reference,
        "23 clocks across retained-word boundaries",
    );
}
#[test]
fn owned_runs_match_one_clock_execution_across_ports_modes_and_control_cuts() {
    for color in 0..=5 {
        for effect in [0, 1, 2, 3, 4, 6, 7, 0xC0] {
            for kind in [0, 0x32] {
                let mode = (color << 3)
                    | effect
                    | if kind & 15 == 2 && color % 2 == 0 {
                        0x1000
                    } else {
                        0
                    };
                let mut candidate = scene(mode, kind, true);
                let mut reference = scene(mode, kind, false);
                let mut completed = false;
                for cut in 0..128 {
                    let clocks = [1, 2, 3, 5, 17, 65, 127][cut % 7];
                    let vram_busy = cut % 4 == 2;
                    let framebuffer_busy = cut % 9 == 7;
                    if cut == 6 {
                        for v in [&mut candidate, &mut reference] {
                            for word in v.vram[0x800..0x880].as_chunks_mut::<2>().0 {
                                word.copy_from_slice(&0x8ABCu16.to_be_bytes());
                            }
                        }
                    }
                    if cut == 7 && effect & 1 != 0 {
                        for v in [&mut candidate, &mut reference] {
                            v.write16(REGISTERS + 0x0C, 0).unwrap();
                        }
                    }
                    if cut == 9 {
                        for v in [&mut candidate, &mut reference] {
                            v.hblank_in();
                            v.vblank_in(1408, 38);
                        }
                    }
                    let actual = candidate
                        .advance_with_bus(clocks, vram_busy, framebuffer_busy)
                        .unwrap();
                    let mut expected = false;
                    for _ in 0..clocks {
                        expected |= reference
                            .advance_with_bus(1, vram_busy, framebuffer_busy)
                            .unwrap();
                    }
                    assert_eq!(actual, expected, "mode={mode:X},kind={kind:X},cut={cut}");
                    assert_same(
                        &candidate,
                        &mut reference,
                        &format!("mode={mode:X},kind={kind:X},cut={cut}"),
                    );
                    if !candidate.busy() {
                        completed = true;
                        break;
                    }
                }
                assert!(completed, "bounded original command must finish");
            }
        }
    }
}

#[test]
fn endr_keeps_the_elapsed_prefix_before_its_last_clock() {
    for runs in [false, true] {
        let mut v = scene(0xC0, 0, runs);
        // Clip command (32) plus two sprite words (4), then ENDR's 30 clocks.
        // Sprite decode consumes the first 28. One texture clock precedes stop.
        v.advance(36).unwrap();
        v.write16(REGISTERS + 12, 0).unwrap();
        assert!(!v.advance(30).unwrap());
        assert!(!v.running);
        assert_eq!(v.phase, 1);
        assert_eq!(v.edsr & 2, 0);
        assert!(v.drawing.is_none());
    }
}
