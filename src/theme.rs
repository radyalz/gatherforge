//! Studio / listening-room palette. Single source of color and spacing.
use eframe::egui::{self, Color32, Vec2};

pub const BASE: Color32 = Color32::from_rgb(244, 241, 235); // paper
pub const SIDEBAR: Color32 = Color32::from_rgb(27, 36, 48); // midnight ink
pub const SURFACE: Color32 = Color32::from_rgb(255, 254, 250); // porcelain
pub const SURFACE_RAISED: Color32 = Color32::from_rgb(236, 232, 224);
pub const STROKE: Color32 = Color32::from_rgb(214, 211, 203);
pub const WHITE: Color32 = Color32::from_rgb(34, 42, 52); // text on light surfaces (legacy alias)
pub const MUTED: Color32 = Color32::from_rgb(92, 99, 109);
pub const LIME: Color32 = Color32::from_rgb(54, 75, 189); // signature cobalt (legacy alias)
pub const LAVENDER: Color32 = Color32::from_rgb(55, 104, 115); // deep teal
pub const CORAL: Color32 = Color32::from_rgb(175, 78, 53);
pub const HERO: Color32 = Color32::from_rgb(28, 40, 54);
pub const INK: Color32 = Color32::from_rgb(253, 250, 243); // text on dark hero and primary actions
pub const ERROR: Color32 = Color32::from_rgb(171, 57, 54);
pub const SIGNAL: Color32 = Color32::from_rgb(238, 150, 93);
pub const SIDEBAR_WIDTH: f32 = 222.0;
pub const GUTTER: f32 = 28.0;
pub const RADIUS: u8 = 15;

pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = BASE;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = SURFACE_RAISED;
    visuals.override_text_color = Some(WHITE);
    visuals.selection.bg_fill = LIME.gamma_multiply(0.22);
    visuals.selection.stroke.color = LIME;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, STROKE);
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(9);
    visuals.widgets.hovered.bg_fill = SURFACE_RAISED;
    visuals.widgets.active.bg_fill = SURFACE_RAISED;
    visuals.hyperlink_color = LIME;
    style.visuals = visuals;
    style.spacing.item_spacing = Vec2::new(10.0, 9.0);
    style.spacing.button_padding = Vec2::new(16.0, 10.0);
    ctx.set_style(style);
}
