//! GatherForge shared design contracts. The only editable defaults live in components.json.
//! No Rune, GPUI, or egui dependency: every renderer can use these same values.
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind { Regular, Compact, Pill, Icon, IconLabel }
impl ButtonKind {
    fn key(self) -> &'static str {
        match self {Self::Regular=>"regular",Self::Compact=>"compact",Self::Pill=>"pill",Self::Icon=>"icon",Self::IconLabel=>"icon_label"}
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ButtonPreset { pub height:f32, pub radius:f32, pub minimum_width:f32, pub padding_x:f32, pub font_px:f32 }
#[derive(Clone, Copy, Debug)]
pub struct TablePreset {pub row_height:f32, pub header_height:f32, pub cell_padding:f32, pub zebra:bool, pub show_art:bool}
#[derive(Clone, Copy, Debug)]
pub struct ColumnPreset {pub width:Option<f32>,pub flex:f32,pub minimum:f32,pub resizable:bool,pub sortable:bool}
pub fn column(table:&str,key:&str)->ColumnPreset {
    let v=&data()["columns"][table][key];
    assert!(v.is_object(),"unknown Forge column {table}.{key}");
    ColumnPreset {width:v["width"].as_f64().map(|n|n as f32),flex:v["flex"].as_f64().unwrap_or(1.0) as f32,
        minimum:v["minimum"].as_f64().unwrap_or(45.0) as f32,resizable:v["resizable"].as_bool().unwrap_or(true),
        sortable:v["sortable"].as_bool().unwrap_or(false)}
}
#[derive(Clone, Copy, Debug)]
pub struct InteractionPreset {pub revolution:f64, pub fraction:f32, pub ripple:f64, pub frame_millis:u64}

fn data() -> &'static serde_json::Value {
    static CONFIG:OnceLock<serde_json::Value>=OnceLock::new();
    CONFIG.get_or_init(||serde_json::from_str(include_str!("components.json"))
        .expect("ui/shared/components.json contains valid JSON"))
}
fn number(v:&serde_json::Value,key:&str)->f32 {v[key].as_f64().expect("missing numeric Forge token") as f32}
pub fn button(kind:ButtonKind)->ButtonPreset {
    let v=&data()["buttons"][kind.key()];
    let height=number(v,"height");
    let raw_radius=number(v,"radius");
    ButtonPreset {height, radius:raw_radius.min(height*0.5), minimum_width:number(v,"minimum_width"),padding_x:number(v,"padding_x"),font_px:number(v,"font_px")}
}
pub fn table(name:&str)->TablePreset {
    let v=&data()["tables"][name];
    assert!(v.is_object(),"unknown Forge table preset: {name}");
    TablePreset {row_height:number(v,"row_height"),header_height:number(v,"header_height"),cell_padding:number(v,"cell_padding"),zebra:v["zebra"].as_bool().unwrap_or(true),show_art:v["show_art"].as_bool().unwrap_or(false)}
}
pub fn interaction()->InteractionPreset {
    let v=&data()["interaction"];
    InteractionPreset {revolution:v["trace_revolution_seconds"].as_f64().expect("trace period"),fraction:number(v,"trace_fraction"),ripple:v["ripple_seconds"].as_f64().expect("ripple period"),frame_millis:(1000.0/v["trace_frames_per_second"].as_f64().unwrap_or(60.0)).round().max(1.0) as u64}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn one_source_of_truth() {
        assert_eq!(button(ButtonKind::Pill).radius,13.0);
        assert!(button(ButtonKind::Regular).height < 34.0);
        assert_eq!(button(ButtonKind::Icon).minimum_width,28.0);
        for name in ["media","music","reels","downloads"] {assert!(table(name).row_height>=30.0);}
    }
}
