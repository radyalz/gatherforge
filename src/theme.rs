//! GatherForge Forge Edition palette: dense, angular, low-gloss, crimson-first.
//! Only music artwork/card surfaces retain the mandated 8px radius exception.
use eframe::egui::{self, Color32, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};
static DARK: AtomicBool = AtomicBool::new(true);
pub const SIDEBAR: Color32 = Color32::from_rgb(18, 9, 12);
pub const INK: Color32 = Color32::from_rgb(250, 239, 229);
pub const SIGNAL: Color32 = Color32::from_rgb(238, 53, 55);
pub const GOLD: Color32 = Color32::from_rgb(203, 150, 82);
pub const SIDEBAR_WIDTH: f32 = 226.0;
pub const GUTTER: f32 = 18.0;
pub const RADIUS: u8 = 2;
pub const CONTROL_HEIGHT: f32 = 36.0;
pub const ROW_HEIGHT: f32 = 58.0;
fn dark() -> bool { DARK.load(Ordering::Relaxed) }
pub fn base() -> Color32 {
    if dark() { Color32::from_rgb(21, 9, 12) } else { Color32::from_rgb(246, 241, 237) }
}
pub fn surface() -> Color32 {
    if dark() { Color32::from_rgb(30, 16, 19) } else { Color32::from_rgb(253, 249, 246) }
}
pub fn surface_raised() -> Color32 {
    if dark() { Color32::from_rgb(39, 18, 21) } else { Color32::from_rgb(255, 253, 251) }
}
pub fn stroke() -> Color32 {
    if dark() { Color32::from_rgb(104, 56, 45) } else { Color32::from_rgb(176, 150, 135) }
}
pub fn glass_edge() -> Color32 {
    if dark() { Color32::from_rgb(151, 95, 58) } else { Color32::from_rgb(196, 160, 113) }
}
pub fn glass_hover() -> Color32 {
    if dark() { Color32::from_rgb(69, 25, 30) } else { Color32::from_rgb(245, 220, 213) }
}
pub fn white() -> Color32 {
    if dark() { Color32::from_rgb(249, 238, 232) } else { Color32::from_rgb(40, 22, 24) }
}
pub fn muted() -> Color32 {
    if dark() { Color32::from_rgb(182, 153, 149) } else { Color32::from_rgb(105, 78, 77) }
}
pub fn lime() -> Color32 {
    if dark() { Color32::from_rgb(241, 58, 61) } else { Color32::from_rgb(178, 28, 37) }
}
pub fn primary_text() -> Color32 { Color32::WHITE }
pub fn lavender() -> Color32 {
    if dark() { GOLD } else { Color32::from_rgb(135, 84, 41) }
}
pub fn coral() -> Color32 {
    if dark() { Color32::from_rgb(255, 111, 87) } else { Color32::from_rgb(176, 51, 41) }
}
pub fn hero() -> Color32 {
    if dark() { Color32::from_rgb(59, 18, 24) } else { Color32::from_rgb(236, 216, 206) }
}
pub fn error() -> Color32 {
    if dark() { Color32::from_rgb(255, 111, 101) } else { Color32::from_rgb(168, 38, 39) }
}
pub fn install(ctx: &egui::Context, dark_mode: bool) {
    DARK.store(dark_mode, Ordering::Relaxed);
    let mut style = (*ctx.style()).clone();
    let mut visuals = if dark_mode { egui::Visuals::dark() } else { egui::Visuals::light() };
    visuals.panel_fill = base();
    visuals.window_fill = surface_raised();
    visuals.extreme_bg_color = surface_raised();
    visuals.override_text_color = Some(white());
    visuals.selection.bg_fill = lime().gamma_multiply(0.42);
    visuals.selection.stroke.color = lime();
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, stroke());
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.widgets.inactive.bg_fill = surface();
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, stroke());
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.widgets.hovered.bg_fill = glass_hover();
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, GOLD);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.widgets.active.bg_fill = lime().gamma_multiply(0.72);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, GOLD);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.window_corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.menu_corner_radius = egui::CornerRadius::same(RADIUS);
    visuals.hyperlink_color = lavender();
    style.visuals = visuals;
    style.spacing.item_spacing = Vec2::new(9.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.spacing.interact_size = Vec2::new(40.0, CONTROL_HEIGHT);
    ctx.set_style(style);
}
