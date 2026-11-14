use serde::{Deserialize, Serialize};
use tracker_core::kinematics::KinematicRow;

/// Least-squares summary of a run: quadratic fits x(t) and y(t).
/// For free fall, `ay` estimates the gravitational acceleration (negative, y up).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FitSummary {
    pub n: usize,
    pub ax: Option<f64>,
    pub ay: Option<f64>,
    pub vx0: Option<f64>,
    pub vy0: Option<f64>,
    pub r2_x: Option<f64>,
    pub r2_y: Option<f64>,
}

fn quadratic(t: &[f64], v: &[f64]) -> Option<([f64; 3], f64)> {
    let n = t.len();
    if n < 3 {
        return None;
    }
    let t0 = t[0];
    let mut s = [0.0f64; 5];
    let mut b = [0.0f64; 3];
    for (&ti, &vi) in t.iter().zip(v) {
        let x = ti - t0;
        let mut p = 1.0;
        for k in 0..5 {
            s[k] += p;
            if k < 3 {
                b[k] += p * vi;
            }
            p *= x;
        }
    }
    let m = [[s[0], s[1], s[2]], [s[1], s[2], s[3]], [s[2], s[3], s[4]]];
    let det = |m: [[f64; 3]; 3]| m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let d = det(m);
    if d.abs() < 1e-18 {
        return None;
    }
    let mut c = [0.0; 3];
    for (k, ck) in c.iter_mut().enumerate() {
        let mut mk = m;
        for (r, row) in mk.iter_mut().enumerate() {
            row[k] = b[r];
        }
        *ck = det(mk) / d;
    }
    let mean = v.iter().sum::<f64>() / n as f64;
    let (mut ss_res, mut ss_tot) = (0.0, 0.0);
    for (&ti, &vi) in t.iter().zip(v) {
        let x = ti - t0;
        let f = c[0] + c[1] * x + c[2] * x * x;
        ss_res += (vi - f).powi(2);
        ss_tot += (vi - mean).powi(2);
    }
    let r2 = if ss_tot > 0.0 { 1.0 - ss_res / ss_tot } else { 1.0 };
    Some((c, r2))
}

pub fn fit_motion(rows: &[KinematicRow]) -> Option<FitSummary> {
    let t: Vec<f64> = rows.iter().map(|r| r.t).collect();
    let x: Vec<f64> = rows.iter().map(|r| r.x).collect();
    let y: Vec<f64> = rows.iter().map(|r| r.y).collect();
    let fx = quadratic(&t, &x);
    let fy = quadratic(&t, &y);
    if fx.is_none() && fy.is_none() {
        return None;
    }
    Some(FitSummary {
        n: rows.len(),
        ax: fx.map(|(c, _)| 2.0 * c[2]),
        ay: fy.map(|(c, _)| 2.0 * c[2]),
        vx0: fx.map(|(c, _)| c[1]),
        vy0: fy.map(|(c, _)| c[1]),
        r2_x: fx.map(|(_, r)| r),
        r2_y: fy.map(|(_, r)| r),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracker_core::{FrameStamp, Origin};

    #[test]
    fn recovers_free_fall() {
        let rows: Vec<_> = (0..20)
            .map(|i| {
                let t = 0.2 + i as f64 / 30.0;
                let dt = t - 0.2;
                KinematicRow { frame: FrameStamp::new(i, 30), t, pixel_x: 0.0, pixel_y: 0.0, x: 1.0 + 2.0 * dt, y: 3.0 + 4.0 * dt - 4.9 * dt * dt, vx: None, vy: None, speed: None, ax: None, ay: None, origin: Origin::Automatic, score: None }
            })
            .collect();
        let f = fit_motion(&rows).unwrap();
        assert!((f.ay.unwrap() + 9.8).abs() < 1e-6);
        assert!(f.ax.unwrap().abs() < 1e-6);
        assert!((f.vy0.unwrap() - 4.0).abs() < 1e-6);
        assert!(f.r2_y.unwrap() > 0.999999);
    }
}
