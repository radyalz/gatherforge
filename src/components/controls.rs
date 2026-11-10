use crate::theme;
use eframe::egui::{self, Color32, RichText, Stroke};

pub fn button(label: &str, primary: bool) -> egui::Button<'_> {
    let (fill, fg, border) = if primary {
        (theme::lime(), theme::primary_text(), Stroke::new(1.0_f32, theme::GOLD))
    } else {
        (
            theme::surface(),
            theme::white(),
            Stroke::new(1.0_f32, theme::glass_edge()),
        )
    };
    egui::Button::new(RichText::new(label).size(13.0).strong().color(fg))
        .fill(fill)
        .stroke(border)
        .corner_radius(egui::CornerRadius::same(theme::RADIUS))
        .min_size(egui::vec2(0.0, theme::CONTROL_HEIGHT))
}

pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.10))
        .corner_radius(egui::CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).strong().color(color));
        });
}

pub fn section_heading(ui: &mut egui::Ui, _eyebrow: &str, title: &str, subtitle: &str) {
    ui.label(
        RichText::new(title)
            .size(19.0)
            .strong()
            .color(theme::white()),
    );
    ui.add_space(3.0);
    ui.label(RichText::new(subtitle).size(12.5).color(theme::muted()));
}
