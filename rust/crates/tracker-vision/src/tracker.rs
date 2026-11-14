use crate::frame::GrayFrame;
use crate::matcher::{Match, MatchError, Matcher, NccMatcher, SsdMatcher};
use serde::Serialize;
use tracker_core::{Algorithm, Point, ReviewPolicy, TrackingSettings};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewReason {
    LowScore,
    Ambiguous,
    AtSearchBoundary,
    OutsideImage,
}

impl ReviewReason {
    pub fn message(&self) -> &'static str {
        match self {
            ReviewReason::LowScore => "Appearance differs from the template.",
            ReviewReason::Ambiguous => "Competing matches have similar scores.",
            ReviewReason::AtSearchBoundary => "Best match is at the search boundary.",
            ReviewReason::OutsideImage => "The search region lies outside the image.",
        }
    }
}

/// Everything a person or agent needs to judge an uncertain frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Evidence {
    pub candidate: Option<Point>,
    pub predicted: Option<Point>,
    pub score: f64,
    pub separation: f64,
    pub reason: ReviewReason,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StepOutcome {
    Accepted(Match),
    /// Uncertain, but the policy allows a gap; nothing is recorded for this frame.
    Skipped(Evidence),
    /// Uncertain and the policy (or gap limit) requires an explicit decision.
    Review(Evidence),
}

/// Sequential autotracking state for one track segment.
pub struct AutoTracker {
    settings: TrackingSettings,
    matcher: Box<dyn Matcher>,
    /// Recent accepted positions with their times, newest last.
    recent: Vec<(f64, Point)>,
    gap: u32,
    width: usize,
    height: usize,
}

impl AutoTracker {
    pub fn new(seed_frame: &GrayFrame, seed: Point, seed_time: f64, settings: TrackingSettings) -> Result<Self, MatchError> {
        settings.validate().map_err(|_| MatchError::InvalidTemplate)?;
        let r = settings.template_radius as usize;
        let matcher: Box<dyn Matcher> = match settings.algorithm {
            Algorithm::SsdV1 => Box::new(SsdMatcher::new(seed_frame, seed, r)?),
            Algorithm::NccV2 => Box::new(NccMatcher::new(seed_frame, seed, r, settings.mask)?),
        };
        Ok(Self { settings, matcher, recent: vec![(seed_time, seed)], gap: 0, width: seed_frame.width, height: seed_frame.height })
    }

    pub fn settings(&self) -> &TrackingSettings {
        &self.settings
    }

    /// Where the target should be at `time`, from the last one to three accepted positions.
    pub fn predict(&self, time: f64) -> Point {
        let n = self.recent.len();
        let last = self.recent[n - 1].1;
        if !self.settings.predict || n < 2 {
            return last;
        }
        let p = if n >= 3 {
            // Quadratic (constant acceleration) through the last three points, by Lagrange form.
            let [(t0, p0), (t1, p1), (t2, p2)] = [self.recent[n - 3], self.recent[n - 2], self.recent[n - 1]];
            let l0 = (time - t1) * (time - t2) / ((t0 - t1) * (t0 - t2));
            let l1 = (time - t0) * (time - t2) / ((t1 - t0) * (t1 - t2));
            let l2 = (time - t0) * (time - t1) / ((t2 - t0) * (t2 - t1));
            Point::new(l0 * p0.x + l1 * p1.x + l2 * p2.x, l0 * p0.y + l1 * p1.y + l2 * p2.y)
        } else {
            let [(t0, p0), (t1, p1)] = [self.recent[0], self.recent[1]];
            let k = (time - t1) / (t1 - t0);
            Point::new(p1.x + (p1.x - p0.x) * k, p1.y + (p1.y - p0.y) * k)
        };
        if p.is_finite() { Point::new(p.x.clamp(0.0, self.width as f64 - 1.0), p.y.clamp(0.0, self.height as f64 - 1.0)) } else { last }
    }

    fn judge(&self, m: &Match) -> Option<ReviewReason> {
        if m.score < self.settings.minimum_score {
            Some(ReviewReason::LowScore)
        } else if m.separation < self.settings.minimum_separation {
            Some(ReviewReason::Ambiguous)
        } else if m.at_boundary {
            Some(ReviewReason::AtSearchBoundary)
        } else {
            None
        }
    }

