use serde::{Deserialize, Serialize};

/// A point in orientation-corrected source pixels: origin top-left, x right, y down.
///
/// Serialized as `[x, y]`, which is also how the Swift app's `CGPoint` values were stored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f64; 2]", into = "[f64; 2]")]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
    pub fn distance(&self, other: Point) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
    /// True when the point lies inside an image of the given size.
    pub fn inside(&self, width: f64, height: f64) -> bool {
        self.is_finite() && self.x >= 0.0 && self.y >= 0.0 && self.x < width && self.y < height
    }
}

impl From<[f64; 2]> for Point {
    fn from(v: [f64; 2]) -> Self {
        Self { x: v[0], y: v[1] }
    }
}
impl From<Point> for [f64; 2] {
    fn from(p: Point) -> Self {
        [p.x, p.y]
    }
}
