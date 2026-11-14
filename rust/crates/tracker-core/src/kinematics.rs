use crate::coords::CoordinateSystem;
use crate::model::{Origin, Track};
use crate::stamp::FrameStamp;
use serde::Serialize;

/// One row of derived motion data in analysis units (unit, unit/s, unit/s²).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct KinematicRow {
    pub frame: FrameStamp,
    pub t: f64,
    pub pixel_x: f64,
    pub pixel_y: f64,
    pub x: f64,
    pub y: f64,
    pub vx: Option<f64>,
    pub vy: Option<f64>,
    pub speed: Option<f64>,
    pub ax: Option<f64>,
    pub ay: Option<f64>,
    pub origin: Origin,
    pub score: Option<f64>,
}

/// Velocity and acceleration from three-point finite differences on the actual source
/// timestamps, so variable frame rates are handled exactly.
///
/// Derivatives are left empty at the ends of the track and wherever a neighbouring interval is
/// more than `max_gap_ratio` times the median interval (an occlusion or skipped frames).
pub fn kinematics(track: &Track, coords: &CoordinateSystem, max_gap_ratio: f64) -> Vec<KinematicRow> {
    let pts = &track.points;
    let world: Vec<_> = pts.iter().map(|m| coords.to_world(m.pixel)).collect();
    let times: Vec<f64> = pts.iter().map(|m| m.time()).collect();
    let mut gaps: Vec<f64> = times.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.sort_by(f64::total_cmp);
    let median = gaps.get(gaps.len() / 2).copied().unwrap_or(f64::INFINITY);
    let limit = median * max_gap_ratio.max(1.0);

    pts.iter()
        .enumerate()
        .map(|(i, m)| {
            let mut row = KinematicRow {
                frame: m.frame,
                t: times[i],
                pixel_x: m.pixel.x,
                pixel_y: m.pixel.y,
                x: world[i].x,
                y: world[i].y,
                vx: None,
                vy: None,
                speed: None,
                ax: None,
                ay: None,
                origin: m.origin,
                score: m.score,
            };
            if i > 0 && i + 1 < pts.len() {
                let (h1, h2) = (times[i] - times[i - 1], times[i + 1] - times[i]);
                if h1 > 0.0 && h2 > 0.0 && h1 <= limit && h2 <= limit {
                    let d1 = |f0: f64, f1: f64, f2: f64| -h2 / (h1 * (h1 + h2)) * f0 + (h2 - h1) / (h1 * h2) * f1 + h1 / (h2 * (h1 + h2)) * f2;
                    let d2 = |f0: f64, f1: f64, f2: f64| 2.0 * (f0 / (h1 * (h1 + h2)) - f1 / (h1 * h2) + f2 / (h2 * (h1 + h2)));
                    let (a, b, c) = (world[i - 1], world[i], world[i + 1]);
                    let (vx, vy) = (d1(a.x, b.x, c.x), d1(a.y, b.y, c.y));
                    row.vx = Some(vx);
                    row.vy = Some(vy);
                    row.speed = Some(vx.hypot(vy));
                    row.ax = Some(d2(a.x, b.x, c.x));
                    row.ay = Some(d2(a.y, b.y, c.y));
                }
            }
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::{Calibration, LengthUnit};
    use crate::geom::Point;
    use crate::model::Measurement;

    #[test]
    fn projectile_with_variable_frame_rate_recovers_g() {
        // x = 2t, y = 5t - 4.9t² metres, sampled at alternating 0.04 / 0.08 s. 100 px = 1 m.
        let mut coords = CoordinateSystem::bottom_left(1000.0);
        coords.calibration = Some(Calibration { first: Point::new(0.0, 0.0), second: Point::new(100.0, 0.0), length: 1.0, unit: LengthUnit::M });
        let mut track = Track::new("ball", 0);
        let mut ticks = 0i64; // in 1/100 s
        for i in 0..20 {
            let t = ticks as f64 / 100.0;
            let (x, y) = (2.0 * t, 5.0 * t - 4.9 * t * t);
            track.put(Measurement::manual(FrameStamp::new(ticks, 100), Point::new(x * 100.0, 1000.0 - y * 100.0)), true);
            ticks += if i % 2 == 0 { 4 } else { 8 };
        }
        let rows = kinematics(&track, &coords, 3.0);
        assert!(rows[0].vx.is_none() && rows.last().unwrap().ay.is_none());
        for r in &rows[1..rows.len() - 1] {
            assert!((r.vx.unwrap() - 2.0).abs() < 1e-9);
            assert!((r.vy.unwrap() - (5.0 - 9.8 * r.t)).abs() < 1e-9);
            assert!((r.ay.unwrap() + 9.8).abs() < 1e-9, "ay = {:?}", r.ay);
        }
    }

    #[test]
    fn gaps_suppress_derivatives() {
        let coords = CoordinateSystem::bottom_left(100.0);
        let mut track = Track::new("t", 0);
        for n in [0, 1, 2, 10, 11, 12] {
            track.put(Measurement::manual(FrameStamp::new(n, 30), Point::new(n as f64, 0.0)), true);
        }
        let rows = kinematics(&track, &coords, 3.0);
        assert!(rows[1].vx.is_some());
        assert!(rows[2].vx.is_none() && rows[3].vx.is_none());
        assert!(rows[4].vx.is_some());
    }
}
