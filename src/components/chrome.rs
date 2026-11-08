use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, RichText, Stroke, Vec2};

pub fn brand(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(43.0), egui::Sense::hover());
        let c = r.center();
        ui.painter().circle_filled(c, 21.0, theme::SIGNAL);
        for rad in [14.5, 10.0, 5.5] {
            ui.painter()
                .circle_stroke(c, rad, Stroke::new(1.2, theme::SIDEBAR));
        }
        ui.painter().circle_filled(c, 2.3, theme::INK);
        ui.add_space(7.0);
        ui.vertical(|ui| {
            ui.label(
                RichText::new("GatherForge")
                    .size(20.0)
                    .strong()
                    .color(theme::INK),
            );
            ui.label(
                RichText::new("The collection studio")
                    .size(11.0)
                    .color(Color32::from_rgb(183, 190, 197)),
            );
        });
    });
}

fn icon(ui: &egui::Ui, center: Pos2, label: &str, tint: Color32) {
    let p = ui.painter();
    let stroke = Stroke::new(1.7, tint);
    match label {
        "Overview" => {
            p.circle_stroke(center, 8.2, stroke);
            p.circle_filled(center, 2.8, tint);
            p.line_segment(
                [
                    center + Vec2::new(0.0, -11.0),
                    center + Vec2::new(0.0, -6.5),
                ],
                stroke,
            );
        }
        "Downloads" => {
            p.line_segment(
                [center + Vec2::new(0.0, -9.0), center + Vec2::new(0.0, 5.0)],
                stroke,
            );
            p.line_segment(
                [center + Vec2::new(-5.0, 0.0), center + Vec2::new(0.0, 5.0)],
                stroke,
            );
            p.line_segment(
                [center + Vec2::new(5.0, 0.0), center + Vec2::new(0.0, 5.0)],
                stroke,
            );
            p.line_segment(
                [center + Vec2::new(-9.0, 9.0), center + Vec2::new(9.0, 9.0)],
                stroke,
            );
        }
        "Music library" => {
            for dy in [-6.0, 0.0, 6.0] {
                p.circle_filled(center + Vec2::new(-6.0, dy), 1.5, tint);
                p.line_segment(
                    [center + Vec2::new(-1.5, dy), center + Vec2::new(9.0, dy)],
                    stroke,
                );
            }
        }
        "Duplicate review" => {
            p.circle_stroke(center + Vec2::new(-3.0, -2.0), 7.0, stroke);
            p.circle_stroke(center + Vec2::new(4.0, 3.0), 7.0, stroke);
        }
        "Artist folders" => {
            p.circle_stroke(center + Vec2::new(0.0, -4.0), 4.5, stroke);
            p.line_segment(
                [center + Vec2::new(-9.0, 9.0), center + Vec2::new(-5.0, 5.0)],
                stroke,
            );
            p.line_segment(
                [center + Vec2::new(-5.0, 5.0), center + Vec2::new(5.0, 5.0)],
                stroke,
            );
            p.line_segment(
                [center + Vec2::new(5.0, 5.0), center + Vec2::new(9.0, 9.0)],
                stroke,
            );
        }
        _ => {
            p.circle_stroke(center, 8.0, stroke);
            p.circle_filled(center, 2.0, tint);
        }
    }
}

pub fn nav(
    ui: &mut egui::Ui,
    _glyph: &str,
    label: &str,
    active: bool,
    badge: Option<usize>,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), egui::Sense::click());
    let painter = ui.painter();
    let hovering = response.hovered();
    let fill = if active {
        Color32::from_rgb(49, 63, 80)
    } else if hovering {
        Color32::from_rgb(38, 49, 63)
    } else {
        theme::SIDEBAR
    };
    painter.rect_filled(rect, 9.0, fill);
    if active {
        painter.rect_filled(
            egui::Rect::from_min_size(rect.left_top() + Vec2::new(0.0, 9.0), Vec2::new(3.0, 26.0)),
            2.0,
            theme::SIGNAL,
        );
    }
    let color = if active {
        theme::INK
    } else {
        Color32::from_rgb(179, 188, 199)
    };
    icon(
        ui,
        rect.left_center() + Vec2::new(21.0, 0.0),
        label,
        if active { theme::SIGNAL } else { color },
    );
    painter.text(
        rect.left_center() + Vec2::new(44.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.5),
        color,
    );
    if let Some(number) = badge.filter(|n| *n > 0) {
        painter.text(
            rect.right_center() - Vec2::new(12.0, 0.0),
            Align2::RIGHT_CENTER,
            number.to_string(),
            FontId::proportional(11.0),
            theme::SIGNAL,
        );
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

pub fn path_strip(ui: &mut egui::Ui, path: &str, phase: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Working folder")
                .size(12.0)
                .color(theme::MUTED),
        );
        ui.add(egui::Label::new(RichText::new(path).size(12.0).strong()).truncate());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(phase).size(11.0).color(theme::LAVENDER));
        });
    });
    ui.add_space(3.0);
    ui.separator();
}
