use crate::theme;
use eframe::egui::{self, Color32, RichText, Stroke};

pub fn button(label: &str, primary: bool) -> egui::Button<'_> {
    let (fill, fg, border) = if primary {
        (theme::LIME, theme::INK, Stroke::NONE)
    } else {
        (
            theme::SURFACE,
            theme::WHITE,
            Stroke::new(1.0, theme::STROKE),
        )
    };
    egui::Button::new(RichText::new(label).size(13.0).strong().color(fg))
        .fill(fill)
        .stroke(border)
        .corner_radius(egui::CornerRadius::same(9))
}

pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.10))
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(egui::Margin::symmetric(9, 5))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).strong().color(color));
        });
}

pub fn section_heading(ui: &mut egui::Ui, _eyebrow: &str, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(24.0).strong().color(theme::WHITE));
    ui.add_space(3.0);
    ui.label(RichText::new(subtitle).size(12.5).color(theme::MUTED));
}
