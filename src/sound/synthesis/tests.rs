//! Independently authored vectors from ST-077 §4.2. No reference emulator.
use super::*;

#[test]
fn binary_total_level_attenuation_and_fractional_gain_match_mathematical_vectors() {
    for (tl, expected) in [
        (0, 16384),
        (8, 11585),
        (16, 8192),
        (32, 4096),
        (64, 1024),
        (128, 64),
        (240, 0),
    ] {
        assert!(
            (i32::from(attenuate(16384, tl, 0)) - expected).abs() <= 1,
            "TL {tl}"
        );
        assert!(
            (i32::from(attenuate(-16384, tl, 0)) + expected).abs() <= 1,
            "negative TL {tl}"
        );
    }
    assert_eq!(attenuate(8192, 0, 16 * 256), 4096);
    assert_eq!(attenuate(8192, 16, 16 * 256), 2048);
    for tl in 0..=255 {
        let expected = 16384.0 * 2f64.powf(-f64::from(tl) / 16.0);
        assert!((f64::from(attenuate(16384, tl, 0)) - expected).abs() < 1.01);
    }
}

#[test]
fn lfo_shapes_have_authored_quarter_cycle_values_and_independent_sensitivities() {
    let mut lfo = Lfo::new(0);
    for (phase, am, pm) in [
        (0, [0, 0, 0], [0, 127, 0]),
        (64, [64, 0, 128], [64, 127, 127]),
        (128, [128, 255, 254], [-128, -128, -1]),
        (192, [192, 255, 126], [-64, -128, -127]),
    ] {
        lfo.phase = phase * 4_410_000 / 256;
        for shape in 0..3 {
            assert_eq!(lfo.amplitude(shape), am[shape as usize]);
            assert_eq!(lfo.pitch(shape), pm[shape as usize]);
        }
    }
    lfo.phase = 4_410_000 / 2; // negative peak for pitch, half attenuation for saw AM
    assert_eq!(lfo.attenuation(0), 0);
    assert_eq!(lfo.attenuation(7), 128 * 64);
    assert_eq!(lfo.step(0, 65536), 65536);
    // -494 cents: mathematical ratio 2^(-494/1200), table 4.24.
    assert!((lfo.step(7 << 5, 65536) - 49267).abs() <= 2);
    lfo.phase = 0;
    assert!((lfo.step((1 << 8) | (7 << 5), 65536) - 86983).abs() <= 2);
}

#[test]
fn all_lfo_frequencies_carry_remainders_and_reset_holds_only_tonal_oscillator() {
    for (setting, hundredths) in [(0, 17), (8, 68), (16, 287), (24, 1440), (31, 17230)] {
        let mut lfo = Lfo::new(0);
        let mut elapsed = 0;
        while elapsed < 44100 {
            lfo.advance(setting << 10);
            elapsed += 1;
        }
        assert_eq!(lfo.phase, (hundredths * 44100) % 4_410_000);
        let noise = lfo.noise;
        lfo.reset();
        assert_eq!(lfo.phase, 0);
        assert_eq!(lfo.noise, noise);
        for _ in 0..100 {
            lfo.advance(0x8000 | (setting << 10));
            assert_eq!(lfo.phase, 0);
        }
        assert_ne!(lfo.noise, noise);
    }
    // Original deterministic noise policy: signed, nonzero, maximal period.
    let mut lfo = Lfo::new(0);
    let initial = lfo.noise;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..65535 {
        assert_ne!(lfo.noise, 0);
        assert!(seen.insert(lfo.noise));
        lfo.advance(0x8000);
    }
    assert_eq!(lfo.noise, initial);
}

#[test]
fn modulation_uses_recent_or_past_generation_after_five_slot_pipeline_clocks() {
    let mut stack = Stack::default();
    // Each output encodes (sample,slot) so all carriers/selectors and both
    // generations can be distinguished independently of the stack's storage.
    for frame in 0..5 {
        for carrier in 0..32 {
            stack.begin_slot(carrier);
            if frame >= 3 {
                for select in 0..64u16 {
                    let source = (carrier + usize::from(select)) % 32;
                    let distance = (carrier + 32 - source) % 32;
                    let newest_frame = if source + 5 <= carrier {
                        frame
                    } else if source < carrier + 28 {
                        frame - 1
                    } else {
                        frame - 2
                    };
                    let latest = (0x1C..=0x3B).contains(&select);
                    let expected_frame = newest_frame - usize::from(!latest);
                    assert_eq!(
                        stack.read(carrier, select),
                        (expected_frame * 100 + source) as i64,
                        "frame={frame} carrier={carrier} select={select:02x} distance={distance}"
                    );
                }
            }
            stack.write(carrier, (frame * 100 + carrier) as i16, false);
        }
        stack.end_frame();
    }
    // Known modulator samples and table 4.17 gains, signed average, rollover.
    let mut stack = Stack::default();
    stack.data[0] = 16384;
    stack.data[1] = -8192;
    for level in 0..=15 {
        let value = stack.displacement(0, (level << 12) | (0x20 << 6) | 0x21);
        let expected = if level <= 4 {
            0
        } else {
            (4096i64 << (level + 1)) % (1024 << 16)
        };
        assert_eq!(value, expected);
    }
    stack.write(0, 123, false);
    for slot in 1..5 {
        stack.begin_slot(slot);
    }
    assert_eq!(stack.data[0], 16384);
    stack.begin_slot(5);
    assert_eq!(stack.data[0], 123);
    stack.write(0, 999, true);
    stack.begin_slot(5);
    assert_eq!(stack.data[0], 123); // STWINH preserves committed data.
}