    /// Processes the next frame in time order.
    pub fn step(&mut self, frame: &GrayFrame, time: f64) -> StepOutcome {
        // Until two positions give a velocity, search twice as far to acquire the motion.
        let base = self.settings.search_radius as usize;
        let search = if self.settings.predict && self.recent.len() < 2 { base * 2 } else { base };
        let predicted = self.predict(time);
        let last = self.recent.last().unwrap().1;
        let primary = self.matcher.find(frame, predicted, search);
        // Bounded secondary assessment: if the predicted search is uncertain, also try around the
        // last accepted position and keep whichever candidate is acceptable or scores higher.
        let candidate = match primary {
            Ok(m) if self.judge(&m).is_none() => Ok(m),
            first if self.settings.predict && predicted.distance(last) > 1.0 => match (first, self.matcher.find(frame, last, search)) {
                (Ok(a), Ok(b)) => Ok(if self.judge(&b).is_none() || b.score > a.score { b } else { a }),
                (Err(_), Ok(b)) => Ok(b),
                (a, Err(_)) => a,
            },
            other => other,
        };
        let evidence = |m: Option<Match>, reason| Evidence { candidate: m.map(|m| m.position), predicted: Some(predicted), score: m.map_or(0.0, |m| m.score), separation: m.map_or(0.0, |m| m.separation), reason };
        let (m, reason) = match candidate {
            Ok(m) => (Some(m), self.judge(&m)),
            Err(_) => (None, Some(ReviewReason::OutsideImage)),
        };
        match (m, reason) {
            (Some(m), None) => {
                self.accept_position(frame, time, m.position, true);
                StepOutcome::Accepted(m)
            }
            (m, Some(reason)) => {
                let e = evidence(m, reason);
                if self.settings.review_policy == ReviewPolicy::SkipUncertain && self.gap < self.settings.max_gap {
                    self.gap += 1;
                    StepOutcome::Skipped(e)
                } else {
                    StepOutcome::Review(e)
                }
            }
            (None, None) => unreachable!(),
        }
    }

    fn accept_position(&mut self, frame: &GrayFrame, time: f64, position: Point, evolve: bool) {
        if evolve {
            self.matcher.evolve(frame, position, self.settings.evolve_rate, self.settings.tether);
        }
        self.recent.push((time, position));
        if self.recent.len() > 3 {
            self.recent.remove(0);
        }
        self.gap = 0;
    }

    /// Records an explicitly accepted or corrected position and makes it the new key frame.
    pub fn accept_correction(&mut self, frame: &GrayFrame, time: f64, position: Point) -> Result<(), MatchError> {
        self.matcher.rekey(frame, position)?;
        self.accept_position(frame, time, position, false);
        Ok(())
    }

    /// Continues past an uncertain frame without recording it.
    pub fn skip(&mut self) {
        self.gap += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::tests::{background, draw_target};

    fn run(settings: TrackingSettings, path: impl Fn(usize) -> (f64, f64), frames: usize, occlude: Option<std::ops::Range<usize>>) -> (Vec<(usize, Point)>, Option<(usize, Evidence)>) {
        let dt = 1.0 / 30.0;
        let make = |i: usize| {
            let mut f = background(320, 240, i as u64 + 10);
            if !occlude.as_ref().is_some_and(|r| r.contains(&i)) {
                let (x, y) = path(i);
                draw_target(&mut f, x, y, 1.0, 0.0);
            }
            f
        };
        let (sx, sy) = path(0);
        let mut t = AutoTracker::new(&make(0), Point::new(sx, sy), 0.0, settings).unwrap();
        let mut accepted = vec![];
        for i in 1..frames {
            match t.step(&make(i), i as f64 * dt) {
                StepOutcome::Accepted(m) => accepted.push((i, m.position)),
                StepOutcome::Skipped(_) => {}
                StepOutcome::Review(e) => return (accepted, Some((i, e))),
            }
        }
        (accepted, None)
    }

    /// A thrown ball: 11 px/frame across and 25 px/frame up at release, then falling
    /// (similar to the BallToss sample, where the Swift baseline stopped on the first frame).
    fn toss(i: usize) -> (f64, f64) {
        let t = i as f64;
        (20.0 + 11.0 * t, 210.0 - 26.0 * t + 1.0 * t * t)
    }

    #[test]
    fn prediction_follows_fast_motion_that_stops_the_swift_baseline() {
        let (_, review) = run(TrackingSettings::swift_v1(), toss, 27, None);
        assert!(review.is_some(), "fixed 24 px search without prediction loses the target");
        let mut s = TrackingSettings::default();
        s.search_radius = 16;
        let (acc, review) = run(s, toss, 27, None);
        assert!(review.is_none(), "{review:?}");
        assert_eq!(acc.len(), 26);
        for (i, p) in acc {
            let (x, y) = toss(i);
            assert!(p.distance(Point::new(x, y)) < 0.75, "frame {i}: {p:?}");
        }
    }

    #[test]
    fn occlusion_pauses_or_skips_by_policy() {
        let slow = |i: usize| (60.0 + 2.0 * i as f64, 100.0);
        let (acc, review) = run(TrackingSettings::default(), slow, 30, Some(10..14));
        let (frame, e) = review.unwrap();
        assert_eq!(frame, 10);
        assert_eq!(acc.len(), 9);
        assert!(e.candidate.is_some() && e.predicted.is_some());

        let mut s = TrackingSettings::default();
        s.review_policy = ReviewPolicy::SkipUncertain;
        s.max_gap = 6;
        let (acc, review) = run(s, slow, 30, Some(10..14));
        assert!(review.is_none(), "{review:?}");
        assert_eq!(acc.len(), 29 - 4, "four occluded frames left as gaps");
        assert!(acc.iter().all(|(i, _)| !(10..14).contains(i)));
    }

    #[test]
    fn gap_limit_turns_skips_into_review() {
        let mut s = TrackingSettings::default();
        s.review_policy = ReviewPolicy::SkipUncertain;
        s.max_gap = 2;
        let (_, review) = run(s, |i| (60.0 + i as f64, 100.0), 30, Some(10..20));
        assert_eq!(review.unwrap().0, 12);
    }
}
