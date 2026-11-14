use crate::geom::Point;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LengthUnit {
    M,
    Cm,
    Mm,
}

impl LengthUnit {
    pub fn symbol(&self) -> &'static str {
        match self {
            LengthUnit::M => "m",
            LengthUnit::Cm => "cm",
            LengthUnit::Mm => "mm",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "m" => Some(Self::M),
            "cm" => Some(Self::Cm),
            "mm" => Some(Self::Mm),
            _ => None,
        }
    }
}

/// An explicit physical scale reference: two source-pixel endpoints a known distance apart.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Calibration {
    pub first: Point,
    pub second: Point,
    pub length: f64,
    pub unit: LengthUnit,
}

impl Calibration {
    pub fn pixels_per_unit(&self) -> f64 {
        self.first.distance(self.second) / self.length
    }
    pub fn is_valid(&self) -> bool {
        let ppu = self.pixels_per_unit();
        self.first.is_finite() && self.second.is_finite() && self.length.is_finite() && self.length > 0.0 && ppu.is_finite() && ppu > 0.0
    }
}

/// Maps source pixels to analysis coordinates.
///
/// The origin is a source-pixel position and the x axis may be rotated (counterclockwise,
/// radians, as seen on screen). Analysis y points up. Without a calibration, units are pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoordinateSystem {
    pub origin: Point,
    #[serde(default)]
    pub angle: f64,
    #[serde(default)]
    pub calibration: Option<Calibration>,
}

impl CoordinateSystem {
    /// Origin at the bottom-left corner of an image, x right, y up (the Swift app's fixed frame).
    pub fn bottom_left(height: f64) -> Self {
        Self { origin: Point::new(0.0, height), angle: 0.0, calibration: None }
    }
    pub fn unit(&self) -> &'static str {
        self.calibration.map(|c| c.unit.symbol()).unwrap_or("px")
    }
    pub fn scale(&self) -> f64 {
        self.calibration.map(|c| c.pixels_per_unit()).unwrap_or(1.0)
    }
    pub fn is_valid(&self) -> bool {
        self.origin.is_finite() && self.angle.is_finite() && self.calibration.is_none_or(|c| c.is_valid())
    }
    /// Source pixel to analysis coordinates.
    pub fn to_world(&self, p: Point) -> Point {
        let dx = p.x - self.origin.x;
        let dy = self.origin.y - p.y; // flip to y-up
        let (s, c) = self.angle.sin_cos();
        let k = self.scale();
        Point::new((dx * c + dy * s) / k, (-dx * s + dy * c) / k)
    }
    /// Analysis coordinates back to source pixels.
    pub fn to_pixel(&self, w: Point) -> Point {
        let k = self.scale();
        let (s, c) = self.angle.sin_cos();
        let (x, y) = (w.x * k, w.y * k);
        let dx = x * c - y * s;
        let dy = x * s + y * c;
        Point::new(self.origin.x + dx, self.origin.y - dy)
    }
    /// Converts a pixel-space vector (for example a velocity in px/s) to analysis units.
    pub fn vector_to_world(&self, v: Point) -> Point {
        let o = self.to_world(Point::new(self.origin.x, self.origin.y));
        let t = self.to_world(Point::new(self.origin.x + v.x, self.origin.y + v.y));
        Point::new(t.x - o.x, t.y - o.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn bottom_left_matches_swift_convention() {
        let mut cs = CoordinateSystem::bottom_left(480.0);
        assert!(close(cs.to_world(Point::new(30.0, 45.0)), Point::new(30.0, 435.0)));
        cs.calibration = Some(Calibration { first: Point::new(0.0, 0.0), second: Point::new(100.0, 0.0), length: 1.0, unit: LengthUnit::M });
        assert!(close(cs.to_world(Point::new(480.0, 210.0)), Point::new(4.8, 2.7)));
        assert_eq!(cs.unit(), "m");
    }

    #[test]
    fn rotated_axes_round_trip() {
        let cs = CoordinateSystem { origin: Point::new(320.0, 240.0), angle: 30f64.to_radians(), calibration: Some(Calibration { first: Point::new(0.0, 0.0), second: Point::new(50.0, 0.0), length: 25.0, unit: LengthUnit::Cm }) };
        let p = Point::new(400.0, 100.0);
        assert!(close(cs.to_pixel(cs.to_world(p)), p));
        // A point along the rotated +x axis has zero y.
        let on_axis = Point::new(320.0 + 30f64.to_radians().cos() * 10.0, 240.0 - 30f64.to_radians().sin() * 10.0);
        assert!(cs.to_world(on_axis).y.abs() < 1e-9);
    }

    #[test]
    fn degenerate_calibration_is_invalid() {
        let c = Calibration { first: Point::new(5.0, 5.0), second: Point::new(5.0, 5.0), length: 1.0, unit: LengthUnit::M };
        assert!(!c.is_valid());
        let c = Calibration { first: Point::new(0.0, 0.0), second: Point::new(5.0, 5.0), length: -1.0, unit: LengthUnit::M };
        assert!(!c.is_valid());
    }
}
