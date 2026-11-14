use crate::frame::GrayFrame;
use tracker_core::{MaskShape, Point};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Match {
    /// Subpixel position of the best candidate in source pixels.
    pub position: Point,
    /// Similarity of the best candidate (higher is better; algorithm-specific scale).
    pub score: f64,
    /// Margin between the best candidate and the best spatially separate alternative.
    pub separation: f64,
    /// The best candidate sits on the edge of the searched region.
    pub at_boundary: bool,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MatchError {
    #[error("The template needs visible detail and must fit inside the video. Mark the target's center or enlarge the template.")]
    InvalidTemplate,
    #[error("The search region lies outside the image.")]
    OutsideImage,
}

pub trait Matcher: Send {
    fn template_radius(&self) -> usize;
    /// Searches a square of `search_radius` around `center`.
    fn find(&self, image: &GrayFrame, center: Point, search_radius: usize) -> Result<Match, MatchError>;
    /// Blends the patch at `position` into the working template (no-op for fixed templates).
    fn evolve(&mut self, _image: &GrayFrame, _position: Point, _rate: f64, _tether: f64) {}
    /// Replaces both the working and key-frame templates.
    fn rekey(&mut self, image: &GrayFrame, center: Point) -> Result<(), MatchError>;
}

fn extract(image: &GrayFrame, center: Point, r: usize) -> Result<Vec<f32>, MatchError> {
    let (x, y) = (center.x.round() as isize, center.y.round() as isize);
    if !center.is_finite() || !image.fits(x, y, r) {
        return Err(MatchError::InvalidTemplate);
    }
    let (x, y) = (x as usize, y as usize);
    let mut out = Vec::with_capacity((2 * r + 1).pow(2));
    for row in y - r..=y + r {
        out.extend(image.data[row * image.width + x - r..=row * image.width + x + r].iter().map(|&v| v as f32));
    }
    Ok(out)
}

/// Search bounds clamped so every candidate's patch is inside the image.
fn bounds(image: &GrayFrame, center: Point, search: usize, r: usize) -> Result<(usize, usize, usize, usize), MatchError> {
    let (cx, cy) = (center.x.round() as isize, center.y.round() as isize);
    let (r, s) = (r as isize, search as isize);
    let x0 = (cx - s).max(r);
    let x1 = (cx + s).min(image.width as isize - r - 1);
    let y0 = (cy - s).max(r);
    let y1 = (cy + s).min(image.height as isize - r - 1);
    if x0 > x1 || y0 > y1 || !center.is_finite() {
        return Err(MatchError::OutsideImage);
    }
    Ok((x0 as usize, x1 as usize, y0 as usize, y1 as usize))
}

/// Parabolic peak offset in [-0.5, 0.5] from three samples around an extremum.
fn parabola(before: f64, at: f64, after: f64, maximize: bool) -> f64 {
    let (b, a, c) = if maximize { (-before, -at, -after) } else { (before, at, after) };
    let d = b - 2.0 * a + c;
    if d > 1e-12 { (0.5 * (b - c) / d).clamp(-0.5, 0.5) } else { 0.0 }
}

/// Best candidate and best alternative outside a Chebyshev exclusion zone around it.
fn best_and_alternative(values: &[f64], w: usize, maximize: bool, exclusion: usize) -> (usize, f64, f64) {
    let better = |a: f64, b: f64| if maximize { a > b } else { a < b };
    let mut bi = 0;
    for (i, &v) in values.iter().enumerate() {
        if better(v, values[bi]) {
            bi = i;
        }
    }
    let (bx, by) = (bi % w, bi / w);
    let mut alt = if maximize { f64::NEG_INFINITY } else { f64::INFINITY };
    for (i, &v) in values.iter().enumerate() {
        let (x, y) = (i % w, i / w);
        if x.abs_diff(bx) > exclusion || y.abs_diff(by) > exclusion {
            if better(v, alt) {
                alt = v;
            }
        }
    }
    (bi, values[bi], alt)
}

/// Exact port of the Swift baseline matcher ("fixed-grayscale-ssd-v1").
pub struct SsdMatcher {
    radius: usize,
    template: Vec<f32>,
}

impl SsdMatcher {
    pub fn new(image: &GrayFrame, center: Point, radius: usize) -> Result<Self, MatchError> {
        let template = extract(image, center, radius)?;
        let n = template.len() as f64;
        let mean = template.iter().map(|&v| v as f64).sum::<f64>() / n;
        let var = template.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>() / n;
        if var <= 16.0 {
            return Err(MatchError::InvalidTemplate);
        }
        Ok(Self { radius, template })
    }
    fn cost(&self, image: &GrayFrame, x: usize, y: usize) -> f64 {
        let r = self.radius;
        let mut sum = 0.0f64;
        let mut k = 0;
        for row in y - r..=y + r {
            let base = row * image.width + x - r;
            for col in 0..2 * r + 1 {
                let d = image.data[base + col] as f64 - self.template[k] as f64;
                sum += d * d;
                k += 1;
            }
        }
        sum / (self.template.len() as f64 * 255.0 * 255.0)
    }
}

impl Matcher for SsdMatcher {
    fn template_radius(&self) -> usize {
        self.radius
    }
    fn find(&self, image: &GrayFrame, center: Point, search: usize) -> Result<Match, MatchError> {
        let (x0, x1, y0, y1) = bounds(image, center, search, self.radius)?;
        let w = x1 - x0 + 1;
        let mut costs = Vec::with_capacity(w * (y1 - y0 + 1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                costs.push(self.cost(image, x, y));
            }
        }
        let (bi, best, alt) = best_and_alternative(&costs, w, false, (self.radius / 2).max(2));
        let (bx, by) = (x0 + bi % w, y0 + bi / w);
        let at = |x: usize, y: usize| costs[(y - y0) * w + (x - x0)];
        let dx = if bx > x0 && bx < x1 { parabola(at(bx - 1, by), best, at(bx + 1, by), false) } else { 0.0 };
        let dy = if by > y0 && by < y1 { parabola(at(bx, by - 1), best, at(bx, by + 1), false) } else { 0.0 };
        Ok(Match {
            position: Point::new(bx as f64 + dx, by as f64 + dy),
            score: (1.0 - best).max(0.0),
            separation: if alt.is_finite() { (alt - best).max(0.0) } else { 0.0 },
            at_boundary: bx == x0 || bx == x1 || by == y0 || by == y1,
        })
    }
    fn rekey(&mut self, image: &GrayFrame, center: Point) -> Result<(), MatchError> {
        *self = Self::new(image, center, self.radius)?;
        Ok(())
    }
}

/// Masked zero-mean normalized cross-correlation with an evolving, tethered template.
pub struct NccMatcher {
    radius: usize,
    weights: Vec<f32>,
    key: Vec<f32>,
    working: Vec<f32>,
}

impl NccMatcher {
    pub fn new(image: &GrayFrame, center: Point, radius: usize, mask: MaskShape) -> Result<Self, MatchError> {
        let side = 2 * radius + 1;
        let r2 = (radius as f32 + 0.5).powi(2);
        let weights: Vec<f32> = (0..side * side)
            .map(|i| {
                let (dx, dy) = ((i % side) as f32 - radius as f32, (i / side) as f32 - radius as f32);
                match mask {
                    MaskShape::Square => 1.0,
                    MaskShape::Circle => if dx * dx + dy * dy <= r2 { 1.0 } else { 0.0 },
                }
            })
            .collect();
        let key = extract(image, center, radius)?;
        let m = Self { radius, weights, working: key.clone(), key };
        if m.template_stats(&m.working).1 < 16.0 {
            return Err(MatchError::InvalidTemplate);
        }
        Ok(m)
    }
    /// (masked mean, masked variance)
    fn template_stats(&self, t: &[f32]) -> (f64, f64) {
        let (mut sw, mut s, mut s2) = (0.0f64, 0.0f64, 0.0f64);
        for (&w, &v) in self.weights.iter().zip(t) {
            let (w, v) = (w as f64, v as f64);
            sw += w;
            s += w * v;
            s2 += w * v * v;
        }
        let mean = s / sw;
        (mean, (s2 / sw - mean * mean).max(0.0))
    }
    fn score_at(&self, image: &GrayFrame, x: usize, y: usize, t: &[f32], t_mean: f64, t_norm: f64) -> f64 {
        let r = self.radius;
        let side = 2 * r + 1;
        let (mut sw, mut si, mut sii, mut sit) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for j in 0..side {
            let base = (y - r + j) * image.width + x - r;
            let row = &image.data[base..base + side];
            let wrow = &self.weights[j * side..(j + 1) * side];
            let trow = &t[j * side..(j + 1) * side];
            for k in 0..side {
                let w = wrow[k] as f64;
                if w == 0.0 {
                    continue;
                }
                let iv = row[k] as f64;
                sw += w;
                si += w * iv;
                sii += w * iv * iv;
                sit += w * iv * (trow[k] as f64 - t_mean);
            }
        }
        let i_var = sii - si * si / sw;
        if i_var <= 1e-6 || t_norm <= 1e-6 {
            return 0.0;
        }
        sit / (i_var.sqrt() * t_norm)
    }
}

impl Matcher for NccMatcher {
    fn template_radius(&self) -> usize {
        self.radius
    }
    fn find(&self, image: &GrayFrame, center: Point, search: usize) -> Result<Match, MatchError> {
        let (x0, x1, y0, y1) = bounds(image, center, search, self.radius)?;
        let (t_mean, t_var) = self.template_stats(&self.working);
        let sw: f64 = self.weights.iter().map(|&w| w as f64).sum();
        let t_norm = (t_var * sw).sqrt();
        let w = x1 - x0 + 1;
        let mut scores = Vec::with_capacity(w * (y1 - y0 + 1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                scores.push(self.score_at(image, x, y, &self.working, t_mean, t_norm));
            }
        }
        let (bi, best, alt) = best_and_alternative(&scores, w, true, self.radius.max(2));
        let (bx, by) = (x0 + bi % w, y0 + bi / w);
        let at = |x: usize, y: usize| scores[(y - y0) * w + (x - x0)];
        let dx = if bx > x0 && bx < x1 { parabola(at(bx - 1, by), best, at(bx + 1, by), true) } else { 0.0 };
        let dy = if by > y0 && by < y1 { parabola(at(bx, by - 1), best, at(bx, by + 1), true) } else { 0.0 };
        Ok(Match {
            position: Point::new(bx as f64 + dx, by as f64 + dy),
            score: best,
            separation: if alt.is_finite() { (best - alt).max(0.0) } else { 1.0 },
            at_boundary: bx == x0 || bx == x1 || by == y0 || by == y1,
        })
    }
    fn evolve(&mut self, image: &GrayFrame, position: Point, rate: f64, tether: f64) {
        if rate <= 0.0 {
            return;
        }
        if let Ok(patch) = extract(image, position, self.radius) {
            let (e, t) = (rate as f32, tether as f32);
            for ((wv, &pv), &kv) in self.working.iter_mut().zip(&patch).zip(&self.key) {
                let blended = (1.0 - e) * *wv + e * pv;
                *wv = (1.0 - t) * blended + t * kv;
            }
        }
    }
    fn rekey(&mut self, image: &GrayFrame, center: Point) -> Result<(), MatchError> {
        let key = extract(image, center, self.radius)?;
        self.working = key.clone();
        self.key = key;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A textured disc (radius 6) drawn on a background, like a marked ball.
    pub fn draw_target(img: &mut GrayFrame, cx: f64, cy: f64, gain: f64, offset: f64) {
        for y in 0..img.height {
            for x in 0..img.width {
                let (dx, dy) = (x as f64 - cx, y as f64 - cy);
                if dx * dx + dy * dy <= 36.0 {
                    let stripe = if ((dx + dy).round() as i64).rem_euclid(3) == 0 { 230.0 } else { 120.0 };
                    img.set(x, y, (stripe * gain + offset).clamp(0.0, 255.0) as u8);
                }
            }
        }
    }
    pub fn background(w: usize, h: usize, seed: u64) -> GrayFrame {
        let mut s = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let data = (0..w * h)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (35 + ((s >> 33) % 12) as u8) as u8
            })
            .collect();
        GrayFrame::new(w, h, data)
    }

    #[test]
    fn both_matchers_find_translation() {
        let mut a = background(160, 120, 1);
        draw_target(&mut a, 40.0, 50.0, 1.0, 0.0);
        let mut b = background(160, 120, 2);
        draw_target(&mut b, 47.0, 53.0, 1.0, 0.0);
        let ssd = SsdMatcher::new(&a, Point::new(40.0, 50.0), 12).unwrap();
        let ncc = NccMatcher::new(&a, Point::new(40.0, 50.0), 12, MaskShape::Circle).unwrap();
        for m in [ssd.find(&b, Point::new(40.0, 50.0), 24).unwrap(), ncc.find(&b, Point::new(40.0, 50.0), 24).unwrap()] {
            assert!(m.position.distance(Point::new(47.0, 53.0)) < 0.5, "{m:?}");
        }
    }

    #[test]
    fn ncc_ignores_brightness_change_that_breaks_ssd_scores() {
        let mut a = background(160, 120, 3);
        draw_target(&mut a, 60.0, 60.0, 1.0, 0.0);
        let mut b = GrayFrame::new(160, 120, a.data.iter().map(|&v| (v as f64 * 0.6 + 70.0) as u8).collect());
        b.data.iter_mut().for_each(|_| {});
        let ssd = SsdMatcher::new(&a, Point::new(60.0, 60.0), 12).unwrap();
        let ncc = NccMatcher::new(&a, Point::new(60.0, 60.0), 12, MaskShape::Circle).unwrap();
        let s = ssd.find(&b, Point::new(60.0, 60.0), 16).unwrap();
        let n = ncc.find(&b, Point::new(60.0, 60.0), 16).unwrap();
        assert!(n.score > 0.98 && n.position.distance(Point::new(60.0, 60.0)) < 0.5, "{n:?}");
        assert!(s.score < n.score);
    }

    #[test]
    fn uniform_templates_are_rejected() {
        let img = GrayFrame::filled(100, 100, 40);
        assert_eq!(SsdMatcher::new(&img, Point::new(50.0, 50.0), 12).err(), Some(MatchError::InvalidTemplate));
        assert_eq!(NccMatcher::new(&img, Point::new(50.0, 50.0), 12, MaskShape::Circle).err(), Some(MatchError::InvalidTemplate));
        let mut img = background(100, 100, 4);
        draw_target(&mut img, 5.0, 5.0, 1.0, 0.0);
        assert!(NccMatcher::new(&img, Point::new(5.0, 5.0), 12, MaskShape::Circle).is_err(), "too close to the edge");
    }

    #[test]
    fn identical_distractor_has_low_separation() {
        let mut a = background(200, 120, 5);
        draw_target(&mut a, 60.0, 60.0, 1.0, 0.0);
        let mut b = background(200, 120, 6);
        draw_target(&mut b, 62.0, 60.0, 1.0, 0.0);
        draw_target(&mut b, 82.0, 60.0, 1.0, 0.0);
        let ncc = NccMatcher::new(&a, Point::new(60.0, 60.0), 8, MaskShape::Circle).unwrap();
        let m = ncc.find(&b, Point::new(72.0, 60.0), 24).unwrap();
        assert!(m.separation < 0.05, "{m:?}");
    }
}
