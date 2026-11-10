use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, RichText, Stroke, Vec2};

pub fn surface() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::surface())
        .stroke(Stroke::new(1.0_f32, theme::glass_edge()))
        .corner_radius(egui::CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin::same(16))
}

pub fn metric(
    ui: &mut egui::Ui,
    title: &str,
    value: usize,
    detail: &str,
    tint: Color32,
    _width: f32,
) {
    ui.vertical(|ui| {
        ui.set_min_height(83.0);
        ui.label(
            RichText::new(title)
                .size(11.0)
                .strong()
                .color(theme::muted()),
        );
        ui.add_space(3.0);
        ui.label(
            RichText::new(value.to_string())
                .size(29.0)
                .strong()
                .color(tint),
        );
        ui.label(RichText::new(detail).size(11.0).color(theme::muted()));
    });
}

pub fn hero(ui: &mut egui::Ui, tracks: usize, artists: usize) {
    egui::Frame::new()
        .fill(theme::hero())
        .stroke(Stroke::new(1.0_f32, theme::glass_edge()))
        .corner_radius(egui::CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin::same(22))
        .show(ui, |ui| {
            let available = ui.available_width();
            let show_record = available >= 565.0;
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    if show_record {
                        ui.set_width((available - 190.0).max(260.0));
                    }
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("YOUR COLLECTION / AT A GLANCE")
                            .size(11.0)
                            .strong()
                            .color(Color32::from_rgb(192, 208, 221)),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("A better home\nfor your sound.")
                            .size(28.0)
                            .strong()
                            .color(theme::INK),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!("{tracks} tracks  ·  {artists} artist folders"))
                            .size(12.0)
                            .color(Color32::from_rgb(208, 216, 224)),
                    );
                    ui.add_space(6.0);
                });
                if show_record {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(164.0, 148.0), egui::Sense::hover());
                    let center = rect.center();
                    let painter = ui.painter();
                    painter.circle_filled(center, 69.0, Color32::from_rgb(12, 17, 24));
                    for radius in [63.0, 57.0, 51.0, 45.0] {
                        painter.circle_stroke(
                            center,
                            radius,
                            Stroke::new(1.0_f32, Color32::from_rgb(42, 54, 65)),
                        );
                    }
                    painter.circle_filled(center, 31.0, theme::SIGNAL);
                    painter.circle_stroke(
                        center,
                        22.0,
                        Stroke::new(1.0_f32, Color32::from_rgb(161, 83, 59)),
                    );
                    painter.circle_filled(center, 3.0, theme::hero());
                    painter.text(
                        center + Vec2::new(0.0, -11.0),
                        Align2::CENTER_CENTER,
                        "GF",
                        FontId::proportional(13.0),
                        theme::hero(),
                    );
                    painter.text(
                        center + Vec2::new(0.0, 12.0),
                        Align2::CENTER_CENTER,
                        "STUDIO",
                        FontId::proportional(8.0),
                        theme::hero(),
                    );
                }
            });
        });
}

pub fn process_step(ui: &mut egui::Ui, n: &str, label: &str, caption: &str, _tint: Color32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
        ui.painter()
            .circle_stroke(rect.center(), 15.0, Stroke::new(1.0_f32, theme::stroke()));
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            n,
            FontId::proportional(12.0),
            theme::muted(),
        );
        ui.vertical(|ui| {
            ui.label(
                RichText::new(label)
                    .size(13.0)
                    .strong()
                    .color(theme::white()),
            );
            ui.label(RichText::new(caption).size(11.0).color(theme::muted()));
        });
    });
}
