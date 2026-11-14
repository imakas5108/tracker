#![allow(deprecated)] // the synchronous AVAsset property getters are still available and simplest here

use crate::{Frame, MediaError, Orientation, VideoInfo, orient};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVAssetTrack, AVMediaTypeVideo, AVURLAsset};
use objc2_core_media::{CMSampleBuffer, CMSampleTimingInfo, CMTime, CMTimeFlags, CMTimeRange, kCMTimePositiveInfinity};
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBaseAddressOfPlane, CVPixelBufferGetBytesPerRow, CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferGetHeight, CVPixelBufferGetHeightOfPlane,
    CVPixelBufferGetWidth, CVPixelBufferGetWidthOfPlane, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelBufferPixelFormatTypeKey,
    kCVPixelFormatType_32BGRA, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use std::ptr::NonNull;
use tracker_core::{FrameIndex, FrameStamp};
use tracker_vision::GrayFrame;

fn err(e: Retained<objc2_foundation::NSError>) -> MediaError {
    MediaError::Read(e.localizedDescription().to_string())
}

fn asset_track(path: &std::path::Path) -> Result<(Retained<AVURLAsset>, Retained<AVAssetTrack>), MediaError> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    unsafe {
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        let video = AVMediaTypeVideo.ok_or_else(|| MediaError::NoVideo(path.display().to_string()))?;
        let tracks = asset.tracksWithMediaType(video);
        let track = tracks.firstObject().ok_or_else(|| MediaError::NoVideo(path.display().to_string()))?;
        Ok((asset, track))
    }
}

fn orientation(track: &AVAssetTrack) -> Orientation {
    let t = unsafe { track.preferredTransform() };
    let r = |v: f64| v.round() as i32;
    match (r(t.a), r(t.b), r(t.c), r(t.d)) {
        (0, 1, -1, 0) => Orientation::Right,
        (0, -1, 1, 0) => Orientation::Left,
        (-1, 0, 0, -1) => Orientation::Down,
        _ => Orientation::Up,
    }
}

fn stamp(t: CMTime) -> Option<FrameStamp> {
    (t.flags.contains(CMTimeFlags::Valid) && !t.flags.contains(CMTimeFlags::PositiveInfinity) && t.timescale > 0 && t.value >= 0).then(|| FrameStamp::new(t.value, t.timescale))
}

pub fn open(path: &std::path::Path) -> Result<VideoInfo, MediaError> {
    if !path.exists() {
        return Err(MediaError::Read(format!("{} does not exist", path.display())));
    }
    let (asset, track) = asset_track(path)?;
    let size = unsafe { track.naturalSize() };
    let o = orientation(&track);
    let (w, h) = match o {
        Orientation::Right | Orientation::Left => (size.height, size.width),
        _ => (size.width, size.height),
    };
    let duration = unsafe { asset.duration() };
    let fps = unsafe { track.nominalFrameRate() } as f64;
    // Index from compressed samples: no decoding.
    let mut stamps = Vec::new();
    unsafe {
        let reader = AVAssetReader::assetReaderWithAsset_error(&asset).map_err(err)?;
        let output = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, None);
        output.setAlwaysCopiesSampleData(false);
        reader.addOutput(&output);
        if !reader.startReading() {
            return Err(reader.error().map(err).unwrap_or(MediaError::Read("could not start reading".into())));
        }
        while let Some(sample) = output.copyNextSampleBuffer() {
            let n = sample.num_samples();
            for i in 0..n {
                let mut info = std::mem::zeroed::<CMSampleTimingInfo>();
                if sample.sample_timing_info(i, NonNull::from(&mut info)) == 0 {
                    if let Some(s) = stamp(info.presentationTimeStamp) {
                        stamps.push((s, stamp(info.duration)));
                    }
                }
            }
        }
        if reader.status() == AVAssetReaderStatus::Failed {
            return Err(reader.error().map(err).unwrap_or(MediaError::Read("reading failed".into())));
        }
    }
    // Samples that would end after the presentation range (edit lists, trailing frames) are
    // never delivered by the decoder, so they are left out of the index.
    let end = (duration.timescale > 0 && duration.value > 0).then(|| duration.value as f64 / duration.timescale as f64);
    let stamps = stamps
        .into_iter()
        .filter(|(s, d)| end.is_none_or(|end| s.seconds() + d.map_or(0.0, |d| d.seconds()) <= end + 1e-6))
        .map(|(s, _)| s)
        .collect();
    let index = FrameIndex::new(stamps);
    if index.is_empty() || w <= 0.0 || h <= 0.0 {
        return Err(MediaError::NoVideo(path.display().to_string()));
    }
    Ok(VideoInfo { path: path.to_path_buf(), width: w as usize, height: h as usize, duration: duration.value as f64 / duration.timescale.max(1) as f64, nominal_fps: fps, orientation: o, index })
}

