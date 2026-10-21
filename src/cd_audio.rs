//! CD-DA 50/15 µs de-emphasis (IEC 60908 audio CONTROL bit 0).
//! H(s)=(1+15e-6*s)/(1+50e-6*s). A bilinear transform at 44.1 kHz
//! gives (2323-323*z^-1)/(5410-3410*z^-1). Carry Q24 history so
//! rounding does not accumulate DC loss; native and Wasm use the same integers.
#[derive(Default)]
pub(crate) struct Deemphasis {
    input: [i16; 2],
    output: [i64; 2],
}
impl Deemphasis {
    pub fn sample(&mut self, input: [i16; 2]) -> [i16; 2] {
        std::array::from_fn(|channel| {
            let feed =
                (2323 * i64::from(input[channel]) - 323 * i64::from(self.input[channel])) << 24;
            let output = (feed + 3410 * self.output[channel]) / 5410;
            self.input[channel] = input[channel];
            self.output[channel] = output;
            ((output + (1 << 23)) >> 24).clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stereo_impulse_dc_and_nyquist_match_independent_transfer_function() {
        let mut filter = Deemphasis::default();
        let mut reference = [0f64; 2];
        let mut previous = [0f64; 2];
        for index in 0..4096 {
            let input = [
                if index < 2048 { 16384 } else { 0 },
                if index % 2 == 0 { 16384 } else { -16384 },
            ];
            let actual = filter.sample(input);
            for channel in 0..2 {
                // Independently derived bilinear transform in SI time constants.
                let x = f64::from(input[channel]);
                let y = ((1.0 + 2.0 * 44100.0 * 15e-6) * x
                    + (1.0 - 2.0 * 44100.0 * 15e-6) * previous[channel]
                    - (1.0 - 2.0 * 44100.0 * 50e-6) * reference[channel])
                    / (1.0 + 2.0 * 44100.0 * 50e-6);
                assert!((f64::from(actual[channel]) - y).abs() < 0.51);
                previous[channel] = x;
                reference[channel] = y;
            }
            if index == 1024 {
                assert_eq!(actual, [16384, 4915]); // DC unity; Nyquist gain 15/50.
            }
        }
    }
}
