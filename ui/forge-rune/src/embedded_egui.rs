//! Embedded egui component renderer inside Rune's single native window.
//!
//! There is no eframe::run_native, GPUI Application::run or second OS window here.
//! Rune owns input/lifecycle. egui owns the control's paint commands; a software
//! rasterizer converts egui's tessellated meshes to a BGRA ByteFrame, which Rune
//! composites via NativeSurface in the same GPU scene.
//!
//! No fake app state: an egui control sends its action through Rune's handler.
//! The registry only holds transient drawing information (hover/ripple/geometry).
use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Widget};
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use rune::native_surface::{ByteFrame, ColorInfo, Format};
use rune::{NativeSurface, Waker};
use crate::forge_components::{self,ButtonKind};

const TRACE_SAMPLES: usize = 80;

#[derive(Clone, Copy)]
struct Ripple { origin: Pos2, started: Instant }

pub struct EmbeddedButton {
    pub surface: NativeSurface,
    egui: egui::Context,
    software: EguiSoftwareRender,
    label: String,
    icon: Option<String>,
    kind: ButtonKind,
    primary: bool,
    selected: bool,
    dark: bool,
    width: u32,
    height: u32,
    hovered: bool,
    cursor: Pos2,
    ripple: Option<Ripple>,
    dirty: bool,
    frame: u64,
    start: Instant,
    rendered_at: Option<Instant>,
}

impl EmbeddedButton {
    fn new(label: &str, icon:Option<&str>, kind:ButtonKind, primary: bool, selected: bool, dark: bool, wake: Option<&Waker>) -> Self {
        let surface = NativeSurface::new();
        if let Some(wake) = wake { surface.set_waker(wake.clone()); }
        Self {
            surface, egui: egui::Context::default(),
            software: EguiSoftwareRender::new(ColorFieldOrder::Bgra),
            label: label.to_owned(),icon:icon.map(str::to_owned),kind, primary, selected, dark,
            width: 100, height: forge_components::button(kind).height as u32, hovered: false,
            cursor: Pos2::new(50.0, 17.0), ripple: None,
            dirty: true, frame: 0, start: Instant::now(), rendered_at: None,
        }
    }

    fn configure(&mut self,label:&str,icon:Option<&str>,kind:ButtonKind,primary:bool,selected:bool,dark:bool,width:f32,height:f32) {
        let width = width.clamp(24.0, 2048.0).round() as u32;
        let height = height.clamp(20.0, 160.0).round() as u32;
        if self.label != label || self.icon.as_deref() != icon || self.kind != kind || self.primary != primary || self.selected != selected || self.dark != dark
            || self.width != width || self.height != height {
            self.label=label.to_owned();self.icon=icon.map(str::to_owned);self.kind=kind;self.primary=primary;self.selected=selected;
            self.dark = dark; self.width = width; self.height = height;
            self.dirty = true;
        }
    }

    fn hover(&mut self, inside: bool) {
        if self.hovered != inside { self.hovered = inside; self.dirty = true; }
    }

    fn move_pointer(&mut self, x: f32, y: f32) {
        let point = Pos2::new(x.clamp(0.0, self.width as f32), y.clamp(0.0, self.height as f32));
        if self.cursor != point { self.cursor = point; self.dirty = true; }
    }

    fn click(&mut self) {
        self.ripple = Some(Ripple { origin: self.cursor, started: Instant::now() });
        self.dirty = true;
    }

    fn animating(&self) -> bool {
        self.hovered || self.ripple.is_some_and(|r| r.started.elapsed().as_secs_f64() < forge_components::interaction().ripple)
    }

    fn render(&mut self) -> Result<bool, String> {
        if !self.dirty && !self.animating() { return Ok(false); }
        // Keep animated renders under ~60fps, even if Rune is polling faster.
        if self.rendered_at.is_some_and(|last| last.elapsed() < Duration::from_millis(15)) {return Ok(false);}
        if self.ripple.is_some_and(|r| r.started.elapsed().as_secs_f64() >= forge_components::interaction().ripple) {self.ripple = None;}
        let width = self.width as usize;
        let height = self.height as usize;
        let mut pixels = vec![[0u8; 4]; width * height];
        let data = PaintButton {
            label: &self.label, icon:self.icon.as_deref(),kind:self.kind,primary: self.primary, selected: self.selected,
            dark: self.dark, hovered: self.hovered,
            ripple: self.ripple.map(|r| (r.origin, (r.started.elapsed().as_secs_f64() / forge_components::interaction().ripple) as f32)),
            phase: (self.start.elapsed().as_secs_f64() / forge_components::interaction().revolution).fract() as f32,
        };
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(self.width as f32, self.height as f32))),
            time: Some(self.start.elapsed().as_secs_f64()),
            ..Default::default()
        };
        let output = self.egui.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(Color32::TRANSPARENT).inner_margin(egui::Margin::same(0)))
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    ui.add(data.clone());
                });
        });
        let prims = self.egui.tessellate(output.shapes, output.pixels_per_point);
        {
            let mut buffer = BufferMutRef::new(&mut pixels, width, height);
            self.software.render(&mut buffer, &prims, &output.textures_delta, output.pixels_per_point);
        }
        // EguiSoftwareRender's BGRA order matches Rune's portable native surface.
        let bytes: Vec<u8> = pixels.into_iter().flat_map(|pixel| pixel).collect();
        self.frame += 1;
        self.surface.push_bytes(ByteFrame {
            width: self.width, height: self.height, format: Format::Bgra,
            color: ColorInfo::default(), planes: [bytes, Vec::new()],
        }, self.frame)?;
        self.dirty = false;
        self.rendered_at = Some(Instant::now());
        Ok(true)
    }
}

