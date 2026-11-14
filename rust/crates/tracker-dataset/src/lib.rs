//! Datasets collect tracking runs from many videos into two tidy tables:
//!
//! - `runs`: one row per stored run (video, track, calibration, algorithm, conditions, fit summary)
//! - `observations`: one row per measured point (time, position, velocity, acceleration, provenance)
//!
//! A dataset lives in its own folder (`dataset.json`, `runs.csv`, `observations.csv`) and is
//! rewritten atomically on every change. It exports to CSV and XLSX, both of which Google Sheets
//! imports directly, and to row arrays shaped like the Google Sheets API `values` field so an
//! agent with a Sheets connector can transfer it without files.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tracker_core::io::atomic_write;
use tracker_core::kinematics::KinematicRow;
use uuid::Uuid;

mod fit;
pub use fit::{FitSummary, fit_motion};

#[derive(Debug, thiserror::Error)]
pub enum DatasetError {
    #[error("no dataset named {0:?}")]
    NotFound(String),
    #[error("a dataset named {0:?} already exists")]
    Exists(String),
    #[error("invalid dataset name: use letters, digits, spaces, dots, dashes or underscores")]
    InvalidName,
    #[error("run {0} is not in this dataset")]
    RunNotFound(Uuid),
    #[error("unknown sheet {0:?}; use runs or observations")]
    UnknownSheet(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Csv(#[from] csv::Error),
    #[error("{0}")]
    Xlsx(#[from] rust_xlsxwriter::XlsxError),
    #[error("{0}")]
    Write(#[from] tracker_core::io::WriteError),
}

/// Metadata for one stored run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub run_id: Uuid,
    pub added_at: String,
    /// Who stored it: "user" or the MCP client name.
    pub added_by: String,
    pub video_path: String,
    pub video_name: String,
    pub video_fingerprint: Option<String>,
    pub track_id: Uuid,
    pub track_name: String,
    pub unit: String,
    pub pixels_per_unit: Option<f64>,
    pub origin_x: f64,
    pub origin_y: f64,
    pub axes_angle_deg: f64,
    pub algorithm: String,
    pub settings_json: String,
    pub points: usize,
    pub manual_points: usize,
    pub automatic_points: usize,
    pub reviewed_points: usize,
    pub t_start: Option<f64>,
    pub t_end: Option<f64>,
    /// Experimental conditions supplied by the person or agent, e.g. {"mass_kg": "0.05"}.
    pub conditions: BTreeMap<String, String>,
    pub notes: String,
    pub fit: Option<FitSummary>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub run_id: Uuid,
    pub t: f64,
    pub frame_value: i64,
    pub frame_timescale: i32,
    pub pixel_x: f64,
    pub pixel_y: f64,
    pub x: f64,
    pub y: f64,
    pub vx: Option<f64>,
    pub vy: Option<f64>,
    pub speed: Option<f64>,
    pub ax: Option<f64>,
    pub ay: Option<f64>,
    pub unit: String,
    pub origin: String,
    pub score: Option<f64>,
}

impl Observation {
    pub fn from_row(run_id: Uuid, unit: &str, r: &KinematicRow) -> Self {
        Self {
            run_id,
            t: r.t,
            frame_value: r.frame.value,
            frame_timescale: r.frame.timescale,
            pixel_x: r.pixel_x,
            pixel_y: r.pixel_y,
            x: r.x,
            y: r.y,
            vx: r.vx,
            vy: r.vy,
            speed: r.speed,
            ax: r.ax,
            ay: r.ay,
            unit: unit.to_string(),
            origin: r.origin.name().to_string(),
            score: r.score,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
    pub runs: Vec<RunRecord>,
}

/// A dataset loaded in memory, saved back to its folder after each change.
#[derive(Clone, Debug)]
pub struct Dataset {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub observations: Vec<Observation>,
}

pub const RUN_COLUMNS: [&str; 24] = [
    "run_id", "added_at", "added_by", "video_name", "video_path", "track_name", "track_id", "unit", "pixels_per_unit", "origin_x_px", "origin_y_px", "axes_angle_deg", "algorithm", "points",
    "manual_points", "automatic_points", "reviewed_points", "t_start_s", "t_end_s", "fit_ax", "fit_ay", "fit_r2_y", "conditions", "notes",
];
pub const OBSERVATION_COLUMNS: [&str; 16] = ["run_id", "t_s", "frame_value", "frame_timescale", "pixel_x", "pixel_y", "x", "y", "vx", "vy", "speed", "ax", "ay", "unit", "origin", "match_score"];

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 80 && name.chars().all(|c| c.is_alphanumeric() || " ._-".contains(c)) && !name.starts_with('.')
}

/// A cell value for exports.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Cell {
    Text(String),
    Number(f64),
    Empty,
}

fn num(v: Option<f64>) -> Cell {
    v.filter(|v| v.is_finite()).map_or(Cell::Empty, Cell::Number)
}

impl RunRecord {
    pub fn cells(&self) -> Vec<Cell> {
        use Cell::*;
        let conditions = self.conditions.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; ");
        vec![
            Text(self.run_id.to_string()),
            Text(self.added_at.clone()),
            Text(self.added_by.clone()),
            Text(self.video_name.clone()),
            Text(self.video_path.clone()),
            Text(self.track_name.clone()),
            Text(self.track_id.to_string()),
            Text(self.unit.clone()),
            num(self.pixels_per_unit),
            Number(self.origin_x),
            Number(self.origin_y),
            Number(self.axes_angle_deg),
            Text(self.algorithm.clone()),
            Number(self.points as f64),
            Number(self.manual_points as f64),
            Number(self.automatic_points as f64),
            Number(self.reviewed_points as f64),
            num(self.t_start),
            num(self.t_end),
            num(self.fit.as_ref().and_then(|f| f.ax)),
            num(self.fit.as_ref().and_then(|f| f.ay)),
            num(self.fit.as_ref().and_then(|f| f.r2_y)),
            Text(conditions),
            Text(self.notes.clone()),
        ]
    }
}

impl Observation {
    pub fn cells(&self) -> Vec<Cell> {
        use Cell::*;
        vec![
            Text(self.run_id.to_string()),
            Number(self.t),
            Number(self.frame_value as f64),
            Number(self.frame_timescale as f64),
            Number(self.pixel_x),
            Number(self.pixel_y),
            Number(self.x),
            Number(self.y),
            num(self.vx),
            num(self.vy),
            num(self.speed),
            num(self.ax),
            num(self.ay),
            Text(self.unit.clone()),
            Text(self.origin.clone()),
            num(self.score),
        ]
    }
}

impl Dataset {
    /// Default folder: `$TRACKER_DATASETS_DIR`, else `~/Documents/Tracker Datasets`.
    pub fn default_root() -> PathBuf {
        if let Some(p) = std::env::var_os("TRACKER_DATASETS_DIR") {
            return PathBuf::from(p);
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        home.join("Documents").join("Tracker Datasets")
    }

    pub fn create(root: &Path, name: &str, description: &str) -> Result<Self, DatasetError> {
        if !valid_name(name) {
            return Err(DatasetError::InvalidName);
        }
        let dir = root.join(name.trim());
        if dir.join("dataset.json").exists() {
            return Err(DatasetError::Exists(name.into()));
        }
        std::fs::create_dir_all(&dir)?;
        let t = now();
        let ds = Self {
            dir,
            manifest: Manifest { format: "tracker-dataset".into(), version: 1, id: Uuid::new_v4(), name: name.trim().into(), description: description.into(), created_at: t.clone(), updated_at: t, runs: vec![] },
            observations: vec![],
        };
        ds.save()?;
        Ok(ds)
    }

    pub fn open(root: &Path, name: &str) -> Result<Self, DatasetError> {
        if !valid_name(name) {
            return Err(DatasetError::InvalidName);
        }
        let dir = root.join(name.trim());
        let manifest_path = dir.join("dataset.json");
        if !manifest_path.exists() {
            return Err(DatasetError::NotFound(name.into()));
        }
        let manifest: Manifest = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
        let mut observations = vec![];
        let obs_path = dir.join("observations.csv");
        if obs_path.exists() {
            let mut r = csv::Reader::from_path(obs_path)?;
            for row in r.deserialize() {
                observations.push(row?);
            }
        }
        Ok(Self { dir, manifest, observations })
    }

    pub fn list(root: &Path) -> Result<Vec<Manifest>, DatasetError> {
        let mut out = vec![];
        if !root.exists() {
            return Ok(out);
        }
        for entry in std::fs::read_dir(root)? {
            let p = entry?.path().join("dataset.json");
            if let Ok(bytes) = std::fs::read(&p) {
                if let Ok(m) = serde_json::from_slice::<Manifest>(&bytes) {
                    out.push(m);
                }
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// Adds a run and its observations, then saves.
    pub fn add_run(&mut self, mut record: RunRecord, observations: Vec<Observation>) -> Result<Uuid, DatasetError> {
        record.added_at = if record.added_at.is_empty() { now() } else { record.added_at };
        let id = record.run_id;
        self.observations.extend(observations.into_iter().map(|mut o| {
            o.run_id = id;
            o
        }));
        self.manifest.runs.push(record);
        self.save()?;
        Ok(id)
    }

    pub fn remove_run(&mut self, run_id: Uuid) -> Result<(), DatasetError> {
        let before = self.manifest.runs.len();
        self.manifest.runs.retain(|r| r.run_id != run_id);
        if self.manifest.runs.len() == before {
            return Err(DatasetError::RunNotFound(run_id));
        }
        self.observations.retain(|o| o.run_id != run_id);
        self.save()
    }

    pub fn save(&self) -> Result<(), DatasetError> {
        let mut m = self.manifest.clone();
        m.updated_at = now();
        atomic_write(&self.dir.join("dataset.json"), &serde_json::to_vec_pretty(&m)?, true)?;
        atomic_write(&self.dir.join("runs.csv"), &self.csv("runs")?, true)?;
        atomic_write(&self.dir.join("observations.csv"), &self.observations_csv()?, true)?;
        Ok(())
    }

    fn observations_csv(&self) -> Result<Vec<u8>, DatasetError> {
        // Serde field names match OBSERVATION_COLUMNS order so the file reloads losslessly.
        let mut w = csv::Writer::from_writer(vec![]);
        for o in &self.observations {
            w.serialize(o)?;
        }
        if self.observations.is_empty() {
            w.write_record(["run_id", "t", "frame_value", "frame_timescale", "pixel_x", "pixel_y", "x", "y", "vx", "vy", "speed", "ax", "ay", "unit", "origin", "score"])?;
        }
        Ok(w.into_inner().map_err(|e| std::io::Error::other(e.to_string()))?)
    }

    /// Header and rows of a sheet ("runs" or "observations").
    pub fn sheet(&self, name: &str) -> Result<(Vec<&'static str>, Vec<Vec<Cell>>), DatasetError> {
        match name {
            "runs" => Ok((RUN_COLUMNS.to_vec(), self.manifest.runs.iter().map(|r| r.cells()).collect())),
            "observations" => Ok((OBSERVATION_COLUMNS.to_vec(), self.observations.iter().map(|o| o.cells()).collect())),
            other => Err(DatasetError::UnknownSheet(other.into())),
        }
    }

    /// CSV text of a sheet, with spreadsheet-friendly headers.
    pub fn csv(&self, name: &str) -> Result<Vec<u8>, DatasetError> {
        let (header, rows) = self.sheet(name)?;
        let mut w = csv::Writer::from_writer(vec![]);
        w.write_record(&header)?;
        for row in rows {
            w.write_record(row.iter().map(|c| match c {
                Cell::Text(s) => s.clone(),
                Cell::Number(n) => format!("{n}"),
                Cell::Empty => String::new(),
            }))?;
        }
        Ok(w.into_inner().map_err(|e| std::io::Error::other(e.to_string()))?)
    }

    /// Rows shaped like the Google Sheets API `values` field: the header row, then data rows.
    pub fn sheet_values(&self, name: &str, offset: usize, limit: usize) -> Result<(Vec<Vec<Cell>>, usize), DatasetError> {
        let (header, rows) = self.sheet(name)?;
        let total = rows.len();
        let mut values = vec![header.iter().map(|h| Cell::Text(h.to_string())).collect()];
        values.extend(rows.into_iter().skip(offset).take(limit));
        Ok((values, total))
    }

    /// Writes an XLSX workbook with `runs`, `observations` and `about` sheets.
    pub fn export_xlsx(&self, dest: &Path, overwrite: bool) -> Result<(), DatasetError> {
        use rust_xlsxwriter::{Format, Workbook};
        let mut wb = Workbook::new();
        let bold = Format::new().set_bold();
        for name in ["runs", "observations"] {
            let (header, rows) = self.sheet(name)?;
            let ws = wb.add_worksheet().set_name(name)?;
            for (c, h) in header.iter().enumerate() {
                ws.write_string_with_format(0, c as u16, *h, &bold)?;
            }
            for (r, row) in rows.iter().enumerate() {
                for (c, cell) in row.iter().enumerate() {
                    match cell {
                        Cell::Text(s) => ws.write_string(r as u32 + 1, c as u16, s)?,
                        Cell::Number(n) => ws.write_number(r as u32 + 1, c as u16, *n)?,
                        Cell::Empty => ws,
                    };
                }
            }
            ws.set_freeze_panes(1, 0)?;
        }
        let about = wb.add_worksheet().set_name("about")?;
        let lines = [
            format!("Dataset: {}", self.manifest.name),
            format!("Description: {}", self.manifest.description),
            format!("Runs: {}, observations: {}", self.manifest.runs.len(), self.observations.len()),
            "Positions use the run's coordinate system: y points up; units are in the unit column (px when uncalibrated).".into(),
            "Velocities and accelerations are three-point finite differences on source timestamps; blank at track ends and across gaps.".into(),
            "origin: manual, automatic (autotracker) or reviewed (an uncertain match explicitly accepted). match_score is a similarity value, not a probability.".into(),
            "fit_ax, fit_ay: least-squares acceleration from a quadratic fit of x(t) and y(t); fit_r2_y is that fit's R².".into(),
        ];
        for (i, l) in lines.iter().enumerate() {
            about.write_string(i as u32, 0, l)?;
        }
        let bytes = wb.save_to_buffer()?;
        atomic_write(dest, &bytes, overwrite)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracker_core::{FrameStamp, Origin};

    fn row(i: i64) -> KinematicRow {
        KinematicRow { frame: FrameStamp::new(i, 30), t: i as f64 / 30.0, pixel_x: i as f64, pixel_y: 2.0 * i as f64, x: 0.01 * i as f64, y: 0.02 * i as f64, vx: (i > 0).then_some(0.3), vy: None, speed: None, ax: None, ay: None, origin: Origin::Automatic, score: Some(0.97) }
    }

    fn record() -> RunRecord {
        RunRecord {
            run_id: Uuid::new_v4(),
            added_at: String::new(),
            added_by: "codex".into(),
            video_path: "/v/ball.mp4".into(),
            video_name: "ball.mp4".into(),
            video_fingerprint: None,
            track_id: Uuid::new_v4(),
            track_name: "ball".into(),
            unit: "m".into(),
            pixels_per_unit: Some(100.0),
            origin_x: 0.0,
            origin_y: 480.0,
            axes_angle_deg: 0.0,
            algorithm: "masked-ncc-predictive-v2".into(),
            settings_json: "{}".into(),
            points: 3,
            manual_points: 1,
            automatic_points: 2,
            reviewed_points: 0,
            t_start: Some(0.0),
            t_end: Some(0.0667),
            conditions: BTreeMap::from([("mass_kg".into(), "0.05".into())]),
            notes: "test".into(),
            fit: None,
        }
    }

    #[test]
    fn create_add_reload_export() {
        let root = tempfile::tempdir().unwrap();
        let mut ds = Dataset::create(root.path(), "Ball toss 2026", "class lab").unwrap();
        assert!(matches!(Dataset::create(root.path(), "Ball toss 2026", ""), Err(DatasetError::Exists(_))));
        assert!(matches!(Dataset::create(root.path(), "../escape", ""), Err(DatasetError::InvalidName)));
        let rec = record();
        let obs: Vec<_> = (0..3).map(|i| Observation::from_row(Uuid::nil(), "m", &row(i))).collect();
        let id = ds.add_run(rec, obs).unwrap();
        let back = Dataset::open(root.path(), "Ball toss 2026").unwrap();
        assert_eq!(back.manifest.runs.len(), 1);
        assert_eq!(back.observations.len(), 3);
        assert!(back.observations.iter().all(|o| o.run_id == id));
        assert_eq!(back.observations[1].vx, Some(0.3));
        assert_eq!(back.observations[0].vx, None);

        let (values, total) = back.sheet_values("observations", 1, 10).unwrap();
        assert_eq!(total, 3);
        assert_eq!(values.len(), 3, "header plus two rows after offset 1");
        assert_eq!(values[0][1], Cell::Text("t_s".into()));

        let csv = String::from_utf8(back.csv("runs").unwrap()).unwrap();
        assert!(csv.starts_with("run_id,added_at,added_by"));
        assert!(csv.contains("mass_kg=0.05"));

        let x = root.path().join("out.xlsx");
        back.export_xlsx(&x, false).unwrap();
        assert!(std::fs::metadata(&x).unwrap().len() > 1000);
        assert!(back.export_xlsx(&x, false).is_err(), "no clobber");

        assert_eq!(Dataset::list(root.path()).unwrap().len(), 1);
        let mut ds = back;
        ds.remove_run(id).unwrap();
        assert!(Dataset::open(root.path(), "Ball toss 2026").unwrap().observations.is_empty());
    }
}
