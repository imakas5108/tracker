//! Plot and table views of a track (the right-hand panes, as in Java Tracker).

use crate::style;
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use tracker_core::{KinematicRow, Origin};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Var {
    X,
    Y,
    Vx,
    Vy,
    Speed,
    Ax,
    Ay,
}

impl Var {
    pub const ALL: [Var; 7] = [Var::X, Var::Y, Var::Vx, Var::Vy, Var::Speed, Var::Ax, Var::Ay];
    pub fn name(&self) -> &'static str {
        match self {
            Var::X => "x",
            Var::Y => "y",
            Var::Vx => "vx",
            Var::Vy => "vy",
            Var::Speed => "v",
            Var::Ax => "ax",
            Var::Ay => "ay",
        }
    }
    pub fn unit(&self, u: &str) -> String {
        match self {
            Var::X | Var::Y => u.to_string(),
            Var::Vx | Var::Vy | Var::Speed => format!("{u}/s"),
            Var::Ax | Var::Ay => format!("{u}/s²"),
        }
    }
    pub fn get(&self, r: &KinematicRow) -> Option<f64> {
        match self {
            Var::X => Some(r.x),
            Var::Y => Some(r.y),
            Var::Vx => r.vx,
            Var::Vy => r.vy,
            Var::Speed => r.speed,
            Var::Ax => r.ax,
            Var::Ay => r.ay,
        }
    }
}

fn fmt(v: f64) -> String {
    let a = v.abs();
    if a != 0.0 && (a < 0.01 || a >= 10000.0) { format!("{v:.3e}") } else { format!("{v:.3}") }
}

/// A scatter-and-line plot of `var` against time. `now` marks the viewer time.
pub fn plot(rows: &[KinematicRow], var: Var, unit: &str, color: Hsla, now: f64) -> Div {
    let pts: Vec<(f64, f64, Origin)> = rows.iter().filter_map(|r| var.get(r).map(|v| (r.t, v, r.origin))).collect();
    if pts.is_empty() {
        return div().flex().flex_1().items_center().justify_center().p_4().child(
            div().text_size(px(12.5)).text_color(style::text_muted()).child(if rows.is_empty() { "No points yet. Mark the track in the video (Shift-click), or let an agent track it." } else { "Needs three neighbouring points for velocity, and more for acceleration." }),
        );
    }
    let (mut t0, mut t1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for (t, v, _) in &pts {
        t0 = t0.min(*t);
        t1 = t1.max(*t);
        v0 = v0.min(*v);
        v1 = v1.max(*v);
    }
    if (t1 - t0).abs() < 1e-9 {
        t1 = t0 + 1.0;
    }
    if (v1 - v0).abs() < 1e-12 {
        v0 -= 1.0;
        v1 += 1.0;
    }
    let pad = (v1 - v0) * 0.06;
    let (v0, v1) = (v0 - pad, v1 + pad);
    let data = pts.clone();
    let canvas = canvas(
        move |_, _, _| (),
        move |bounds, _, window, _| {
            let (x0, y0) = (bounds.origin.x, bounds.origin.y);
            let (w, h) = (bounds.size.width, bounds.size.height);
            let map = |t: f64, v: f64| point(x0 + w * ((t - t0) / (t1 - t0)) as f32, y0 + h * (1.0 - ((v - v0) / (v1 - v0)) as f32));
            window.paint_quad(fill(bounds, style::surface()));
            // Grid: quarters, plus the zero line when in range.
            for k in 1..4 {
                let f = k as f32 / 4.0;
                window.paint_quad(fill(Bounds::new(point(x0, y0 + h * f), size(w, px(1.))), style::hairline()));
                window.paint_quad(fill(Bounds::new(point(x0 + w * f, y0), size(px(1.), h)), style::hairline()));
            }
            if v0 < 0.0 && v1 > 0.0 {
                let z = map(t0, 0.0).y;
                window.paint_quad(fill(Bounds::new(point(x0, z), size(w, px(1.))), style::border()));
            }
            if now >= t0 && now <= t1 {
                let x = map(now, v0).x;
                window.paint_quad(fill(Bounds::new(point(x, y0), size(px(1.), h)), style::accent()));
            }
            let mut b = PathBuilder::stroke(px(1.5));
            for (i, (t, v, _)) in data.iter().enumerate() {
                let p = map(*t, *v);
                if i == 0 { b.move_to(p) } else { b.line_to(p) }
            }
            if let Ok(path) = b.build() {
                window.paint_path(path, color.opacity(0.55));
            }
            for (t, v, o) in &data {
                let p = map(*t, *v);
                let r = px(if *o == Origin::Automatic { 2.5 } else { 3.5 });
                let rect = Bounds::new(point(p.x - r, p.y - r), size(r * 2., r * 2.));
                // Manual and reviewed points are hollow squares; automatic ones are filled.
                if *o == Origin::Automatic {
                    window.paint_quad(fill(rect, color));
                } else {
                    window.paint_quad(outline(rect, color, BorderStyle::Solid));
                }
            }
        },
    )
    .size_full();
    let small = |s: String| div().text_size(px(10.5)).text_color(style::text_muted()).child(s);
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(120.))
        .p_2()
        .gap_1()
        .child(div().flex().flex_1().gap_1().child(div().w(px(64.)).flex().flex_col().justify_between().items_end().child(small(fmt(v1))).child(small(format!("{} ({})", var.name(), var.unit(unit)))).child(small(fmt(v0)))).child(div().flex_1().border_1().border_color(style::border()).child(canvas)))
        .child(div().flex().ml(px(68.)).justify_between().child(small(format!("{t0:.3} s"))).child(small("t (s)".into())).child(small(format!("{t1:.3} s"))))
}

pub const TABLE_COLUMNS: [&str; 7] = ["t (s)", "x", "y", "vx", "vy", "ax", "ay"];

pub fn table_row(r: &KinematicRow, selected: bool) -> Div {
    // Equal flex shares in rows and header keep columns aligned at any pane width.
    let cell = |s: String| div().flex_1().min_w(px(0.)).overflow_hidden().whitespace_nowrap().text_size(px(11.5)).text_color(style::text()).child(s);
    let f = |v: Option<f64>| v.map(fmt).unwrap_or_default();
    div()
        .w_full()
        .flex()
        .px_2()
        .h(px(20.))
        .items_center()
        .when(selected, |d| d.bg(style::accent_soft()))
        .child(cell(format!("{:.4}", r.t)))
        .child(cell(fmt(r.x)))
        .child(cell(fmt(r.y)))
        .child(cell(f(r.vx)))
        .child(cell(f(r.vy)))
        .child(cell(f(r.ax)))
        .child(cell(f(r.ay)))
        .child(div().w(px(18.)).flex_none().text_size(px(11.)).text_color(style::text_muted()).child(r.origin.code()))
}
