use std::ops::RangeInclusive;

pub(crate) fn min_range_distance_to(
    range1: RangeInclusive<i64>,
    range2: RangeInclusive<i64>,
) -> i64 {
    if range1.end() < range2.start() {
        range2.start() - range1.end()
    } else if range2.end() < range1.start() {
        range1.start() - range2.end()
    } else {
        0 // overlap
    }
}

pub(crate) fn max_range_distance_to(
    range1: RangeInclusive<i64>,
    range2: RangeInclusive<i64>,
) -> i64 {
    // Pick the farthest end of `range1`.
    [*range1.start(), *range1.end()]
        // Pick the closest end of `range2`.
        .map(|pos1| min_range_distance_to_pos(range2.clone(), pos1))
        .into_iter()
        .max()
        .unwrap_or(0)
}

pub(crate) fn min_range_distance_to_pos(range: RangeInclusive<i64>, pos: i64) -> i64 {
    if *range.end() < pos {
        pos - range.end()
    } else if pos < *range.start() {
        range.start() - pos
    } else {
        0 // contains pos
    }
}
