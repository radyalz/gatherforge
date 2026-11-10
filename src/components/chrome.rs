use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, RichText, Stroke, Vec2};

fn line(ui: &egui::Ui, a: Pos2, b: Pos2, color: Color32) {
    ui.painter()
        .line_segment([a, b], Stroke::new(1.7_f32, color));
}
fn outline(ui: &egui::Ui, c: Pos2, x: f32, y: f32, w: f32, h: f32, color: Color32) {
    let tl = c + Vec2::new(x, y);
    let tr = tl + Vec2::new(w, 0.0);
    let br = tr + Vec2::new(0.0, h);
    let bl = tl + Vec2::new(0.0, h);
    for (a, b) in [(tl, tr), (tr, br), (br, bl), (bl, tl)] {
        line(ui, a, b, color)
    }
}

pub fn brand(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(38.0), egui::Sense::hover());
        let c = r.center();
        let p = ui.painter();
        p.circle_filled(c, 18.0, Color32::from_rgba_unmultiplied(251, 193, 154, 48));
        p.circle_stroke(
            c,
            16.8,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 224, 206, 155)),
        );
        p.circle_stroke(c, 11.5, Stroke::new(2.0_f32, theme::SIGNAL));
        p.circle_stroke(c, 6.3, Stroke::new(1.1_f32, theme::INK));
        p.circle_filled(c, 2.1, theme::SIGNAL);
        ui.add_space(7.0);
        ui.vertical(|ui| {
            ui.label(
                RichText::new("GatherForge")
                    .size(17.0)
                    .strong()
                    .color(theme::INK),
            );
            ui.label(
                RichText::new("MEDIA WORKSPACE")
                    .size(9.5)
                    .color(Color32::from_rgb(168, 190, 207)),
            );
        });
    });
}

fn icon(ui: &egui::Ui, c: Pos2, label: &str, tint: Color32) {
    let p = ui.painter();
    let s = Stroke::new(1.7_f32, tint);
    match label {
        "Overview" => {
            for (dx, dy) in [(-8.0, -8.0), (2.0, -8.0), (-8.0, 2.0), (2.0, 2.0)] {
                outline(ui, c, dx, dy, 6.5, 6.5, tint);
            }
        }
        "All activity" => {
            outline(ui, c, -8.0, -8.0, 16.0, 16.0, tint);
            for y in [-4.0, 1.0, 6.0] {
                line(ui, c + Vec2::new(-4.0, y), c + Vec2::new(4.0, y), tint);
            }
        }
        "Music downloads" => {
            line(ui, c + Vec2::new(0.0, -9.0), c + Vec2::new(0.0, 4.0), tint);
            line(ui, c + Vec2::new(-5.0, -1.0), c + Vec2::new(0.0, 4.0), tint);
            line(ui, c + Vec2::new(5.0, -1.0), c + Vec2::new(0.0, 4.0), tint);
            line(ui, c + Vec2::new(-9.0, 8.0), c + Vec2::new(9.0, 8.0), tint);
        }
        "Music library" => {
            line(ui, c + Vec2::new(1.0, -9.0), c + Vec2::new(1.0, 6.0), tint);
            line(ui, c + Vec2::new(1.0, -9.0), c + Vec2::new(9.0, -7.0), tint);
            line(ui, c + Vec2::new(9.0, -7.0), c + Vec2::new(9.0, 3.0), tint);
            p.circle_stroke(c + Vec2::new(-4.0, 6.0), 4.0, s);
            p.circle_stroke(c + Vec2::new(5.0, 3.0), 4.0, s);
        }
        "Duplicate review" => {
            outline(ui, c, -10.0, -8.0, 12.0, 13.0, tint);
            outline(ui, c, -3.0, -3.0, 12.0, 13.0, tint);
        }
        "Artist folders" => {
            p.circle_stroke(c + Vec2::new(0.0, -5.5), 4.5, s);
            line(ui, c + Vec2::new(-9.0, 8.5), c + Vec2::new(-5.0, 2.0), tint);
            line(ui, c + Vec2::new(-5.0, 2.0), c + Vec2::new(5.0, 2.0), tint);
            line(ui, c + Vec2::new(5.0, 2.0), c + Vec2::new(9.0, 8.5), tint);
        }
        "Reel downloads" => {
            outline(ui, c, -9.0, -9.0, 18.0, 18.0, tint);
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + Vec2::new(-2.0, -5.0),
                    c + Vec2::new(6.0, 0.0),
                    c + Vec2::new(-2.0, 5.0),
                ],
                tint,
                Stroke::NONE,
            ));
            for x in [-5.0, 0.0, 5.0] {
                p.circle_filled(c + Vec2::new(x, -10.0), 0.9, tint);
            }
        }
        "Reel feed" => {
            outline(ui, c, -8.0, -9.0, 16.0, 18.0, tint);
            for y in [-4.0, 0.0, 4.0] {
                p.circle_filled(c + Vec2::new(-4.0, y), 1.0, tint);
                line(ui, c + Vec2::new(-1.0, y), c + Vec2::new(5.0, y), tint);
            }
        }
        "Settings" => {
            for (dy, dx) in [(-7.0, -3.0), (0.0, 5.0), (7.0, -1.0)] {
                line(ui, c + Vec2::new(-9.0, dy), c + Vec2::new(9.0, dy), tint);
                p.circle_filled(c + Vec2::new(dx, dy), 2.9, theme::SIDEBAR);
                p.circle_stroke(c + Vec2::new(dx, dy), 2.9, s);
            }
        }
        _ => {
            p.circle_stroke(c, 7.0, s);
            p.circle_filled(c, 2.0, tint);
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
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), egui::Sense::click());
    let p = ui.painter();
    let hovered = response.hovered();
    if active || hovered {
        let fill = if active {
            Color32::from_rgba_unmultiplied(130, 175, 200, 53)
        } else {
            Color32::from_rgba_unmultiplied(220, 240, 255, 21)
        };
        p.rect_filled(rect, theme::RADIUS as f32, fill);
        p.line_segment(
            [
                rect.left_top() + Vec2::new(7.0, 0.0),
                rect.right_top() + Vec2::new(-7.0, 0.0),
            ],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(226, 238, 247, 48)),
        );
    }
    if active {
        p.rect_filled(
            egui::Rect::from_min_size(rect.left_top() + Vec2::new(0.0, 8.0), Vec2::new(3.0, 24.0)),
            2.0,
            theme::SIGNAL,
        );
    }
    let fg = if active {
        theme::INK
    } else {
        Color32::from_rgb(182, 200, 215)
    };
    icon(
        ui,
        rect.left_center() + Vec2::new(21.0, 0.0),
        label,
        if active { theme::SIGNAL } else { fg },
    );
    p.text(
        rect.left_center() + Vec2::new(43.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        fg,
    );
    if let Some(n) = badge.filter(|n| *n > 0) {
        p.text(
            rect.right_center() - Vec2::new(11.0, 0.0),
            Align2::RIGHT_CENTER,
            n.to_string(),
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
        ui.label(RichText::new("Workspace").size(12.0).color(theme::muted()));
        ui.add(egui::Label::new(RichText::new(path).size(12.0).strong()).truncate());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(phase).size(11.0).color(theme::lavender()));
        });
    });
    ui.add_space(3.0);
    ui.separator();
}
