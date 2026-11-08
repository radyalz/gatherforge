use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, RichText, Stroke, Vec2};

pub fn surface() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::SURFACE)
        .stroke(Stroke::new(1.0, theme::STROKE))
        .corner_radius(egui::CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin::same(20))
}

pub fn metric(
    ui: &mut egui::Ui,
    title: &str,
    value: usize,
    detail: &str,
    tint: Color32,
    width: f32,
) {
    ui.vertical(|ui| {
        ui.set_width((width - 14.0).max(75.0));
        ui.label(
            RichText::new(value.to_string())
                .size(35.0)
                .strong()
                .color(tint),
        );
        ui.label(RichText::new(title).size(12.0).strong().color(theme::WHITE));
        ui.label(RichText::new(detail).size(11.0).color(theme::MUTED));
    });
}

pub fn hero(ui: &mut egui::Ui, tracks: usize, artists: usize) {
    egui::Frame::new()
        .fill(theme::HERO)
        .corner_radius(egui::CornerRadius::same(19))
        .inner_margin(egui::Margin::same(28))
        .show(ui, |ui| {
            let w = ui.available_width().max(430.0);
            ui.set_min_height(229.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width((w * 0.56).max(260.0));
                    ui.add_space(17.0);
                    ui.label(
                        RichText::new("Gathered here, ready for what comes next.")
                            .size(12.0)
                            .color(Color32::from_rgb(197, 206, 216)),
                    );
                    ui.add_space(13.0);
                    ui.label(
                        RichText::new("A better home\nfor your sound.")
                            .size(35.0)
                            .strong()
                            .color(theme::INK),
                    );
                    ui.add_space(17.0);
                    ui.label(
                        RichText::new(format!("{tracks} tracks in {artists} artist collections"))
                            .size(13.0)
                            .color(Color32::from_rgb(222, 225, 228)),
                    );
                });
                let remaining = ui.available_width();
                if remaining >= 150.0 {
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(remaining.min(290.0), 216.0),
                        egui::Sense::hover(),
                    );
                    let center = rect.center() + Vec2::new(17.0, 0.0);
                    let painter = ui.painter();
                    // The single visual signature: a quiet, tactile vinyl record.
                    painter.circle_filled(center, 105.0, Color32::from_rgb(12, 17, 24));
                    for radius in [98.0, 92.0, 86.0, 80.0, 73.0, 67.0, 60.0, 53.0] {
                        painter.circle_stroke(
                            center,
                            radius,
                            Stroke::new(1.0, Color32::from_rgb(42, 54, 65)),
                        );
                    }
                    painter.circle_filled(center, 45.0, theme::SIGNAL);
                    painter.circle_stroke(
                        center,
                        31.0,
                        Stroke::new(1.0, Color32::from_rgb(161, 83, 59)),
                    );
                    painter.circle_filled(center, 4.0, theme::HERO);
                    painter.text(
                        center + Vec2::new(0.0, -14.0),
                        Align2::CENTER_CENTER,
                        "GF",
                        FontId::proportional(15.0),
                        theme::HERO,
                    );
                    painter.text(
                        center + Vec2::new(0.0, 16.0),
                        Align2::CENTER_CENTER,
                        "STUDIO",
                        FontId::proportional(9.0),
                        theme::HERO,
                    );
                }
            });
        });
}

pub fn process_step(ui: &mut egui::Ui, n: &str, label: &str, caption: &str, _tint: Color32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
        ui.painter()
            .circle_stroke(rect.center(), 15.0, Stroke::new(1.0, theme::STROKE));
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            n,
            FontId::proportional(12.0),
            theme::MUTED,
        );
        ui.vertical(|ui| {
            ui.label(RichText::new(label).size(13.0).strong().color(theme::WHITE));
            ui.label(RichText::new(caption).size(11.0).color(theme::MUTED));
        });
    });
}
