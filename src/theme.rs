//! Compact glass design system. Real blur is platform-dependent; panels use alpha and light edges.
use eframe::egui::{self, Color32, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};
static DARK: AtomicBool = AtomicBool::new(false);
pub const SIDEBAR: Color32 = Color32::from_rgb(16, 27, 43);
pub const INK: Color32 = Color32::from_rgb(245, 249, 253);
pub const SIGNAL: Color32 = Color32::from_rgb(141, 212, 220);
pub const SIDEBAR_WIDTH: f32 = 226.0;
pub const GUTTER: f32 = 24.0;
pub const RADIUS: u8 = 2;
pub const CONTROL_HEIGHT: f32 = 36.0;
pub const ROW_HEIGHT: f32 = 58.0;
fn dark() -> bool {
    DARK.load(Ordering::Relaxed)
}
pub fn base() -> Color32 {
    if dark() {
        Color32::from_rgb(13, 22, 35)
    } else {
        Color32::from_rgb(237, 243, 248)
    }
}
pub fn surface() -> Color32 {
    if dark() {
        Color32::from_rgba_unmultiplied(38, 59, 77, 211)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 226)
    }
}
pub fn surface_raised() -> Color32 {
    if dark() {
        Color32::from_rgb(31, 45, 62)
    } else {
        Color32::from_rgb(255, 255, 255)
    }
}
pub fn stroke() -> Color32 {
    if dark() {
        Color32::from_rgba_unmultiplied(176, 202, 223, 72)
    } else {
        Color32::from_rgba_unmultiplied(101, 127, 147, 68)
    }
}
pub fn glass_edge() -> Color32 {
    if dark() {
        Color32::from_rgba_unmultiplied(181, 215, 239, 80)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 250)
    }
}
pub fn glass_hover() -> Color32 {
    if dark() {
        Color32::from_rgba_unmultiplied(104, 134, 157, 120)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 241)
    }
}
pub fn white() -> Color32 {
    if dark() {
        Color32::from_rgb(241, 247, 252)
    } else {
        Color32::from_rgb(30, 47, 65)
    }
}
pub fn muted() -> Color32 {
    if dark() {
        Color32::from_rgb(172, 191, 206)
    } else {
        Color32::from_rgb(90, 109, 126)
    }
}
pub fn lime() -> Color32 {
    if dark() {
        Color32::from_rgb(97, 193, 222)
    } else {
        Color32::from_rgb(45, 117, 166)
    }
}
pub fn primary_text() -> Color32 {
    if dark() { Color32::from_rgb(10, 31, 45) } else { Color32::WHITE }
}
pub fn lavender() -> Color32 {
    if dark() {
        Color32::from_rgb(145, 214, 197)
    } else {
        Color32::from_rgb(46, 119, 116)
    }
}
pub fn coral() -> Color32 {
    if dark() {
        Color32::from_rgb(255, 171, 147)
    } else {
        Color32::from_rgb(178, 88, 65)
    }
}
pub fn hero() -> Color32 {
    if dark() {
        Color32::from_rgba_unmultiplied(35, 57, 80, 239)
    } else {
        Color32::from_rgba_unmultiplied(43, 75, 105, 240)
    }
}
pub fn error() -> Color32 {
    if dark() {
        Color32::from_rgb(255, 151, 146)
    } else {
        Color32::from_rgb(170, 62, 61)
    }
}
pub fn install(ctx: &egui::Context, dark_mode: bool) {
    DARK.store(dark_mode, Ordering::Relaxed);
    let mut style = (*ctx.style()).clone();
    let mut visuals = if dark_mode {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = base();
    visuals.window_fill = surface_raised();
    visuals.extreme_bg_color = surface_raised();
    visuals.override_text_color = Some(white());
    visuals.selection.bg_fill = lime().gamma_multiply(0.27);
    visuals.selection.stroke.color = lime();
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, stroke());
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.widgets.inactive.bg_fill = surface();
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, stroke());
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.widgets.hovered.bg_fill = glass_hover();
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.widgets.active.bg_fill = glass_hover();
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.window_corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.menu_corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.hyperlink_color = lime();
    style.visuals = visuals;
    style.spacing.item_spacing = Vec2::new(10.0, 9.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.spacing.interact_size = Vec2::new(40.0, CONTROL_HEIGHT);
    ctx.set_style(style);
}
