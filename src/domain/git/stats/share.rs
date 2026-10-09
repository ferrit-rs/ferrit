//! Numbers as shares (`docs/PLAN_13_DASHBOARD.md`, "Numbers as shares"): pure
//! functions, no git. Every share keeps its count next to its percentage so
//! the screen can show either (`n` swaps them). The "under 20 items, show
//! counts" rule is presentation and lives with the screen.

/// One part of a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Share {
    /// The part, in the unit of the whole (commits, lines).
    pub count: u64,
    /// Whole percent, or `None` when the whole is zero (the screen shows `–`,
    /// never a percentage of nothing).
    pub percent: Option<u8>,
    /// A non-empty part that rounds to 0 %: the screen shows `<1 %`, never `0 %`.
    pub under_one: bool,
}

impl Share {
    fn new(count: u64, percent: Option<u8>) -> Self {
        Self {
            count,
            percent,
            under_one: count > 0 && percent == Some(0),
        }
    }

    /// `count` out of `whole`, rounded to the nearest percent. For a part that
    /// is not one slice of a partition (hot files: the share of the commits
    /// touching a file), where shares need not add to 100.
    pub fn of(count: u64, whole: u64) -> Self {
        if whole == 0 {
            return Self::new(count, None);
        }
        let percent = (count.saturating_mul(100).saturating_add(whole / 2) / whole).min(100);
        Self::new(count, u8::try_from(percent).ok())
    }
}

/// The parts of one whole, in the order given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shares {
    /// The whole the parts are taken of.
    pub total: u64,
    /// The parts, in the order given.
    pub items: Vec<Share>,
}

/// Split `counts` into percentages by the largest-remainder method, so the
/// displayed shares of one chart add to exactly 100 (unless the total is zero,
/// where every percent is `None`). Ties go to the larger count, then the
/// earlier item.
pub fn shares(counts: &[u64]) -> Shares {
    let total: u64 = counts.iter().sum();
    if total == 0 {
        return Shares {
            total,
            items: counts
                .iter()
                .map(|&count| Share::new(count, None))
                .collect(),
        };
    }
    let mut percents: Vec<u64> = counts.iter().map(|&c| c * 100 / total).collect();
    let mut order: Vec<usize> = (0..counts.len()).collect();
    let remainder = |i: usize| counts.get(i).map_or(0, |&c| c * 100 % total);
    let count_of = |i: usize| counts.get(i).copied().unwrap_or(0);
    order.sort_by(|&a, &b| {
        remainder(b)
            .cmp(&remainder(a))
            .then_with(|| count_of(b).cmp(&count_of(a)))
            .then_with(|| a.cmp(&b))
    });
    let left = 100 - percents.iter().sum::<u64>();
    for &i in order.iter().take(usize::try_from(left).unwrap_or(0)) {
        if let Some(p) = percents.get_mut(i) {
            *p += 1;
        }
    }
    Shares {
        total,
        items: counts
            .iter()
            .zip(percents)
            .map(|(&count, p)| Share::new(count, u8::try_from(p).ok()))
            .collect(),
    }
}

/// The slices of a donut or a top-N list after folding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folded<T> {
    /// Largest first.
    pub slices: Vec<(T, u64)>,
    /// What was folded away: the gray "others" slice, `0` when nothing was.
    pub others: u64,
}

impl<T> Folded<T> {
    /// Shares of the whole: every slice then `others` (when non-zero), so the
    /// whole stays the one before the cut ("a top-3 that covers 92 % says
    /// 92 %").
    pub fn shares(&self) -> Shares {
        let mut counts: Vec<u64> = self.slices.iter().map(|(_, c)| *c).collect();
        if self.others > 0 {
            counts.push(self.others);
        }
        shares(&counts)
    }
}

/// Sort `items` by count, largest first (stable), fold every item under
/// `min_percent` of the whole into others, and keep at most `max_slices`
/// drawn pieces counting the others slice (donut: 3 % and 6; top-N lists: 0
/// and 6, so "the top 5 and others").
pub fn fold<T>(mut items: Vec<(T, u64)>, min_percent: u64, max_slices: usize) -> Folded<T> {
    let total: u64 = items.iter().map(|(_, c)| *c).sum();
    items.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let mut others = 0;
    let mut slices: Vec<(T, u64)> = Vec::new();
    for (item, count) in items {
        if count == 0 || count * 100 < min_percent * total {
            others += count;
        } else {
            slices.push((item, count));
        }
    }
    let room = if others > 0 {
        max_slices.saturating_sub(1)
    } else {
        max_slices
    };
    if slices.len() > room {
        // One more slice folds into others, which then takes a slot itself.
        let keep = max_slices.saturating_sub(1);
        others += slices.drain(keep..).map(|(_, c)| c).sum::<u64>();
    }
    Folded { slices, others }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "a failed setup is the assertion in a test"
)]
mod tests {
    use super::{Share, fold, shares};

