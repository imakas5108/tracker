//! PNG previews of frames for agents: crop, downscale, optional overlays, with an exact
//! mapping back to source pixels.

use crate::{AppError, Result, fail};
use serde_json::{Value, json};
use tracker_core::{AnalysisState, FrameStamp, Point};

pub struct Crop {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

fn put(img: &mut [u8], w: usize, h: usize, x: i64, y: i64, c: [u8; 3]) {
    if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
        let i = (y as usize * w + x as usize) * 4;
        img[i..i + 3].copy_from_slice(&c);
    }
}

fn line(img: &mut [u8], w: usize, h: usize, a: (f64, f64), b: (f64, f64), c: [u8; 3], thick: i64) {
    let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as usize).max(1);
    for k in 0..=steps {
        let t = k as f64 / steps as f64;
        let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        for dx in -thick..=thick {
            for dy in -thick..=thick {
                put(img, w, h, x.round() as i64 + dx, y.round() as i64 + dy, c);
            }
        }
    }
}

/// Renders `rgba` (source size `sw`×`sh`) to PNG. Returns the PNG bytes and metadata.
pub fn render_png(rgba: &[u8], sw: usize, sh: usize, stamp: FrameStamp, crop: Option<Crop>, max_edge: usize, overlays: Option<&AnalysisState>) -> Result<(Vec<u8>, Value)> {
    if !(16..=1600).contains(&max_edge) {
        return fail("invalid_argument", "max_edge must be 16 to 1600.");
    }
    let c = crop.unwrap_or(Crop { x: 0, y: 0, width: sw, height: sh });
    if c.width == 0 || c.height == 0 || c.x >= sw || c.y >= sh || c.width > sw - c.x || c.height > sh - c.y {
        return fail("invalid_crop", "The crop must lie inside the source image.");
    }
    let scale = (max_edge as f64 / c.width.max(c.height) as f64).min(1.0);
    let ow = ((c.width as f64 * scale).round() as usize).max(1);
    let oh = ((c.height as f64 * scale).round() as usize).max(1);
    let (sx, sy) = (ow as f64 / c.width as f64, oh as f64 / c.height as f64);
    // Box filter over the source area each output pixel covers.
    let mut out = vec![0u8; ow * oh * 4];
    for oy in 0..oh {
        let y0 = c.y + (oy as f64 / sy) as usize;
        let y1 = (c.y + ((oy + 1) as f64 / sy).ceil() as usize).min(c.y + c.height).max(y0 + 1);
        for ox in 0..ow {
            let x0 = c.x + (ox as f64 / sx) as usize;
            let x1 = (c.x + ((ox + 1) as f64 / sx).ceil() as usize).min(c.x + c.width).max(x0 + 1);
            let mut acc = [0u32; 3];
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * sw + x) * 4;
                    acc[0] += rgba[i] as u32;
                    acc[1] += rgba[i + 1] as u32;
                    acc[2] += rgba[i + 2] as u32;
                }
            }
            let o = (oy * ow + ox) * 4;
            out[o..o + 4].copy_from_slice(&[(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8, 255]);
        }
    }
    let to_img = |p: Point| ((p.x - c.x as f64) * sx, (p.y - c.y as f64) * sy);
    if let Some(a) = overlays {
        // Axes: origin cross and the +x direction.
        let o = to_img(a.coords.origin);
        let len = 40.0;
        let (sn, cs) = a.coords.angle.sin_cos();
        line(&mut out, ow, oh, o, (o.0 + cs * len, o.1 - sn * len), [255, 0, 255], 0);
        line(&mut out, ow, oh, o, (o.0 - sn * len, o.1 - cs * len), [255, 0, 255], 0);
        if let Some(cal) = a.coords.calibration {
            line(&mut out, ow, oh, to_img(cal.first), to_img(cal.second), [0, 160, 255], 0);
        }
        for t in &a.tracks {
            if let Some(m) = t.point_at(stamp) {
                let (x, y) = to_img(m.pixel);
                line(&mut out, ow, oh, (x - 6.0, y), (x + 6.0, y), [0, 0, 0], 1);
                line(&mut out, ow, oh, (x, y - 6.0), (x, y + 6.0), [0, 0, 0], 1);
                line(&mut out, ow, oh, (x - 6.0, y), (x + 6.0, y), t.color, 0);
                line(&mut out, ow, oh, (x, y - 6.0), (x, y + 6.0), t.color, 0);
            }
        }
    }
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, ow as u32, oh as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| AppError::new("encode_failed", e.to_string()))?;
        w.write_image_data(&out).map_err(|e| AppError::new("encode_failed", e.to_string()))?;
    }
    let meta = json!({
        "time": stamp.seconds(),
        "frame": {"value": stamp.value, "timescale": stamp.timescale},
        "source_width": sw, "source_height": sh,
        "image_width": ow, "image_height": oh,
        "source_origin": "top-left, y down",
        "mapping": {"crop_x": c.x, "crop_y": c.y, "crop_width": c.width, "crop_height": c.height, "scale_x": sx, "scale_y": sy,
                    "to_source": "source_x = image_x / scale_x + crop_x; source_y = image_y / scale_y + crop_y"},
        "overlays": overlays.is_some(),
    });
    Ok((png_bytes, meta))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_and_scale_mapping() {
        let (w, h) = (200, 100);
        let mut rgba = vec![0u8; w * h * 4];
        for y in 40..44 {
            for x in 120..124 {
                rgba[(y * w + x) * 4] = 255;
            }
        }
        let (png, meta) = render_png(&rgba, w, h, FrameStamp::new(1, 30), Some(Crop { x: 100, y: 20, width: 80, height: 40 }), 40, None).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        assert_eq!(meta["image_width"], 40);
        let sx = meta["mapping"]["scale_x"].as_f64().unwrap();
        // The red square at source (120..124, 40..44) maps to image x = (120 - 100) * 0.5 = 10.
        assert!((sx - 0.5).abs() < 1e-12);
        assert!(render_png(&rgba, w, h, FrameStamp::new(1, 30), Some(Crop { x: 150, y: 0, width: 80, height: 10 }), 100, None).is_err());
    }
}
