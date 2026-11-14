//! Project files.
//!
//! Version 2 (`*.tracker.json`, written by the Rust app) stores the source video both as an
//! absolute path and relative to the project file, plus a content fingerprint. Version 1 files
//! written by the Swift app are read and converted.

use crate::coords::{Calibration, CoordinateSystem, LengthUnit};
use crate::geom::Point;
use crate::io::{WriteError, atomic_write};
use crate::model::{AnalysisState, Clip, Measurement, Origin, Track, TrackKind};
use crate::settings::TrackingSettings;
use crate::stamp::FrameStamp;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

pub const FORMAT: &str = "tracker-rs-project";
pub const VERSION: u32 = 2;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("unsupported project version {0}")]
    UnsupportedVersion(u64),
    #[error("project data is invalid: {0}")]
    Invalid(String),
    #[error("could not parse project: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Write(#[from] WriteError),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceRef {
    pub absolute: PathBuf,
    /// Path relative to the project file's folder, used first when reopening.
    #[serde(default)]
    pub relative: Option<String>,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub fingerprint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub format: String,
    pub version: u32,
    pub source: SourceRef,
    pub analysis: AnalysisState,
    pub settings: TrackingSettings,
    /// Frame shown when the project was saved.
    #[serde(default)]
    pub current_frame: Option<FrameStamp>,
    #[serde(default)]
    pub selected_track: Option<Uuid>,
    #[serde(default)]
    pub notes: String,
}

