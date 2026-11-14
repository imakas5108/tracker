use crate::{AppError, Result, fail};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracker_core::{AnalysisState, FrameStamp, History, Project, SourceRef, Track, TrackId, TrackingSettings};
use tracker_media::VideoInfo;
use tracker_vision::{AutoTracker, Evidence};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Default)]
pub enum SessionStatus {
    #[default]
    Loading,
    Ready,
    Failed(String),
}

/// A decoded frame ready to draw (straight RGBA, orientation-corrected).
#[derive(Debug)]
pub struct DisplayFrame {
    pub stamp: FrameStamp,
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// An uncertain frame waiting for an explicit decision. Holds the paused tracker so
/// continuing keeps its motion history and template.
pub struct PendingReview {
    pub job_id: Uuid,
    pub track_id: TrackId,
    pub frame: FrameStamp,
    pub evidence: Evidence,
    pub(crate) tracker: Option<AutoTracker>,
}

pub struct Session {
    pub id: Uuid,
    pub path: PathBuf,
    pub name: String,
    pub status: SessionStatus,
    pub info: Option<Arc<VideoInfo>>,
    pub fingerprint: Option<String>,
    pub history: History,
    pub settings: TrackingSettings,
    /// Bumped by non-history changes that agents must observe (tracking settings).
    pub(crate) meta_revision: u64,
    pub(crate) saved_revision: u64,
    pub project_path: Option<PathBuf>,
    /// Frame shown in the native viewer. Independent of agent marking and frame inspection.
    pub viewer: FrameStamp,
    pub display: Option<Arc<DisplayFrame>>,
    pub display_error: Option<String>,
    pub(crate) display_generation: u64,
    cache: Vec<Arc<DisplayFrame>>,
    pub(crate) playing: Option<Arc<AtomicBool>>,
    pub selected_track: Option<TrackId>,
    pub review: Option<PendingReview>,
}

const CACHE: usize = 12;

impl Session {
    pub(crate) fn loading(path: PathBuf, project_path: Option<PathBuf>) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
        Self {
            id: Uuid::new_v4(),
            path,
            name,
            status: SessionStatus::Loading,
            info: None,
            fingerprint: None,
            history: History::new(AnalysisState::new(0.0)),
            settings: TrackingSettings::default(),
            meta_revision: 0,
            saved_revision: 0,
            project_path,
            viewer: FrameStamp::ZERO,
            display: None,
            display_error: None,
            display_generation: 0,
            cache: Vec::new(),
            playing: None,
            selected_track: None,
            review: None,
        }
    }

    pub(crate) fn make_ready(&mut self, info: Arc<VideoInfo>, project: Option<Project>, fingerprint: Option<String>) {
        self.viewer = info.index.first().unwrap_or(FrameStamp::ZERO);
        match project {
            Some(p) => {
                self.history = History::new(p.analysis);
                self.settings = p.settings;
                self.selected_track = p.selected_track;
                if let Some(f) = p.current_frame.and_then(|f| info.index.stamp_at(f.seconds())) {
                    self.viewer = f;
                }
            }
            None => self.history = History::new(AnalysisState::new(info.height as f64)),
        }
        self.saved_revision = self.revision();
        self.info = Some(info);
        self.fingerprint = fingerprint;
        self.status = SessionStatus::Ready;
    }

    /// Analysis revision: changes on every analysis edit, tracking delivery, undo/redo and
    /// settings change. Selecting a track, seeking or playing never changes it.
    pub fn revision(&self) -> u64 {
        self.history.revision() + self.meta_revision
    }

    pub fn is_dirty(&self) -> bool {
        self.status == SessionStatus::Ready && self.revision() != self.saved_revision
    }

    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    pub(crate) fn stop_playback(&mut self) {
        if let Some(p) = self.playing.take() {
            p.store(true, Ordering::Relaxed);
        }
    }

    pub fn ready(&self) -> Result<&Arc<VideoInfo>> {
        match (&self.status, &self.info) {
            (SessionStatus::Ready, Some(i)) => Ok(i),
            (SessionStatus::Failed(m), _) => fail("media_error", m.clone()),
            _ => fail("session_loading", "The video is still loading; wait for the import job to complete."),
        }
    }

    pub fn require_track(&self, id: TrackId) -> Result<&Track> {
        self.history.state().track(id).ok_or_else(|| AppError::new("track_not_found", "No track with this ID in the session."))
    }

    pub fn tracks(&self) -> &[Arc<Track>] {
        &self.history.state().tracks
    }

    pub(crate) fn cache_get(&self, stamp: FrameStamp) -> Option<Arc<DisplayFrame>> {
        self.cache.iter().find(|f| f.stamp == stamp).cloned()
    }
    pub(crate) fn cache_put(&mut self, f: Arc<DisplayFrame>) {
        self.cache.retain(|x| x.stamp != f.stamp);
        self.cache.push(f);
        if self.cache.len() > CACHE {
            self.cache.remove(0);
        }
    }

    /// A project snapshot and the revision it represents.
    pub(crate) fn snapshot(&self) -> Result<(Project, u64)> {
        let info = self.ready()?;
        let source = SourceRef { absolute: self.path.clone(), relative: None, width: info.width as f64, height: info.height as f64, fingerprint: self.fingerprint.clone() };
        let mut p = Project::new(source, self.history.state().clone(), self.settings);
        p.current_frame = Some(self.viewer);
        p.selected_track = self.selected_track.filter(|t| self.history.state().track(*t).is_some());
        Ok((p, self.revision()))
    }
}
