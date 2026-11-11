//! Real GPUI Kit control deck. Separate native window/process, launched by the
//! Rune shell. Native egui runs the specialized editor pages and real backend.
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use std::{path::PathBuf, process::Command};

fn project_root() -> PathBuf {
    std::env::current_exe().ok().and_then(|p|p.parent().map(|p|p.to_path_buf()))
        .unwrap_or_else(||std::env::current_dir().unwrap_or_default())
}

fn launch_egui(page:&str) {
    let root=project_root();
    let candidates=[
        root.join("GatherForge-Rust-Linux"),
        root.join("GatherForge-Linux-x64"),
        root.join("target/release/gatherforge"),
        root.join("../../target/release/gatherforge"),
    ];
    let found=candidates.into_iter().find(|p|p.is_file());
    match found {
        Some(exe)=>match Command::new(&exe).arg("--forge-page").arg(page).spawn(){
            Ok(_) => println!("Opened egui workspace: {page}"),
            Err(e) => eprintln!("Failed to start {}: {e}",exe.display()),
        },
        None=>eprintln!("No egui worker executable found. Build the main GatherForge crate first."),
    }
}

struct ForgeOperatorDeck;
impl Render for ForgeOperatorDeck {
    fn render(&mut self,_:&mut Window,_:&mut Context<Self>)->impl IntoElement {
        div().v_flex().size_full().gap_3().p_5().bg(rgb(0x10080c))
            .child(div().text_color(rgb(0xf4eee8)).text_2xl().child("GATHERFORGE / OPERATOR DECK"))
            .child(div().text_color(rgb(0xdaa767)).text_sm().child(
                "GPUI Kit control surface  •  Rune owns the shell  •  egui owns the media workspaces"))
            .child(div().h_flex().gap_3()
                .child(Button::new("open-overview").primary().label("OPEN WORKSPACE")
                    .on_click(|_,_,_|launch_egui("overview")))
                .child(Button::new("open-media").label("MEDIA LIBRARY")
                    .on_click(|_,_,_|launch_egui("media"))))
            .child(div().h_flex().gap_3()
                .child(Button::new("open-music").primary().label("MUSIC CATEGORIZER")
                    .on_click(|_,_,_|launch_egui("music")))
                .child(Button::new("open-reels").label("REEL STUDIO")
                    .on_click(|_,_,_|launch_egui("reels"))))
            .child(div().h_flex().gap_3()
                .child(Button::new("open-downloads").label("DOWNLOADER")
                    .on_click(|_,_,_|launch_egui("downloads")))
                .child(Button::new("open-queue").label("TASKS & QUEUE")
                    .on_click(|_,_,_|launch_egui("queue"))))
            .child(div().text_color(rgb(0xa99793)).text_sm().child(
                "Controls launch the real egui engine with the requested page; no mock tracks or invented transfer progress."))
    }
}
fn main() {
    gpui_kit::application().run(|cx|{
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(),cx,|_,cx|cx.new(|_|ForgeOperatorDeck))
            .expect("Failed to open GPUI operator deck");
    });
}