#[derive(Default)]
pub struct EmbeddedButtons {
    buttons: HashMap<String, EmbeddedButton>,
    wake: Option<Waker>,
}
impl EmbeddedButtons {
    pub fn setup(&mut self, wake: Waker) {
        for button in self.buttons.values() { button.surface.set_waker(wake.clone()); }
        self.wake = Some(wake);
    }
    pub fn surface(&self, id: &str) -> Option<&NativeSurface> {
        self.buttons.get(id).map(|b| &b.surface)
    }
    pub fn configure(&mut self,id:&str,label:&str,icon:Option<&str>,kind:ButtonKind,primary:bool,selected:bool,dark:bool,w:f32,h:f32) {
        let wake = self.wake.clone();
        self.buttons.entry(id.to_owned()).or_insert_with(|| EmbeddedButton::new(label,icon,kind,primary,selected,dark,wake.as_ref()))
            .configure(label,icon,kind,primary,selected,dark,w,h);
    }
    pub fn hover(&mut self, id: &str, inside: bool) {
        if let Some(b) = self.buttons.get_mut(id) { b.hover(inside); }
    }
    pub fn move_pointer(&mut self, id: &str, x: f32, y: f32) {
        if let Some(b) = self.buttons.get_mut(id) { b.move_pointer(x, y); }
    }
    pub fn click(&mut self, id: &str) {
        if let Some(b) = self.buttons.get_mut(id) { b.click(); }
    }
    pub fn cancel_all_hovers(&mut self) {
        // Page changes can unmount hovered controls without another leave event.
        // Stop their animation before Rune switches to the next page.
        for b in self.buttons.values_mut() {b.hover(false);b.ripple=None;}
    }
    pub fn is_animating(&self) -> bool { self.buttons.values().any(EmbeddedButton::animating) }
    pub fn render(&mut self) -> Result<bool, String> {
        let mut rendered = false;
        for b in self.buttons.values_mut() { rendered |= b.render()?; }
        Ok(rendered)
    }
    pub fn total(&self) -> usize { self.buttons.len() }
}

#[derive(Clone)]
struct PaintButton<'a> {
    label: &'a str,
    icon:Option<&'a str>,
    kind:ButtonKind,
    primary: bool,
    selected: bool,
    dark: bool,
    hovered: bool,
    ripple: Option<(Pos2, f32)>,
    phase: f32,
}

