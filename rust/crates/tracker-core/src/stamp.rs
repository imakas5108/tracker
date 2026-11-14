use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

/// An exact source presentation timestamp, `value / timescale` seconds.
///
/// Equality, ordering and hashing all use the exact rational value, so `1/10` equals `3/30`.
/// (The Swift app compared equality by fields but ordered by floating seconds.)
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FrameStamp {
    pub value: i64,
    pub timescale: i32,
}

impl FrameStamp {
    pub const ZERO: FrameStamp = FrameStamp { value: 0, timescale: 1 };

    pub fn new(value: i64, timescale: i32) -> Self {
        assert!(timescale > 0, "timescale must be positive");
        Self { value, timescale }
    }
    pub fn seconds(&self) -> f64 {
        self.value as f64 / self.timescale as f64
    }
    pub fn is_valid(&self) -> bool {
        self.timescale > 0 && self.value >= 0
    }
    fn reduced(&self) -> (i64, i64) {
        let (mut a, mut b) = (self.value.unsigned_abs(), self.timescale as u64);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        let g = a.max(1) as i64;
        (self.value / g, self.timescale as i64 / g)
    }
}

impl PartialEq for FrameStamp {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for FrameStamp {}
impl PartialOrd for FrameStamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for FrameStamp {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.value as i128 * other.timescale as i128).cmp(&(other.value as i128 * self.timescale as i128))
    }
}
impl Hash for FrameStamp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.reduced().hash(state)
    }
}

/// Sorted, de-duplicated presentation stamps of every decodable video frame.
///
/// Built from the container's sample table, so variable frame rates and a non-zero first
/// timestamp (common in MP4 files) are represented exactly.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameIndex {
    stamps: Vec<FrameStamp>,
}

impl FrameIndex {
    pub fn new(mut stamps: Vec<FrameStamp>) -> Self {
        stamps.retain(|s| s.is_valid());
        stamps.sort();
        stamps.dedup();
        Self { stamps }
    }
    pub fn len(&self) -> usize {
        self.stamps.len()
    }
    pub fn is_empty(&self) -> bool {
        self.stamps.is_empty()
    }
    pub fn stamps(&self) -> &[FrameStamp] {
        &self.stamps
    }
    pub fn get(&self, index: usize) -> Option<FrameStamp> {
        self.stamps.get(index).copied()
    }
    pub fn first(&self) -> Option<FrameStamp> {
        self.stamps.first().copied()
    }
    pub fn last(&self) -> Option<FrameStamp> {
        self.stamps.last().copied()
    }
    /// Index of an exact stamp.
    pub fn position(&self, stamp: FrameStamp) -> Option<usize> {
        self.stamps.binary_search(&stamp).ok()
    }
    /// The frame displayed at `seconds`: the last frame starting at or before it, else the first.
    pub fn index_at(&self, seconds: f64) -> Option<usize> {
        if self.stamps.is_empty() || !seconds.is_finite() {
            return None;
        }
        let count = self.stamps.partition_point(|s| s.seconds() <= seconds + 1e-9);
        Some(count.saturating_sub(1))
    }
    pub fn stamp_at(&self, seconds: f64) -> Option<FrameStamp> {
        self.index_at(seconds).and_then(|i| self.get(i))
    }
    /// The frame whose start is closest to `seconds`.
    pub fn nearest(&self, seconds: f64) -> Option<FrameStamp> {
        let i = self.index_at(seconds)?;
        let a = self.stamps[i];
        match self.stamps.get(i + 1) {
            Some(b) if (b.seconds() - seconds).abs() < (seconds - a.seconds()).abs() => Some(*b),
            _ => Some(a),
        }
    }
    /// Step `count` frames from `stamp`, clamped to the video.
    pub fn step(&self, stamp: FrameStamp, count: isize) -> Option<FrameStamp> {
        let i = self.position(stamp).or_else(|| self.index_at(stamp.seconds()))? as isize;
        let j = (i + count).clamp(0, self.stamps.len() as isize - 1);
        self.get(j as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equality_and_order_are_exact_and_consistent() {
        let a = FrameStamp::new(1, 10);
        let b = FrameStamp::new(3, 30);
        assert_eq!(a, b);
        assert_eq!(a.cmp(&b), Ordering::Equal);
        let mut set = std::collections::HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
        assert!(FrameStamp::new(1001, 30000) < FrameStamp::new(1, 29));
    }

    #[test]
    fn index_handles_offset_start_and_variable_rate() {
        // First PTS at 2/30 s, then alternating 0.04 / 0.08 s durations.
        let idx = FrameIndex::new(vec![
            FrameStamp::new(2, 30),
            FrameStamp::new(1067, 15000),
            FrameStamp::new(2267, 15000),
            FrameStamp::new(2867, 15000),
            FrameStamp::new(2, 30),
        ]);
        assert_eq!(idx.len(), 4);
        assert_eq!(idx.index_at(0.0), Some(0));
        assert_eq!(idx.index_at(0.10), Some(1));
        assert_eq!(idx.stamp_at(10.0), idx.last());
        assert_eq!(idx.step(idx.first().unwrap(), 2), idx.get(2));
        assert_eq!(idx.step(idx.first().unwrap(), -5), idx.first());
        assert_eq!(idx.nearest(0.15), idx.get(2));
    }
}
