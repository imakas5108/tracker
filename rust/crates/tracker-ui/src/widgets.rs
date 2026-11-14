//! Small shared controls. Text labels throughout: the toolbar names each operation in words.

use crate::style;
use gpui_kit::*;

/// A text button. Disabled buttons show a not-allowed cursor and muted text.
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, enabled: bool, active: bool, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Stateful<Div> {
    let base = div()
        .id(id)
        .px_2()
        .py(px(3.))
        .rounded(px(4.))
        .border_1()
        .text_size(px(12.5))
        .whitespace_nowrap()
        .child(label.into());
    let base = if active {
        base.bg(style::accent_soft()).border_color(style::accent()).text_color(style::accent())
    } else {
        base.bg(style::surface()).border_color(style::border()).text_color(if enabled { style::text() } else { rgb(0x9aa1ab) })
    };
    if enabled {
        base.cursor_pointer().hover(|s| s.bg(rgb(0xeef1f5))).active(|s| s.bg(rgb(0xdfe4ea))).on_click(on_click)
    } else {
        base.cursor(CursorStyle::OperationNotAllowed)
    }
}

pub fn separator() -> Div {
    div().w(px(1.)).h(px(18.)).bg(style::border()).mx_1()
}

pub fn label(text: impl Into<SharedString>) -> Div {
    div().text_size(px(12.)).text_color(style::text_muted()).whitespace_nowrap().child(text.into())
}

/// A one-line editable text value for dialogs (numbers and short names).
#[derive(Clone, Debug, Default)]
pub struct Field {
    pub label: &'static str,
    pub text: String,
}

impl Field {
    pub fn new(label: &'static str, text: impl Into<String>) -> Self {
        Self { label, text: text.into() }
    }
    pub fn number(&self) -> Option<f64> {
        self.text.trim().parse::<f64>().ok().filter(|v| v.is_finite())
    }
    /// Applies a key press. Returns true when the text changed.
    pub fn key(&mut self, ks: &Keystroke) -> bool {
        if ks.modifiers.platform || ks.modifiers.control {
            return false;
        }
        match ks.key.as_str() {
            "backspace" => self.text.pop().is_some(),
            "space" => {
                self.text.push(' ');
                true
            }
            k if k.chars().count() == 1 => {
                let c = ks.key_char.as_deref().unwrap_or(k);
                if self.text.len() < 80 {
                    self.text.push_str(c);
                }
                true
            }
            _ => false,
        }
    }
    pub fn render(&self, focused: bool) -> Div {
        div().flex().items_center().gap_2().child(div().w(px(120.)).text_size(px(12.5)).text_color(style::text()).child(self.label)).child(
            div()
                .min_w(px(180.))
                .px_2()
                .py(px(4.))
                .rounded(px(4.))
                .border_1()
                .border_color(if focused { style::accent() } else { style::border() })
                .bg(style::surface())
                .text_size(px(13.))
                .text_color(style::text())
                .child(if focused { format!("{}▏", self.text) } else if self.text.is_empty() { " ".to_string() } else { self.text.clone() }),
        )
    }
}
