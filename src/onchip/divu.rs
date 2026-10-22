//! SH7604 §10, signed 32/32 and 64/32 division with 39/6-clock completion.
#[derive(Default)]
pub(super) struct Division {
    divisor: u32,
    high: u32,
    low: u32,
    pub control: u32,
    pub vector: u32,
    pending: Option<(u32, u32, u32, bool)>,
}
impl Division {
    pub fn read(&self, offset: u32) -> Option<u32> {
        Some(match offset {
            0 => self.divisor,
            4 | 0x14 => self.low,
            8 => self.control,
            0xC => self.vector,
            0x10 => self.high,
            _ => return None,
        })
    }
    pub fn write(&mut self, offset: u32, value: u32) -> bool {
        // Bus accesses during an active divide interlock until its completion.
        self.finish();
        match offset {
            0 => self.divisor = value,
            4 => {
                self.high = if (value as i32) < 0 { u32::MAX } else { 0 };
                self.low = value;
                self.start();
            }
            8 => self.control = value & 3,
            0xC => self.vector = value & 0x7F,
            0x10 => self.high = value,
            0x14 => {
                self.low = value;
                self.start();
            }
            _ => return false,
        }
        true
    }
    fn start(&mut self) {
        let dividend = ((u64::from(self.high) << 32) | u64::from(self.low)) as i64;
        let divisor = i64::from(self.divisor as i32);
        let quotient = dividend.checked_div(divisor);
        let remainder = dividend.checked_rem(divisor);
        // §10.3.3 includes the negative/negative maximum-quotient-with-remainder
        // boundary in addition to a quotient outside the signed 32-bit range.
        let overflow = quotient.is_none_or(|q| {
            !(i64::from(i32::MIN)..=i64::from(i32::MAX)).contains(&q)
                || (q == i64::from(i32::MAX) && dividend < 0 && divisor < 0 && remainder != Some(0))
        });
        let negative = (dividend < 0) != (divisor < 0);
        let (q, r) = if overflow {
            // H §10.3.3/table 10.2: three setup clocks followed by three
            // division clocks. DVDNTH always exposes the partial remainder;
            // only DVDNT(L) saturates, and only with overflow IRQs disabled.
            let (partial_q, partial_r) = self.overflow_result();
            let q = if self.control & 2 != 0 {
                partial_q
            } else if negative {
                0x8000_0000
            } else {
                0x7FFF_FFFF
            };
            (q, partial_r)
        } else {
            (quotient.unwrap() as u32, remainder.unwrap() as u32)
        };
        self.pending = Some((if overflow { 6 } else { 39 }, q, r, overflow));
    }
    fn overflow_result(&self) -> (u32, u32) {
        // Signed non-restoring steps (P §6.19). Keep the extra remainder
        // sign in Q: narrowing an overflowing add/subtract before deciding
        // the next operation would lose it. Append each resulting quotient
        // bit directly; unlike ROTCL/DIV1, the hardware has no exposed T lag.
        let mut low = self.low;
        let mut high = self.high;
        let m = self.divisor >> 31 != 0;
        let mut q = high >> 31 != 0;
        for _ in 0..3 {
            let sign = high >> 31 != 0;
            let shifted = (high << 1) | (low >> 31);
            let (result, carry) = if q == m {
                shifted.overflowing_sub(self.divisor)
            } else {
                shifted.overflowing_add(self.divisor)
            };
            q = sign ^ carry ^ m;
            high = result;
            low = (low << 1) | u32::from(q == m);
        }
        (low, high)
    }
    pub fn advance(&mut self, clocks: u32) {
        if let Some((remaining, q, r, overflow)) = self.pending {
            if clocks < remaining {
                self.pending = Some((remaining - clocks, q, r, overflow));
            } else {
                self.finish();
            }
        }
    }
    pub fn finish(&mut self) {
        if let Some((_, q, r, overflow)) = self.pending.take() {
            self.low = q;
            self.high = r;
            self.control |= u32::from(overflow);
        }
    }
    pub fn remaining(&self) -> u32 {
        self.pending.map_or(0, |p| p.0)
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn irq(&self) -> bool {
        self.control & 3 == 3
    }
    pub fn reset(&mut self) {
        self.control = 0;
        self.pending = None;
    }
}