    fn percents(counts: &[u64]) -> Vec<Option<u8>> {
        shares(counts).items.iter().map(|s| s.percent).collect()
    }

    #[test]
    fn three_equal_parts_add_to_exactly_100() {
        assert_eq!(percents(&[1, 1, 1]), [Some(34), Some(33), Some(33)]);
    }

    #[test]
    fn largest_remainder_beats_plain_rounding() {
        // 33.3 / 33.3 / 33.3 and 5 / 5 / 5 / 5 / 5 / 5 / 5 style sums.
        for counts in [
            vec![7, 7, 7, 7, 7, 7, 7],
            vec![1, 2, 3, 4, 5, 6],
            vec![124, 103, 64, 54, 78],
        ] {
            let s = shares(&counts);
            assert_eq!(
                s.items
                    .iter()
                    .filter_map(|i| i.percent)
                    .map(u64::from)
                    .sum::<u64>(),
                100,
                "{counts:?}"
            );
            assert_eq!(s.total, counts.iter().sum::<u64>());
        }
    }

    #[test]
    fn a_tiny_non_empty_part_is_under_one_not_zero() {
        let s = shares(&[1000, 1]);
        let tiny = s.items.last().copied().unwrap();
        assert_eq!(tiny.percent, Some(0));
        assert!(tiny.under_one);
        assert_eq!(tiny.count, 1);
        assert!(!s.items.first().unwrap().under_one);
    }

    #[test]
    fn an_empty_part_is_zero_and_not_under_one() {
        let s = shares(&[5, 0]);
        assert_eq!(s.items.last().unwrap().percent, Some(0));
        assert!(!s.items.last().unwrap().under_one);
    }

    #[test]
    fn a_zero_total_has_no_percentages() {
        let s = shares(&[0, 0]);
        assert_eq!(s.total, 0);
        assert!(s.items.iter().all(|i| i.percent.is_none() && !i.under_one));
        assert!(shares(&[]).items.is_empty());
        assert_eq!(Share::of(0, 0).percent, None);
    }

    #[test]
    fn one_part_is_the_whole() {
        assert_eq!(percents(&[42]), [Some(100)]);
    }

    #[test]
    fn share_of_rounds_to_the_nearest_percent() {
        assert_eq!(Share::of(1, 5).percent, Some(20));
        assert_eq!(Share::of(1, 3).percent, Some(33));
        assert_eq!(Share::of(2, 3).percent, Some(67));
        assert!(Share::of(1, 1000).under_one);
        assert_eq!(Share::of(9, 9).percent, Some(100));
    }

    #[test]
    fn fold_moves_slices_under_three_percent_into_others() {
        let f = fold(vec![("a", 90), ("b", 8), ("c", 2)], 3, 6);
        assert_eq!(f.slices, [("a", 90), ("b", 8)]);
        assert_eq!(f.others, 2);
    }

    #[test]
    fn fold_keeps_the_whole_so_a_top_n_says_its_real_share() {
        let f = fold(vec![("a", 50), ("b", 30), ("c", 12), ("d", 8)], 0, 4);
        assert_eq!(f.others, 0);
        let f = fold(vec![("a", 50), ("b", 30), ("c", 12), ("d", 8)], 0, 3);
        assert_eq!(f.slices, [("a", 50), ("b", 30)]);
        assert_eq!(f.others, 20);
        let s = f.shares();
        assert_eq!(s.total, 100);
        assert_eq!(
            s.items.iter().map(|i| i.percent).collect::<Vec<_>>(),
            [Some(50), Some(30), Some(20)]
        );
    }

    #[test]
    fn fold_draws_at_most_six_slices_including_others() {
        let items: Vec<(usize, u64)> = (0..10).map(|i| (i, 100 - i as u64)).collect();
        let f = fold(items, 3, 6);
        assert_eq!(f.slices.len(), 5);
        assert!(f.others > 0);
        let exact: Vec<(usize, u64)> = (0..6).map(|i| (i, 10)).collect();
        let f = fold(exact, 3, 6);
        assert_eq!((f.slices.len(), f.others), (6, 0));
    }

    #[test]
    fn fold_of_nothing_is_empty() {
        let f = fold(Vec::<(u8, u64)>::new(), 3, 6);
        assert!(f.slices.is_empty());
        assert_eq!(f.others, 0);
        assert_eq!(f.shares().total, 0);
    }

    #[test]
    fn fold_sorts_largest_first() {
        let f = fold(vec![("x", 1), ("y", 5), ("z", 3)], 0, 6);
        assert_eq!(f.slices, [("y", 5), ("z", 3), ("x", 1)]);
    }
}
