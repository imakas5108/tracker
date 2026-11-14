//! Video access.
//!
//! The macOS backend uses AVFoundation directly: the frame index is built from the container's
//! sample timing without decoding, and tracking frames are decoded as 4:2:0 YpCbCr so luma is
//! copied straight from the Y plane (no color conversion, no intermediate images). Other
//! platforms can add a backend behind the same functions later.

use tracker_core::{FrameIndex, FrameStamp};
use tracker_vision::GrayFrame;

#[cfg(target_os = "macos")]
mod avf;

#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum MediaError {
    #[error("No readable video track was found in {0}.")]
    NoVideo(String),
    #[error("The video could not be read: {0}")]
    Read(String),
    #[error("The requested frame is not in this video.")]
    FrameNotFound,
    #[error("Video decoding is not available on this platform yet.")]
    Unsupported,
}

/// Rotation needed to show stored frames upright (from the track's preferred transform).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Up,
    Right,
    Down,
    Left,
}

#[derive(Clone, Debug)]
pub struct VideoInfo {
    pub path: std::path::PathBuf,
    /// Orientation-corrected size in pixels.
    pub width: usize,
    pub height: usize,
    pub duration: f64,
    pub nominal_fps: f64,
    pub orientation: Orientation,
    pub index: FrameIndex,
}

/// A decoded, orientation-corrected frame.
#[derive(Clone, Debug)]
pub struct Frame {
    pub stamp: FrameStamp,
    pub luma: GrayFrame,
    /// Straight RGBA, present when requested for display.
    pub rgba: Option<Vec<u8>>,
}

/// Opens a video and indexes every frame's presentation timestamp.
pub fn open(path: &std::path::Path) -> Result<VideoInfo, MediaError> {
    #[cfg(target_os = "macos")]
    {
        avf::open(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(MediaError::Unsupported)
    }
}

/// Decodes the exact frame at `stamp`.
pub fn frame_at(info: &VideoInfo, stamp: FrameStamp, rgba: bool) -> Result<Frame, MediaError> {
    let mut reader = FrameReader::new(info, stamp, rgba)?;
    while let Some(f) = reader.next_frame()? {
        if f.stamp == stamp {
            return Ok(f);
        }
        if f.stamp > stamp {
            break;
        }
    }
    Err(MediaError::FrameNotFound)
}

/// Sequential decoder starting at (or just before) a stamp. Holds one decoded frame at a time.
pub struct FrameReader {
    #[cfg(target_os = "macos")]
    inner: avf::Reader,
}

impl FrameReader {
    pub fn new(info: &VideoInfo, start: FrameStamp, rgba: bool) -> Result<Self, MediaError> {
        #[cfg(target_os = "macos")]
        {
            Ok(Self { inner: avf::Reader::new(info, start, rgba)? })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (info, start, rgba);
            Err(MediaError::Unsupported)
        }
    }
    pub fn next_frame(&mut self) -> Result<Option<Frame>, MediaError> {
        #[cfg(target_os = "macos")]
        {
            self.inner.next_frame()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(None)
        }
    }
}

/// Rotates a single-channel or 4-channel image to upright orientation.
pub(crate) fn orient(data: &[u8], w: usize, h: usize, channels: usize, o: Orientation) -> (Vec<u8>, usize, usize) {
    if o == Orientation::Up {
        return (data.to_vec(), w, h);
    }
    let (ow, oh) = match o {
        Orientation::Right | Orientation::Left => (h, w),
        _ => (w, h),
    };
    let mut out = vec![0u8; data.len()];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match o {
                Orientation::Right => (h - 1 - y, x),
                Orientation::Left => (y, w - 1 - x),
                Orientation::Down => (w - 1 - x, h - 1 - y),
                Orientation::Up => (x, y),
            };
            let s = (y * w + x) * channels;
            let d = (ny * ow + nx) * channels;
            out[d..d + channels].copy_from_slice(&data[s..s + channels]);
        }
    }
    (out, ow, oh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_rotates_corners() {
        // 3x2 image: values are x + 10y.
        let img = [0u8, 1, 2, 10, 11, 12];
        let (r, w, h) = orient(&img, 3, 2, 1, Orientation::Right);
        assert_eq!((w, h), (2, 3));
        assert_eq!(r, vec![10, 0, 11, 1, 12, 2]);
        let (d, _, _) = orient(&img, 3, 2, 1, Orientation::Down);
        assert_eq!(d, vec![12, 11, 10, 2, 1, 0]);
    }

    /// Uses the Java Tracker sample clips if present (they are not part of the repository).
    fn sample(name: &str) -> Option<std::path::PathBuf> {
        let p = std::path::PathBuf::from(std::env::var("HOME").ok()?).join("Tracker/videos").join(name);
        p.exists().then_some(p)
    }

    #[test]
    fn indexes_and_decodes_balltoss() {
        let Some(path) = sample("BallTossOut.mp4") else {
            eprintln!("SKIPPED: ~/Tracker/videos/BallTossOut.mp4 not found");
            return;
        };
        let info = open(&path).unwrap();
        assert_eq!((info.width, info.height), (640, 480));
        let first = info.index.first().unwrap();
        assert!((first.seconds() - 2.0 / 30.0).abs() < 1e-6, "first PTS {first:?}");
        assert!(info.index.len() >= 30);
        let stamp = info.index.stamp_at(0.4).unwrap();
        let f = frame_at(&info, stamp, true).unwrap();
        assert_eq!(f.stamp, stamp);
        assert_eq!((f.luma.width, f.luma.height), (640, 480));
        assert_eq!(f.rgba.as_ref().unwrap().len(), 640 * 480 * 4);
        // The ball is bright at (176, 154) on frame 12 (0.4 s); the background is dark.
        assert!(f.luma.at(176, 154) > 150, "ball luma {}", f.luma.at(176, 154));
        assert!(f.luma.at(400, 300) < 60, "background luma {}", f.luma.at(400, 300));
        // Sequential reading yields every indexed frame in order.
        let mut r = FrameReader::new(&info, info.index.first().unwrap(), false).unwrap();
        let mut n = 0;
        while let Some(fr) = r.next_frame().unwrap() {
            assert_eq!(Some(fr.stamp), info.index.get(n));
            n += 1;
        }
        assert_eq!(n, info.index.len());
    }
}
