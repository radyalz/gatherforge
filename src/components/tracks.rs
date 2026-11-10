use crate::theme;
use eframe::egui::{self, Align2, Color32, FontId, Stroke, Vec2};

const MUSIC_CARD_RADIUS: f32 = 8.0;

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

fn truncate(text: &str, columns: usize) -> String {
    if text.chars().count() <= columns {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(columns.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub fn track_row(
    ui: &mut egui::Ui, artist: &str, title: &str, format: &str,
    status: &str, selected: bool, duplicate: bool, artwork: Option<&egui::TextureHandle>,
) -> bool {
    let width = ui.available_width().max(1.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 202.0), egui::Sense::click());
    let p = ui.painter();
    let fill = if selected || response.hovered() { theme::glass_hover() } else { theme::surface() };
    p.rect_filled(rect, MUSIC_CARD_RADIUS, fill);
    let art = egui::Rect::from_min_size(rect.left_top() + Vec2::splat(10.0), Vec2::new((width - 20.0).max(1.0), 110.0));
    p.rect_filled(art, MUSIC_CARD_RADIUS, swatch(artist));
    if let Some(texture) = artwork {
        p.image(texture.id(), art, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    } else {
        p.circle_stroke(art.center(), 30.0, Stroke::new(2.0, Color32::from_white_alpha(160)));
        p.circle_filled(art.center(), 4.0, Color32::WHITE);
    }
    let chars = ((width - 20.0).max(1.0) / 7.2) as usize;
    p.text(rect.left_top() + Vec2::new(10.0, 128.0), Align2::LEFT_TOP, truncate(title, chars), FontId::proportional(14.0), theme::white());
    p.text(rect.left_top() + Vec2::new(10.0, 149.0), Align2::LEFT_TOP, truncate(artist, chars), FontId::proportional(12.0), theme::muted());
    let label = if duplicate { "Review match" } else { status };
    p.text(rect.left_top() + Vec2::new(10.0, 175.0), Align2::LEFT_TOP, format!("{}  ·  {}", format.to_uppercase(), label), FontId::proportional(10.5), if duplicate { theme::coral() } else { theme::lavender() });
    response.on_hover_text(if artwork.is_some() { "Select to review track details" } else { "Select to review track details; generated artwork placeholder" }).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}