fn perimeter(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius.min(rect.width().min(rect.height()) * 0.5);
    let corners = [
        (Pos2::new(rect.right() - r, rect.top() + r), -90.0_f32, 0.0_f32),
        (Pos2::new(rect.right() - r, rect.bottom() - r), 0.0, 90.0),
        (Pos2::new(rect.left() + r, rect.bottom() - r), 90.0, 180.0),
        (Pos2::new(rect.left() + r, rect.top() + r), 180.0, 270.0),
    ];
    let mut out = Vec::with_capacity(65);
    for (c, a, b) in corners {
        for i in 0..16 {
            let angle = (a + (b-a) * (i as f32 / 16.0)).to_radians();
            out.push(Pos2::new(c.x + r * angle.cos(), c.y + r * angle.sin()));
        }
    }
    out.push(out[0]);
    out
}
fn path_point(path: &[Pos2], phase: f32) -> (Pos2, egui::Vec2) {
    let mut total = 0.0_f32;
    for pair in path.windows(2) { total += pair[0].distance(pair[1]); }
    if total <= f32::EPSILON {return (path[0], egui::vec2(0.0,1.0));}
    let mut distance = phase.rem_euclid(1.0) * total;
    for pair in path.windows(2) {
        let length = pair[0].distance(pair[1]);
        if distance <= length && length > 0.0 {
            let direction = (pair[1] - pair[0]) / length;
            return (pair[0] + direction * distance, egui::vec2(-direction.y, direction.x));
        }
        distance -= length;
    }
    (path[0], egui::vec2(0.0,1.0))
}
fn tracer(ui:&egui::Ui,rect:Rect,phase:f32,accent:Color32,radius:f32) {
    let path=perimeter(rect.shrink(2.0),(radius-2.0).max(0.0));
    for (half_width, opacity) in [(4.0,0.065),(2.5,0.15),(0.9,0.95)] {
        let mut mesh = egui::Mesh::default();
        for i in 0..=TRACE_SAMPLES {
            let t = i as f32 / TRACE_SAMPLES as f32;
            let (point, normal) = path_point(&path, phase - forge_components::interaction().fraction + forge_components::interaction().fraction * t);
            let a = ((t*t*(3.0 - 2.0*t)) * opacity * 255.0) as u8;
            let ink = Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), a);
            mesh.colored_vertex(point + normal * half_width, ink);
            mesh.colored_vertex(point - normal * half_width, ink);
            if i > 0 {
                let prev = (2 * (i-1)) as u32;
                let cur = (2 * i) as u32;
                mesh.add_triangle(prev, prev+1, cur);
                mesh.add_triangle(prev+1, cur+1, cur);
            }
        }
        ui.painter().add(egui::Shape::mesh(mesh));
    }
}
fn ripple(ui:&egui::Ui,rect:Rect,origin:Pos2,progress:f32,color:Color32,radius:f32) {
    let p = progress.clamp(0.0,1.0);
    let alpha = ((1.0-p).powf(1.6) * 90.0) as u8;
    let outer = Color32::from_rgba_unmultiplied(color.r(),color.g(),color.b(),0);
    let inner = Color32::from_rgba_unmultiplied(color.r(),color.g(),color.b(),alpha);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(origin,inner);
    let max_radius = rect.size().length()*0.66;
    let r = p * max_radius;
    for i in 0..=64 {
        let angle = i as f32 * std::f32::consts::TAU / 64.0;
        let pt = origin + egui::vec2(angle.cos(),angle.sin())*r;
        mesh.colored_vertex(Pos2::new(pt.x.clamp(rect.left()+radius,rect.right()-radius),
                                       pt.y.clamp(rect.top()+radius,rect.bottom()-radius)), outer);
    }
    for i in 0..64u32 {mesh.add_triangle(0,i+1,i+2);}
    ui.painter().add(egui::Shape::mesh(mesh));
}
impl Widget for PaintButton<'_> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let size = ui.available_size_before_wrap();
        let (rect,response) = ui.allocate_exact_size(size, Sense::hover());
        if !ui.is_rect_visible(rect) {return response;}
        let accent = if self.primary {Color32::from_rgb(244,50,55)} else {Color32::from_rgb(226,174,105)};
        let foreground = if self.dark {Color32::from_rgb(251,241,231)}else{Color32::from_rgb(42,24,29)};
        let base = if self.dark {
            if self.primary {Color32::from_rgb(113,13,26)} else {Color32::from_rgb(24,13,17)}
        } else if self.primary {Color32::from_rgb(235,186,184)}else{Color32::from_rgb(241,235,226)};
        let cfg=forge_components::button(self.kind);
        let radius=cfg.radius.min(rect.height()*0.5);
        let painter=ui.painter();
        painter.rect_filled(rect,radius,if self.hovered {base.gamma_multiply(1.35)}else{base});
        if self.selected {
            for inset in [2.0_f32,4.0,6.0] {
                painter.rect_stroke(rect.shrink(inset),(radius-inset*0.36).max(1.0),
                    Stroke::new(1.0_f32,accent.gamma_multiply((9.0-inset)/28.0)),egui::StrokeKind::Inside);
            }
        }
        painter.rect_stroke(rect.shrink(0.5), radius, Stroke::new(1.0_f32,accent.gamma_multiply(0.68)),egui::StrokeKind::Inside);
        if let Some((origin,p))=self.ripple {ripple(ui,rect,origin,p,accent,radius);}
        if self.hovered {tracer(ui,rect,self.phase,accent,radius);}
        let content=match self.icon {
            Some(icon) if self.label.is_empty()=>icon.to_owned(),
            Some(icon)=>format!("{icon}  {}",self.label),
            None=>self.label.to_owned()
        };
        painter.text(rect.center(),egui::Align2::CENTER_CENTER,content,
                     egui::FontId::proportional(cfg.font_px),foreground);
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_is_closed_and_stays_in_control() {
        let rect=Rect::from_min_size(Pos2::ZERO,egui::vec2(120.0,34.0));
        let p=perimeter(rect,forge_components::button(ButtonKind::Regular).radius);
        assert_eq!(p.first(),p.last());
        for f in [0.0_f32,0.16,0.72,1.0] {
            let (pos,_)=path_point(&p,f);
            assert!(rect.contains(pos) || (pos.x==rect.right() || pos.y==rect.bottom()));
        }
    }
    #[test]
    fn controls_have_independent_state() {
        let mut buttons=EmbeddedButtons::default();
        buttons.configure("one","KEEP",None,ButtonKind::Regular,false,false,true,110.0,30.0);
        buttons.configure("two","REVIEW",None,ButtonKind::Pill,true,true,true,110.0,26.0);
        buttons.hover("one",true);buttons.click("one");
        assert_eq!(buttons.total(),2);
        assert!(buttons.is_animating());
        assert!(buttons.surface("two").is_some());
    }
}
