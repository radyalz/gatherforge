//! Forge buttons implemented in egui's GPU-rendered mesh layer, not Rune geometry.
//! Replaces the standard Button used throughout the existing egui workspace.
//! 4 px corners; a continuously travelling conic-style tracer while hovered;
//! click-origin radial ripple; independent primary and selected color semantics.
use crate::theme;
#[path = "../../ui/shared/forge_components.rs"] mod forge_components;
use forge_components::ButtonKind;
use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Widget};

fn corner(kind:ButtonKind)->f32 {forge_components::button(kind).radius}
fn trace_seconds()->f64 {forge_components::interaction().revolution}
fn ripple_seconds()->f64 {forge_components::interaction().ripple}
const SAMPLES_PER_CORNER: usize = 16;
const RING_SAMPLES: usize = 92;

#[derive(Clone, Copy)]
struct Ripple { at: Pos2, start: f64 }

pub struct ForgeButton<'a> {
    label: &'a str,
    icon:Option<&'a str>,
    kind:ButtonKind,
    primary: bool,
    selected: bool,
}

pub fn button(label: &str, primary: bool) -> ForgeButton<'_> {
    ForgeButton { label,icon:None,kind:ButtonKind::Regular,primary, selected: false }
}
pub fn icon_button(icon:&str,primary:bool)->ForgeButton<'_> {
    ForgeButton{label:"",icon:Some(icon),kind:ButtonKind::Icon,primary,selected:false}
}
pub fn button_with_icon<'a>(icon:&'a str,label:&'a str,primary:bool)->ForgeButton<'a> {
    ForgeButton{label,icon:Some(icon),kind:ButtonKind::IconLabel,primary,selected:false}
}
pub fn pill_button(label:&str,primary:bool)->ForgeButton<'_> {
    ForgeButton{label,icon:None,kind:ButtonKind::Pill,primary,selected:false}
}
impl<'a> ForgeButton<'a> {
    /// Selected is separate from primary. Only selected buttons keep an inset light.
    #[allow(dead_code)]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

fn rounded_outline(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius.min(rect.width().min(rect.height()) * 0.5);
    let corners = [
        (Pos2::new(rect.right()-r,rect.top()+r), -90.0_f32, 0.0_f32),
        (Pos2::new(rect.right()-r,rect.bottom()-r), 0.0, 90.0),
        (Pos2::new(rect.left()+r,rect.bottom()-r), 90.0,180.0),
        (Pos2::new(rect.left()+r,rect.top()+r), 180.0,270.0),
    ];
    let mut pts=Vec::with_capacity(SAMPLES_PER_CORNER*4+1);
    for (c,from,to) in corners {
        for i in 0..SAMPLES_PER_CORNER {
            let angle=(from+(to-from)*i as f32/SAMPLES_PER_CORNER as f32).to_radians();
            pts.push(Pos2::new(c.x+r*angle.cos(),c.y+r*angle.sin()));
        }
    }
    pts.push(pts[0]);
    pts
}

fn sample_path(points:&[Pos2], fraction:f32)->(Pos2,egui::Vec2) {
    let lens:Vec<f32>=points.windows(2).map(|p|p[0].distance(p[1])).collect();
    let sum:f32=lens.iter().sum();
    if sum<=0.0{return(points[0],egui::vec2(0.0,1.0));}
    let mut dist=fraction.rem_euclid(1.0)*sum;
    for (index,length) in lens.iter().enumerate() {
        if dist<=*length {
            let tangent=(points[index+1]-points[index]).normalized();
            let p=points[index]+tangent*dist;
            return (p,egui::vec2(-tangent.y,tangent.x));
        }
        dist-=length;
    }
    (points[0],egui::vec2(0.0,1.0))
}

// One GPU mesh per light layer. Alpha is interpolated between adjacent vertices,
// rather than faking glow with 20 separate opaque Rune strokes.
fn draw_tracer(ui:&egui::Ui, rect:Rect, phase:f32, base:Color32,radius:f32) {
    let edge = rounded_outline(rect.shrink(1.6),(radius-1.6).max(0.0));
    // A 22% tail, with its brightest point at the leading end.
    for (half_width,strength) in [(6.0,0.075),(3.7,0.15),(1.7,0.85)] {
        let mut mesh=egui::Mesh::default();
        for i in 0..=RING_SAMPLES {
            let f=i as f32/RING_SAMPLES as f32;
            let (p,n)=sample_path(&edge,phase-forge_components::interaction().fraction+forge_components::interaction().fraction*f);
            let t=f.powf(2.1);
            let alpha = (strength*t*255.0).min(255.0) as u8;
            let color=Color32::from_rgba_unmultiplied(base.r(),base.g(),base.b(),alpha);
            mesh.colored_vertex(p+n*half_width,color);
            mesh.colored_vertex(p-n*half_width,color);
            if i>0 {
                let a=(2*(i-1)) as u32;
                let b=(2*i) as u32;
                mesh.add_triangle(a,a+1,b);
                mesh.add_triangle(a+1,b+1,b);
            }
        }
        ui.painter().add(egui::Shape::mesh(mesh));
    }
}

// The ripple uses a radially alpha-graded mesh; drawing is confined well inside
// the rounded rim and does not use a hard opaque circle.
fn draw_ripple(ui:&egui::Ui,rect:Rect,at:Pos2,progress:f32,color:Color32,radius:f32) {
    let progress=progress.clamp(0.0,1.0);
    let max_radius=rect.size().length()*0.72;
    let r=max_radius*progress;
    let alpha=((1.0-progress).powf(1.7)*0.33*255.0) as u8;
    let inner=Color32::from_rgba_unmultiplied(color.r(),color.g(),color.b(),alpha);
    let clear=Color32::from_rgba_unmultiplied(color.r(),color.g(),color.b(),0);
    let mut mesh=egui::Mesh::default();
    mesh.colored_vertex(at,inner);
    let mut pts=Vec::with_capacity(65);
    for i in 0..=64 {
        let angle=i as f32*std::f32::consts::TAU/64.0;
        let pos=at+egui::vec2(angle.cos(),angle.sin())*r;
        let x=pos.x.clamp(rect.left()+radius,rect.right()-radius);
        let y=pos.y.clamp(rect.top()+radius,rect.bottom()-radius);
        pts.push(Pos2::new(x,y));
    }
    for p in pts {mesh.colored_vertex(p,clear);}
    for i in 0..64u32 {mesh.add_triangle(0,i+1,i+2);}
    ui.painter().add(egui::Shape::mesh(mesh));
}

impl Widget for ForgeButton<'_> {
    fn ui(self, ui:&mut egui::Ui)->Response {
        let enabled=ui.is_enabled();
        let accent=if self.primary {theme::SIGNAL}else{theme::GOLD};
        let fg=if enabled{theme::white()}else{theme::muted()};
        let config=forge_components::button(self.kind);
        let text=match self.icon {Some(icon) if self.label.is_empty()=>icon.to_owned(),Some(icon)=>format!("{icon}  {}",self.label),None=>self.label.to_owned()};
        let radius=config.radius;
        let font=egui::FontId::proportional(config.font_px);
        let galley=ui.painter().layout_no_wrap(text.clone(),font.clone(),fg);
        let desired=egui::vec2((galley.size().x+config.padding_x*2.0).max(config.minimum_width),config.height);
        let (rect,response)=ui.allocate_exact_size(desired,Sense::click());
        if !ui.is_rect_visible(rect){return response;}
        let painter=ui.painter();
        let dark=theme::surface();
        let fill=if self.primary{theme::hero()}else{dark};
        let hovered=enabled&&response.hovered();
        let pressed=enabled&&response.is_pointer_button_down_on();
        let selected=self.selected;
        let base=if pressed{accent.gamma_multiply(0.35)}else if hovered{theme::glass_hover()}else{fill};
        painter.rect_filled(rect,radius,base);
        if selected {
            for inset in [2.0,4.0,7.0] {
                let inner=rect.shrink(inset);
                painter.rect_stroke(inner,(radius-inset*0.38).max(1.0),Stroke::new(1.3,accent.gamma_multiply((9.0-inset)/36.0)),egui::StrokeKind::Inside);
            }
        }
        painter.rect_stroke(rect.shrink(0.5),radius,Stroke::new(1.1,accent.gamma_multiply(if enabled{0.70}else{0.22})),egui::StrokeKind::Inside);
        let now=ui.ctx().input(|i|i.time);
        let id=response.id.with("forge-ripple");
        if response.clicked() && enabled {
            let at=response.interact_pointer_pos().unwrap_or(rect.center());
            ui.ctx().data_mut(|data|data.insert_temp(id,Ripple{at,start:now}));
        }
        let ripple=ui.ctx().data(|data|data.get_temp::<Ripple>(id));
        if let Some(r)=ripple {
            let age=now-r.start;
            if age<ripple_seconds() && age>=0.0 {
                draw_ripple(ui,rect,r.at,(age/ripple_seconds()) as f32,accent,radius);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
            }else {ui.ctx().data_mut(|data|data.remove::<Ripple>(id));}
        }
        if hovered {
            let phase=(now/trace_seconds()).fract() as f32;
            draw_tracer(ui,rect,phase,accent,radius);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
        painter.text(rect.center(),egui::Align2::CENTER_CENTER,text,font,fg);
        response
    }
}

pub fn pill(ui:&mut egui::Ui,text:&str,color:Color32) {
    egui::Frame::new().fill(color.gamma_multiply(0.10))
        .corner_radius(egui::CornerRadius::same(theme::RADIUS))
        .inner_margin(egui::Margin::symmetric(8,5)).show(ui,|ui|{
            ui.label(egui::RichText::new(text).size(11.0).strong().color(color));
        });
}
pub fn section_heading(ui:&mut egui::Ui,_eyebrow:&str,title:&str,subtitle:&str) {
    ui.label(egui::RichText::new(title).size(19.0).strong().color(theme::white()));
    ui.add_space(3.0);
    ui.label(egui::RichText::new(subtitle).size(12.5).color(theme::muted()));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn closed_perimeter() {
        let p=rounded_outline(Rect::from_min_size(Pos2::ZERO,egui::vec2(100.0,36.0)),corner(ButtonKind::Regular));
        assert_eq!(p.first(),p.last());assert!(p.len()>60);
        for f in [0.0,0.20,0.59,0.99,1.0] {
            let (v,_)=sample_path(&p,f);
            assert!((0.0..=100.0).contains(&v.x) && (0.0..=36.0).contains(&v.y));
        }
    }
}
