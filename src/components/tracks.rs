use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, RichText, Stroke, Vec2};

fn swatch(artist: &str) -> Color32 {
    let hash = artist
        .bytes()
        .fold(17u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    match hash % 5 {
        0 => Color32::from_rgb(189, 111, 83),
        1 => Color32::from_rgb(102, 136, 125),
        2 => Color32::from_rgb(133, 117, 170),
        3 => Color32::from_rgb(155, 126, 81),
        _ => Color32::from_rgb(99, 128, 175),
    }
}

pub fn track_row(
    ui: &mut egui::Ui,
    artist: &str,
    title: &str,
    format: &str,
    status: &str,
    selected: bool,
    duplicate: bool,
) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 63.0), egui::Sense::click());
    let p = ui.painter();
    if selected || response.hovered() {
        p.rect_filled(
            rect,
            8.0,
            if selected {
                Color32::from_rgb(229, 232, 242)
            } else {
                theme::SURFACE
            },
        );
    }
    let art = egui::Rect::from_min_size(rect.left_top() + Vec2::new(11.0, 8.0), Vec2::splat(46.0));
    let center = art.center();
    p.rect_filled(art, 8.0, swatch(artist));
    p.circle_stroke(
        center,
        14.0,
        Stroke::new(1.0, Color32::from_white_alpha(130)),
    );
    p.circle_stroke(
        center,
        9.0,
        Stroke::new(1.0, Color32::from_white_alpha(110)),
    );
    p.circle_filled(center, 3.0, Color32::WHITE);
    let x = rect.left() + 70.0;
    let visible_width = (rect.right() - x - 170.0).max(70.0);
    let max_chars = (visible_width / 7.2).floor().max(8.0) as usize;
    let mut title = title.to_owned();
    if title.chars().count() > max_chars {
        title = title
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>()
            + "…";
    }
    let mut artist = artist.to_owned();
    if artist.chars().count() > max_chars {
        artist = artist
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>()
            + "…";
    }
    p.text(
        egui::pos2(x, rect.top() + 14.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(13.0),
        theme::WHITE,
    );
    p.text(
        egui::pos2(x, rect.top() + 35.0),
        Align2::LEFT_TOP,
        artist,
        FontId::proportional(11.0),
        theme::MUTED,
    );
    if width > 540.0 {
        p.text(
            egui::pos2(rect.right() - 115.0, rect.center().y),
            Align2::RIGHT_CENTER,
            format.to_uppercase(),
            FontId::proportional(10.5),
            theme::MUTED,
        );
    }
    let tint = if duplicate {
        theme::CORAL
    } else if status == "Keep" {
        theme::LAVENDER
    } else {
        theme::MUTED
    };
    p.text(
        egui::pos2(rect.right() - 13.0, rect.center().y),
        Align2::RIGHT_CENTER,
        if duplicate { "Review" } else { status },
        FontId::proportional(11.5),
        tint,
    );
    p.line_segment(
        [
            rect.left_bottom() + Vec2::new(11.0, -1.0),
            rect.right_bottom() + Vec2::new(-11.0, -1.0),
        ],
        Stroke::new(0.6, theme::STROKE),
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}
