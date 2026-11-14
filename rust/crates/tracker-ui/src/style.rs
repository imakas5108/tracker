//! Palette and sizes. A light, neutral workspace like Java Tracker's, with saturated color
//! reserved for tracks, the active tool, and states that need attention.

use gpui_kit::{Hsla, Rgba, rgb};

pub fn window_bg() -> Rgba { rgb(0xe9ebee) }
pub fn panel_bg() -> Rgba { rgb(0xf7f8f9) }
pub fn surface() -> Rgba { rgb(0xffffff) }
pub fn video_bg() -> Rgba { rgb(0x1b1d21) }
pub fn border() -> Rgba { rgb(0xc9cdd3) }
pub fn hairline() -> Rgba { rgb(0xdde0e4) }
pub fn text() -> Rgba { rgb(0x1d2025) }
pub fn text_muted() -> Rgba { rgb(0x4d5562) }
pub fn accent() -> Rgba { rgb(0x1f5fbf) }
pub fn accent_soft() -> Rgba { rgb(0xdbe6f7) }
pub fn warn() -> Rgba { rgb(0x8a5200) }
pub fn warn_soft() -> Rgba { rgb(0xfaefd8) }
pub fn bad() -> Rgba { rgb(0xab2620) }
pub fn ok() -> Rgba { rgb(0x19703f) }
pub fn axes() -> Rgba { rgb(0xd61fd6) }
pub fn tape() -> Rgba { rgb(0x00a0ff) }
pub fn candidate() -> Rgba { rgb(0xffd400) }

pub fn track_color(c: [u8; 3]) -> Hsla {
    Rgba { r: c[0] as f32 / 255.0, g: c[1] as f32 / 255.0, b: c[2] as f32 / 255.0, a: 1.0 }.into()
}
