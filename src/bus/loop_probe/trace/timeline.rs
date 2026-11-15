//! Compact chronological metadata for an actually observed CPU period.
pub(in crate::bus) struct Timeline {
    pub(in crate::bus) clocks: Vec<u64>,
    mask_end: Vec<usize>,
    gap_levels: Vec<Vec<u64>>,
}
impl Timeline {
    pub(super) fn new(points: impl IntoIterator<Item = (u64, u32)>) -> Option<Self> {
        let points: Vec<_> = points.into_iter().collect();
        if points.is_empty() || points.windows(2).any(|p| p[0].0 > p[1].0) {
            return None;
        }
        let clocks = points.iter().map(|&(at, _)| at).collect();
        let mut mask_end = vec![points.len() - 1; points.len()];
        for i in (0..points.len() - 1).rev() {
            mask_end[i] = if (points[i].1 ^ points[i + 1].1) & 0xF0 != 0 {
                i
            } else {
                mask_end[i + 1]
            };
        }
        let gaps: Vec<_> = points.windows(2).map(|p| p[1].0 - p[0].0).collect();
        let n = gaps.len();
        let mut gap_levels = vec![gaps];
        let mut half = 1;
        while half <= n / 2 {
            let previous = gap_levels.last().unwrap();
            let next = (0..=n - half * 2)
                .map(|i| previous[i].max(previous[i + half]))
                .collect();
            gap_levels.push(next);
            half *= 2;
        }
        Some(Self {
            clocks,
            mask_end,
            gap_levels,
        })
    }
    pub(in crate::bus) fn retirement_gap(&self, first: usize, last: usize) -> u64 {
        if last.saturating_sub(first) <= 1 {
            return 0;
        }
        // Only gaps between the retained retirements: the first retirement's
        // distance from the live board clock is supplied by the caller.
        let start = first + 1;
        let n = last - start;
        let level = n.ilog2() as usize;
        let width = 1usize << level;
        self.gap_levels[level][start].max(self.gap_levels[level][last - width])
    }
    pub(in crate::bus) fn prefix_end(
        &self,
        first: usize,
        count: u32,
        end: u64,
        shift: u64,
    ) -> Option<usize> {
        // Keep overflow outside every retained phase, including its metadata.
        self.clocks.last()?.checked_add(shift)?;
        let through = end.checked_sub(shift)?;
        let last = self
            .clocks
            .partition_point(|&at| at <= through)
            .saturating_sub(1)
            .min(first.saturating_add(count as usize))
            .min(*self.mask_end.get(first)?);
        (last > first).then_some(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_prefix_matches_literal_count_clock_and_interrupt_boundaries() {
        let points = [
            (0, 0),
            (1, 1),
            (1, 0),
            (3, 0),
            (4, 0x10),
            (6, 0x11),
            (6, 0x10),
            (9, 0),
            (11, 2),
            (12, 0x20),
            (14, 0x20),
        ];
        let timeline = Timeline::new(points).unwrap();
        for first in 0..points.len() {
            for count in [0, 1, 2, 3, 5, 17, u32::MAX] {
                for shift in [0, 1, 17] {
                    for end in 0..=40 {
                        let mut expected = first;
                        for (index, &(at, sr)) in points.iter().enumerate().skip(first + 1) {
                            if index - first > count as usize
                                || at + shift > end
                                || (sr ^ points[first].1) & 0xF0 != 0
                            {
                                break;
                            }
                            expected = index;
                        }
                        assert_eq!(
                            timeline.prefix_end(first, count, end, shift),
                            (expected > first).then_some(expected)
                        );
                    }
                }
            }
        }
        for first in 0..points.len() {
            for last in first..points.len() {
                let expected = points[first.saturating_add(1).min(last)..=last]
                    .windows(2)
                    .map(|p| p[1].0 - p[0].0)
                    .max()
                    .unwrap_or(0);
                assert_eq!(timeline.retirement_gap(first, last), expected);
            }
        }
        assert!(Timeline::new([]).is_none());
        assert!(Timeline::new([(2, 0), (1, 0)]).is_none());
        assert!(timeline.prefix_end(0, 1, u64::MAX, u64::MAX).is_none());
    }
}
