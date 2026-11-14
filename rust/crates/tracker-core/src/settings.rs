use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Algorithm {
    /// Exact port of the Swift baseline: fixed grayscale template, mean squared difference.
    SsdV1,
    /// Zero-mean normalized cross-correlation with mask, motion prediction and tethered template evolution.
    NccV2,
}

impl Algorithm {
    pub fn id(&self) -> &'static str {
        match self {
            Algorithm::SsdV1 => "fixed-grayscale-ssd-v1",
            Algorithm::NccV2 => "masked-ncc-predictive-v2",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskShape {
    Circle,
    Square,
}

/// What the autotracker does when a match is uncertain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewPolicy {
    /// Pause and wait for an explicit accept, skip or manual correction.
    Pause,
    /// Leave a gap and keep searching around the predicted position, up to `max_gap` frames.
    SkipUncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackingSettings {
    pub algorithm: Algorithm,
    pub template_radius: u32,
    pub search_radius: u32,
    pub minimum_score: f64,
    pub minimum_separation: f64,
    pub predict: bool,
    /// Fraction of each accepted match blended into the working template (0 keeps it fixed).
    pub evolve_rate: f64,
    /// Fraction of the key-frame template blended back each frame, limiting drift.
    pub tether: f64,
    pub mask: MaskShape,
    pub review_policy: ReviewPolicy,
    pub max_gap: u32,
}

impl Default for TrackingSettings {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::NccV2,
            template_radius: 12,
            search_radius: 32,
            minimum_score: 0.80,
            minimum_separation: 0.05,
            predict: true,
            evolve_rate: 0.15,
            tether: 0.05,
            mask: MaskShape::Circle,
            review_policy: ReviewPolicy::Pause,
            max_gap: 5,
        }
    }
}

impl TrackingSettings {
    /// The Swift app's defaults, for parity runs.
    pub fn swift_v1() -> Self {
        Self { algorithm: Algorithm::SsdV1, template_radius: 12, search_radius: 24, minimum_score: 0.90, minimum_separation: 0.015, predict: false, evolve_rate: 0.0, tether: 0.0, mask: MaskShape::Square, review_policy: ReviewPolicy::Pause, max_gap: 0 }
    }
    pub fn validate(&self) -> Result<(), String> {
        let unit = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
        if !(3..=48).contains(&self.template_radius) {
            return Err("template_radius must be 3 to 48 px".into());
        }
        if !(4..=256).contains(&self.search_radius) {
            return Err("search_radius must be 4 to 256 px".into());
        }
        if !unit(self.minimum_score) || !unit(self.minimum_separation) {
            return Err("minimum_score and minimum_separation must be 0 to 1".into());
        }
        if !(self.evolve_rate.is_finite() && (0.0..=0.5).contains(&self.evolve_rate)) || !unit(self.tether) {
            return Err("evolve_rate must be 0 to 0.5 and tether 0 to 1".into());
        }
        if self.max_gap > 120 {
            return Err("max_gap must be at most 120 frames".into());
        }
        Ok(())
    }
}