impl Project {
    pub fn new(source: SourceRef, analysis: AnalysisState, settings: TrackingSettings) -> Self {
        Self { format: FORMAT.into(), version: VERSION, source, analysis, settings, current_frame: None, selected_track: None, notes: String::new() }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        let bad = |m: &str| Err(ProjectError::Invalid(m.to_string()));
        if self.format != FORMAT {
            return bad("unknown format");
        }
        if self.version != VERSION {
            return Err(ProjectError::UnsupportedVersion(self.version as u64));
        }
        let (w, h) = (self.source.width, self.source.height);
        if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
            return bad("source size");
        }
        if !self.analysis.coords.is_valid() {
            return bad("coordinate system or calibration");
        }
        self.settings.validate().map_err(ProjectError::Invalid)?;
        let mut ids = std::collections::HashSet::new();
        for t in &self.analysis.tracks {
            if !ids.insert(t.id) {
                return bad("duplicate track id");
            }
            for pair in t.points.windows(2) {
                if pair[0].frame >= pair[1].frame {
                    return bad("points out of order or duplicated");
                }
            }
            for p in &t.points {
                if !p.frame.is_valid() || !p.pixel.inside(w, h) || p.score.is_some_and(|s| !s.is_finite() || !(-1.0..=1.0).contains(&s)) {
                    return bad("point outside the image or invalid");
                }
            }
        }
        if self.selected_track.is_some_and(|id| !ids.contains(&id)) {
            return bad("selected track does not exist");
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<Vec<u8>, ProjectError> {
        self.validate()?;
        Ok(serde_json::to_vec_pretty(self)?)
    }

    /// Saves atomically, filling in the relative source path for the destination folder.
    pub fn save(&mut self, dest: &Path, overwrite: bool) -> Result<(), ProjectError> {
        crate::io::check_not_source(dest, &self.source.absolute)?;
        self.source.relative = dest.parent().and_then(|dir| relative_path(dir, &self.source.absolute));
        let data = self.to_json()?;
        atomic_write(dest, &data, overwrite)?;
        Ok(())
    }

    /// Reads a v2 project or converts a Swift v1 project.
    pub fn read(path: &Path) -> Result<Self, ProjectError> {
        let data = std::fs::read(path)?;
        let value: serde_json::Value = serde_json::from_slice(&data)?;
        let project = if value.get("format").is_some() {
            serde_json::from_value::<Project>(value)?
        } else {
            match value.get("version").and_then(|v| v.as_u64()) {
                Some(1) => swift_v1::convert(serde_json::from_value(value)?)?,
                Some(v) => return Err(ProjectError::UnsupportedVersion(v)),
                None => return Err(ProjectError::Invalid("missing version".into())),
            }
        };
        project.validate()?;
        Ok(project)
    }

    /// Where the source should be: the relative path if that file exists, else the absolute one.
    pub fn resolve_source(&self, project_path: &Path) -> Option<PathBuf> {
        if let (Some(rel), Some(dir)) = (&self.source.relative, project_path.parent()) {
            let candidate = dir.join(rel);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        self.source.absolute.exists().then(|| self.source.absolute.clone())
    }
}

/// `target` relative to `base` (both absolute), e.g. `../videos/clip.mp4`.
pub fn relative_path(base: &Path, target: &Path) -> Option<String> {
    let b: Vec<Component> = base.components().collect();
    let t: Vec<Component> = target.components().collect();
    if b.first() != t.first() {
        return None;
    }
    let common = b.iter().zip(&t).take_while(|(x, y)| x == y).count();
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for c in &t[common..] {
        out.push(c.as_os_str());
    }
    out.to_str().map(|s| s.to_string())
}

mod swift_v1 {
    use super::*;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum UrlValue {
        Text(String),
        Keyed { relative: String },
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct V1 {
        source: UrlValue,
        source_size: [f64; 2],
        analysis: V1Analysis,
        time: f64,
        settings: V1Settings,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct V1Analysis {
        tracks: Vec<V1Track>,
        #[serde(rename = "selectedTrackID")]
        selected_track_id: Option<Uuid>,
        calibration: Option<V1Calibration>,
    }
    #[derive(Deserialize)]
    struct V1Track {
        id: Uuid,
        name: String,
        points: Vec<V1Point>,
    }
    #[derive(Deserialize)]
    struct V1Point {
        id: Uuid,
        time: f64,
        pixel: [f64; 2],
        origin: Origin,
        score: Option<f64>,
        frame: Option<FrameStamp>,
    }
    #[derive(Deserialize)]
    struct V1Calibration {
        first: [f64; 2],
        second: [f64; 2],
        length: f64,
        unit: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct V1Settings {
        template_radius: u32,
        search_radius: u32,
        minimum_score: f64,
        minimum_separation: f64,
    }

    pub fn convert(v: V1) -> Result<Project, ProjectError> {
        let url = match v.source {
            UrlValue::Text(s) | UrlValue::Keyed { relative: s } => s,
        };
        let path = url.strip_prefix("file://").ok_or_else(|| ProjectError::Invalid("source must be a file URL".into()))?;
        let path = PathBuf::from(percent_decode(path));
        let [w, h] = v.source_size;
        let mut analysis = AnalysisState::new(h);
        if let Some(c) = v.analysis.calibration {
            let unit = LengthUnit::parse(&c.unit).ok_or_else(|| ProjectError::Invalid("calibration unit".into()))?;
            analysis.coords = CoordinateSystem { calibration: Some(Calibration { first: c.first.into(), second: c.second.into(), length: c.length, unit }), ..analysis.coords };
        }
        for (i, t) in v.analysis.tracks.into_iter().enumerate() {
            let mut track = Track { id: t.id, name: t.name, kind: TrackKind::PointMass, ..Track::new("", i) };
            for p in t.points {
                // v1 stored a rational frame for media-derived points; fall back to microseconds.
                let frame = p.frame.unwrap_or(FrameStamp::new((p.time * 1_000_000.0).round() as i64, 1_000_000));
                track.put(Measurement { id: p.id, frame, pixel: Point::from(p.pixel), origin: p.origin, score: p.score, run: None }, false);
            }
            analysis.tracks.push(Arc::new(track));
        }
        analysis.clip = Clip { step: 1, ..Default::default() };
        let s = &v.settings;
        let settings = TrackingSettings { template_radius: s.template_radius, search_radius: s.search_radius, minimum_score: s.minimum_score, minimum_separation: s.minimum_separation, ..TrackingSettings::swift_v1() };
        let mut project = Project::new(SourceRef { absolute: path, relative: None, width: w, height: h, fingerprint: None }, analysis, settings);
        project.selected_track = v.analysis.selected_track_id;
        project.current_frame = Some(FrameStamp::new((v.time * 1_000_000.0).round() as i64, 1_000_000));
        Ok(project)
    }

    fn percent_decode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(b);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Project {
        let mut a = AnalysisState::new(120.0);
        let mut t = Track::new("ball", 0);
        t.put(Measurement::manual(FrameStamp::new(2, 25), Point::new(50.0, 70.0)), true);
        t.put(Measurement { origin: Origin::Automatic, score: Some(0.95), ..Measurement::manual(FrameStamp::new(3, 25), Point::new(51.0, 71.0)) }, true);
        let id = t.id;
        a.tracks.push(Arc::new(t));
        let mut p = Project::new(SourceRef { absolute: "/tmp/videos/clip.mov".into(), relative: None, width: 160.0, height: 120.0, fingerprint: None }, a, TrackingSettings::default());
        p.selected_track = Some(id);
        p
    }

    #[test]
    fn round_trip_with_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = sample();
        let vids = dir.path().join("videos");
        std::fs::create_dir(&vids).unwrap();
        p.source.absolute = vids.join("clip.mov");
        std::fs::write(&p.source.absolute, b"x").unwrap();
        let dest = dir.path().join("projects").join("a.tracker.json");
        std::fs::create_dir(dest.parent().unwrap()).unwrap();
        p.save(&dest, false).unwrap();
        assert_eq!(p.source.relative.as_deref(), Some("../videos/clip.mov"));
        let back = Project::read(&dest).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.resolve_source(&dest).unwrap(), dest.parent().unwrap().join("../videos/clip.mov"));
        assert!(p.save(&dest, false).is_err(), "no clobber");
    }

    #[test]
    fn invalid_points_are_rejected() {
        let mut p = sample();
        Arc::make_mut(&mut p.analysis.tracks[0]).points[0].pixel = Point::new(500.0, 1.0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn reads_swift_v1_projects() {
        let json = r#"{"version":1,"source":"file:///Users/me/My%20Videos/clean.mov","sourceSize":[160,120],
          "analysis":{"tracks":[{"id":"6F0DD792-B720-426B-80BB-A340C480FEFF","name":"Point mass 1","points":[
            {"id":"10B88000-F3BB-4384-BF91-A1477610C6E3","time":0.04,"pixel":[30,45],"origin":"manual","frame":{"value":1,"timescale":25}},
            {"id":"54BDB3A3-5150-4C86-A4B1-DB1799C6A229","time":0.12,"pixel":[31,45.2],"origin":"automatic","score":0.97,"frame":{"value":3,"timescale":25}}]}],
            "selectedTrackID":"6F0DD792-B720-426B-80BB-A340C480FEFF",
            "calibration":{"first":[0,0],"second":[100,0],"length":1,"unit":"m"}},
          "time":0.04,"settings":{"templateRadius":12,"searchRadius":24,"minimumScore":0.9,"minimumSeparation":0.015},
          "algorithm":"fixed-grayscale-ssd-v1"}"#;
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("v1.tracker.json");
        std::fs::write(&f, json).unwrap();
        let p = Project::read(&f).unwrap();
        assert_eq!(p.source.absolute, PathBuf::from("/Users/me/My Videos/clean.mov"));
        assert_eq!(p.analysis.tracks[0].points.len(), 2);
        assert_eq!(p.analysis.tracks[0].points[1].origin, Origin::Automatic);
        assert_eq!(p.analysis.coords.unit(), "m");
        assert_eq!(p.settings.algorithm, crate::settings::Algorithm::SsdV1);
        let w = p.analysis.coords.to_world(Point::new(30.0, 45.0));
        assert!((w.x - 0.3).abs() < 1e-12 && (w.y - 0.75).abs() < 1e-12);
    }
}
