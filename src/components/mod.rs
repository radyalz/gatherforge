use eframe::egui::{self, Color32, RichText};

pub const BG: Color32 = Color32::from_rgb(15, 17, 25);
pub const PANEL: Color32 = Color32::from_rgb(27, 30, 43);
pub const ACCENT: Color32 = Color32::from_rgb(169, 146, 255);
pub const GOLD: Color32 = Color32::from_rgb(255, 196, 117);
pub const MUTED: Color32 = Color32::from_rgb(157, 164, 186);

pub fn heading(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(
        RichText::new(title)
            .size(30.0)
            .strong()
            .color(Color32::WHITE),
    );
    ui.add_space(4.0);
    ui.label(RichText::new(subtitle).size(14.0).color(MUTED));
    ui.add_space(22.0);
}

pub fn panel(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(PANEL)
        .corner_radius(egui::CornerRadius::same(16))
        .inner_margin(egui::Margin::same(18))
        .show(ui, content);
}

pub fn metric(ui: &mut egui::Ui, label: &str, value: String, note: &str) {
    panel(ui, |ui| {
        ui.set_min_width(170.0);
        ui.label(RichText::new(label).color(MUTED).size(12.0));
        ui.add_space(9.0);
        ui.label(
            RichText::new(value)
                .size(32.0)
                .strong()
                .color(Color32::WHITE),
        );
        ui.add_space(3.0);
        ui.label(RichText::new(note).color(ACCENT).size(12.0));
    });
}

pub fn nav(ui: &mut egui::Ui, active: bool, number: &str, label: &str) -> bool {
    let fill = if active {
        Color32::from_rgb(56, 48, 85)
    } else {
        BG
    };

    ui.add_sized(
        [196.0, 40.0],
        egui::Button::new(
            RichText::new(format!("{number}    {label}"))
                .size(14.0)
                .color(if active { Color32::WHITE } else { MUTED }),
        )
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(11)),
    )
    .clicked()
}

pub fn action(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).strong().color(BG))
        .fill(ACCENT)
        .corner_radius(egui::CornerRadius::same(10))
}
