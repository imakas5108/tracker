//! Autotracking on grayscale (luma) frames.
//!
//! Two matchers share one interface:
//! - [`SsdMatcher`] reproduces the Swift baseline exactly (fixed template, mean squared difference),
//!   so Rust results can be compared with Swift results on the same footage.
//! - [`NccMatcher`] uses zero-mean normalized cross-correlation over a circular or square mask,
//!   which ignores uniform brightness changes and is not dominated by plain background.
//!
//! [`AutoTracker`] adds motion prediction, a bounded second search around the last accepted
//! position, tethered template evolution and the review policy. Scores are algorithm-specific
//! similarity values, never probabilities.

mod frame;
mod matcher;
mod tracker;

pub use frame::GrayFrame;
pub use matcher::{Match, MatchError, Matcher, NccMatcher, SsdMatcher};
pub use tracker::{AutoTracker, Evidence, ReviewReason, StepOutcome};
