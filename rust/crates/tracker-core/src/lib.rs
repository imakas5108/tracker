//! Scientific data model shared by the UI, the automation server and the MCP bridge.
//!
//! Nothing in this crate knows about video decoding, rendering or MCP. Measurements are keyed
//! by exact rational frame stamps rather than floating-point seconds, so two marks on the same
//! source frame always compare equal.

pub mod coords;
pub mod geom;
pub mod history;
pub mod io;
pub mod kinematics;
pub mod model;
pub mod project;
pub mod settings;
pub mod stamp;

pub use coords::{Calibration, CoordinateSystem, LengthUnit};
pub use geom::Point;
pub use history::History;
pub use kinematics::{KinematicRow, kinematics};
pub use model::{AnalysisState, Clip, Measurement, Origin, Track, TrackId};
pub use project::{Project, ProjectError, SourceRef};
pub use settings::{Algorithm, MaskShape, ReviewPolicy, TrackingSettings};
pub use stamp::{FrameIndex, FrameStamp};
