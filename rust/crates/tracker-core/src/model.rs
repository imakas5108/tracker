use crate::coords::CoordinateSystem;
use crate::geom::Point;
use crate::stamp::FrameStamp;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

pub type TrackId = Uuid;

/// How a measurement came to exist. Predictions are never stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Origin {
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "automatic")]
    Automatic,
    /// An uncertain candidate that a person or agent explicitly accepted.
    #[serde(rename = "acceptedCandidate", alias = "reviewed")]
    AcceptedCandidate,
}

impl Origin {
    pub fn code(&self) -> &'static str {
        match self {
            Origin::Manual => "M",
            Origin::Automatic => "A",
            Origin::AcceptedCandidate => "R",
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Origin::Manual => "manual",
            Origin::Automatic => "automatic",
            Origin::AcceptedCandidate => "reviewed",
        }
    }
    /// Automatic results never overwrite manual or reviewed ones.
    pub fn is_authoritative(&self) -> bool {
        !matches!(self, Origin::Automatic)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    pub id: Uuid,
    pub frame: FrameStamp,
    pub pixel: Point,
    pub origin: Origin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Identifier of the tracking run that produced an automatic point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<Uuid>,
}

impl Measurement {
    pub fn manual(frame: FrameStamp, pixel: Point) -> Self {
        Self { id: Uuid::new_v4(), frame, pixel, origin: Origin::Manual, score: None, run: None }
    }
    pub fn time(&self) -> f64 {
        self.frame.seconds()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    #[default]
    PointMass,
}

/// A point-mass track. Points are kept sorted by frame stamp, one per frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    #[serde(default)]
    pub kind: TrackKind,
    /// Display color as RGB.
    #[serde(default = "default_color")]
    pub color: [u8; 3],
    #[serde(default)]
    pub mass: Option<f64>,
    pub points: Vec<Measurement>,
}

fn default_color() -> [u8; 3] {
    [214, 39, 40]
}

/// Distinct track colors, chosen to stay visible on typical lab footage.
pub const TRACK_COLORS: [[u8; 3]; 6] = [[214, 39, 40], [31, 119, 180], [44, 160, 44], [255, 127, 14], [148, 103, 189], [23, 190, 207]];

impl Track {
    pub fn new(name: impl Into<String>, color_index: usize) -> Self {
        Self { id: Uuid::new_v4(), name: name.into(), kind: TrackKind::PointMass, color: TRACK_COLORS[color_index % TRACK_COLORS.len()], mass: None, points: Vec::new() }
    }
    pub fn point_at(&self, frame: FrameStamp) -> Option<&Measurement> {
        self.points.binary_search_by(|m| m.frame.cmp(&frame)).ok().map(|i| &self.points[i])
    }
    /// Inserts or replaces the point on its frame. Returns false when an automatic point would
    /// replace an authoritative one and `respect_authority` is set.
    pub fn put(&mut self, m: Measurement, respect_authority: bool) -> bool {
        match self.points.binary_search_by(|p| p.frame.cmp(&m.frame)) {
            Ok(i) => {
                if respect_authority && !m.origin.is_authoritative() && self.points[i].origin.is_authoritative() {
                    return false;
                }
                self.points[i] = m;
            }
            Err(i) => self.points.insert(i, m),
        }
        true
    }
    pub fn remove_at(&mut self, frame: FrameStamp) -> Option<Measurement> {
        self.points.binary_search_by(|p| p.frame.cmp(&frame)).ok().map(|i| self.points.remove(i))
    }
    /// The last point strictly before `frame`.
    pub fn last_before(&self, frame: FrameStamp) -> Option<&Measurement> {
        let i = self.points.partition_point(|p| p.frame < frame);
        i.checked_sub(1).map(|i| &self.points[i])
    }
}

/// Optional analysis range. Tracking and exports stay inside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub start: Option<FrameStamp>,
    pub end: Option<FrameStamp>,
    /// Use every n-th frame for tracking (1 = every frame).
    #[serde(default = "one")]
    pub step: u32,
}
fn one() -> u32 {
    1
}

impl Clip {
    pub fn contains(&self, f: FrameStamp) -> bool {
        self.start.is_none_or(|s| f >= s) && self.end.is_none_or(|e| f <= e)
    }
}

/// Everything that undo/redo restores. View state such as the selected track lives elsewhere,
/// so selecting a track never cancels work, dirties the project or changes the revision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalysisState {
    pub tracks: Vec<Arc<Track>>,
    pub coords: CoordinateSystem,
    #[serde(default)]
    pub clip: Clip,
}

impl AnalysisState {
    pub fn new(image_height: f64) -> Self {
        Self { tracks: Vec::new(), coords: CoordinateSystem::bottom_left(image_height), clip: Clip { step: 1, ..Default::default() } }
    }
    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id).map(|t| t.as_ref())
    }
    /// Mutable access with copy-on-write: only the edited track is cloned.
    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id).map(Arc::make_mut)
    }
    pub fn point_count(&self) -> usize {
        self.tracks.iter().map(|t| t.points.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_keeps_order_and_respects_authority() {
        let mut t = Track::new("ball", 0);
        let f = |n| FrameStamp::new(n, 30);
        t.put(Measurement::manual(f(3), Point::new(3.0, 3.0)), true);
        t.put(Measurement::manual(f(1), Point::new(1.0, 1.0)), true);
        let auto = Measurement { origin: Origin::Automatic, ..Measurement::manual(f(3), Point::new(9.0, 9.0)) };
        assert!(!t.put(auto.clone(), true));
        assert_eq!(t.point_at(f(3)).unwrap().pixel, Point::new(3.0, 3.0));
        assert!(t.put(auto, false));
        assert_eq!(t.points.iter().map(|p| p.frame).collect::<Vec<_>>(), vec![f(1), f(3)]);
        assert_eq!(t.last_before(f(3)).unwrap().frame, f(1));
        assert!(t.remove_at(f(1)).is_some());
    }

    #[test]
    fn copy_on_write_only_clones_edited_track() {
        let mut s = AnalysisState::new(100.0);
        s.tracks.push(Arc::new(Track::new("a", 0)));
        s.tracks.push(Arc::new(Track::new("b", 1)));
        let snapshot = s.clone();
        let id = s.tracks[0].id;
        s.track_mut(id).unwrap().name = "renamed".into();
        assert!(Arc::ptr_eq(&s.tracks[1], &snapshot.tracks[1]));
        assert!(!Arc::ptr_eq(&s.tracks[0], &snapshot.tracks[0]));
    }
}