pub struct Reader {
    reader: Retained<AVAssetReader>,
    output: Retained<AVAssetReaderTrackOutput>,
    orientation: Orientation,
    rgba: bool,
    done: bool,
}

impl Reader {
    pub fn new(info: &VideoInfo, start: FrameStamp, rgba: bool) -> Result<Self, MediaError> {
        let (asset, track) = asset_track(&info.path)?;
        unsafe {
            let reader = AVAssetReader::assetReaderWithAsset_error(&asset).map_err(err)?;
            let format = if rgba { kCVPixelFormatType_32BGRA } else { kCVPixelFormatType_420YpCbCr8BiPlanarFullRange };
            let key: &NSString = &*(kCVPixelBufferPixelFormatTypeKey as *const _ as *const NSString);
            let value = NSNumber::numberWithUnsignedInt(format);
            let settings = NSDictionary::<NSString, AnyObject>::from_slices(&[key], &[&*value as &AnyObject]);
            let output = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, Some(&settings));
            output.setAlwaysCopiesSampleData(false);
            reader.addOutput(&output);
            if start.value > 0 {
                let t = CMTime { value: start.value, timescale: start.timescale, flags: CMTimeFlags::Valid, epoch: 0 };
                reader.setTimeRange(CMTimeRange { start: t, duration: kCMTimePositiveInfinity });
            }
            if !reader.startReading() {
                return Err(reader.error().map(err).unwrap_or(MediaError::Read("could not start reading".into())));
            }
            Ok(Self { reader, output, orientation: info.orientation, rgba, done: false })
        }
    }

    pub fn next_frame(&mut self) -> Result<Option<Frame>, MediaError> {
        if self.done {
            return Ok(None);
        }
        objc2::rc::autoreleasepool(|_| unsafe {
            let Some(sample) = self.output.copyNextSampleBuffer() else {
                self.done = true;
                if self.reader.status() == AVAssetReaderStatus::Failed {
                    return Err(self.reader.error().map(err).unwrap_or(MediaError::Read("decoding failed".into())));
                }
                return Ok(None);
            };
            self.convert(&sample).map(Some)
        })
    }

    unsafe fn convert(&self, sample: &CMSampleBuffer) -> Result<Frame, MediaError> {
        unsafe {
            let s = stamp(sample.presentation_time_stamp()).ok_or(MediaError::Read("sample without timestamp".into()))?;
            let buf = sample.image_buffer().ok_or(MediaError::Read("sample without image".into()))?;
            if CVPixelBufferLockBaseAddress(&buf, CVPixelBufferLockFlags::ReadOnly) != 0 {
                return Err(MediaError::Read("could not lock frame".into()));
            }
            let result = if self.rgba {
                let (w, h, stride) = (CVPixelBufferGetWidth(&buf), CVPixelBufferGetHeight(&buf), CVPixelBufferGetBytesPerRow(&buf));
                let base = CVPixelBufferGetBaseAddress(&buf) as *const u8;
                let mut rgba = vec![0u8; w * h * 4];
                let mut luma = vec![0u8; w * h];
                for y in 0..h {
                    let row = std::slice::from_raw_parts(base.add(y * stride), w * 4);
                    for x in 0..w {
                        let (b, g, r) = (row[x * 4], row[x * 4 + 1], row[x * 4 + 2]);
                        let o = (y * w + x) * 4;
                        rgba[o..o + 4].copy_from_slice(&[r, g, b, 255]);
                        // Rec. 709 luma, integer weights summing to 256.
                        luma[y * w + x] = ((54 * r as u32 + 183 * g as u32 + 19 * b as u32) >> 8) as u8;
                    }
                }
                let (rgba, _, _) = orient(&rgba, w, h, 4, self.orientation);
                let (luma, lw, lh) = orient(&luma, w, h, 1, self.orientation);
                Frame { stamp: s, luma: GrayFrame::new(lw, lh, luma), rgba: Some(rgba) }
            } else {
                let (w, h, stride) = (CVPixelBufferGetWidthOfPlane(&buf, 0), CVPixelBufferGetHeightOfPlane(&buf, 0), CVPixelBufferGetBytesPerRowOfPlane(&buf, 0));
                let base = CVPixelBufferGetBaseAddressOfPlane(&buf, 0) as *const u8;
                let mut luma = vec![0u8; w * h];
                for y in 0..h {
                    luma[y * w..(y + 1) * w].copy_from_slice(std::slice::from_raw_parts(base.add(y * stride), w));
                }
                let (luma, lw, lh) = orient(&luma, w, h, 1, self.orientation);
                Frame { stamp: s, luma: GrayFrame::new(lw, lh, luma), rgba: None }
            };
            CVPixelBufferUnlockBaseAddress(&buf, CVPixelBufferLockFlags::ReadOnly);
            Ok(result)
        }
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        unsafe { self.reader.cancelReading() };
    }
}
