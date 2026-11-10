//! Separate interactive layout prototype. No real media I/O or backend integration.
//! Rune and Rune Kit are isolated from the shipping egui binary.
use std::sync::OnceLock;
use rune::{AppSpec, Cx, El, col, row, text};
use rune_kit::{Button, Kit, PaletteSeed};

#[derive(Default)]
struct Demo { page: usize, dark: bool }

fn kit(dark: bool) -> &'static Kit {
    static DARK: OnceLock<Kit> = OnceLock::new();
    static LIGHT: OnceLock<Kit> = OnceLock::new();
    let hex = |s: &str| rune::color::hex(s).expect("valid palette hex");
    if dark {
        DARK.get_or_init(|| Kit::from_palette(PaletteSeed {
            bg: hex("#15090C"), fg: hex("#F9EEE8"), accent: hex("#F13A3D"),
        }))
    } else {
        LIGHT.get_or_init(|| Kit::from_palette(PaletteSeed {
            bg: hex("#F6F1ED"), fg: hex("#281618"), accent: hex("#B21C25"),
        }))
    }
}

fn section(title: &'static str, caption: &'static str) -> El {
    col().key(title).gap(9.0).padding(18.0)
        .child(text(title))
        .child(text(caption))
}

fn view(s: &Demo, _: &mut Cx<Demo>) -> El {
    let k = kit(s.dark);
    let tabs = ["Workspace", "Media Library", "Reel Library", "Music Categorizer", "Downloader", "AI Models"];
    let mut sidebar = col().key("sidebar").w(225.0).h_full().padding(14.0).gap(9.0)
        .child(text("GatherForge / FORGE EDITION"));
    for (i, title) in tabs.iter().enumerate() {
        let index = i;
        sidebar = sidebar.child(Button::new(*title).on_press(move |s: &mut Demo| s.page = index).build(k));
    }
    sidebar = sidebar.child(Button::new("Toggle light / dark").on_press(|s: &mut Demo| s.dark = !s.dark).build(k));
    let content = match s.page {
        1 => section("MEDIA LIBRARY", "Unified audio + Reel index / query / filters / review"),
        2 => section("REEL LIBRARY", "Reel previews / video / transcript / OCR / AI evidence"),
        3 => section("MUSIC CATEGORIZER", "Artists / duplicates / suggested tags / approval workflow"),
        4 => section("DOWNLOADER", "Persistent queue / speed / progress / retry / history"),
        5 => section("AI MODEL CENTER", "Installed models / download and verification / language routing"),
        _ => section("FORGE WORKSPACE", "Media Library   •   Reel Library   •   Music Categorizer   •   Downloader"),
    };
    rune_kit::root(
        row().key("forge-root").w_full().h_full().fill(k.c.page)
            .child(sidebar)
            .child(col().key("main").grow(1.0).h_full().padding(24.0).child(content)),
        Vec::new(),
    )
}

fn main() {
    rune::run(AppSpec::new("GatherForge / Rune Preview", (1250.0, 790.0), || Demo { page: 0, dark: true }, view));
}
