//! GatherForge Forge Edition — native Rune desktop.
//! Shares the real music, Instagram and persistent download engines with GatherForge.
//! No mock rows, no fake download progress, no invented transcripts.
use std::{borrow::Cow, collections::{BTreeMap,HashMap}, fs, path::{Path, PathBuf}, sync::{mpsc::{self, Receiver}, OnceLock}, process::Command, thread, time::{Duration,Instant}};
use rune::{AppSpec, Color, Cx, El, Image, ImageCache, Player, PlayerState, ScrollSpec, Shape, TextEdit, Tx, Waker, canvas, col, el, row, text};
use rune_kit::{Cell, Kit, PaletteSeed, Table, TableColumn, TableState, TableStyle, TextInput};
use serde_json::Value;

#[path = "../../../src/model.rs"] mod model;
#[path = "../../../src/util.rs"] mod util;
#[path = "../../../src/library.rs"] mod library;
#[path = "../../../src/downloader.rs"] mod downloader;
#[path = "../../../src/instagram.rs"] mod instagram;
#[path = "../../../src/jobs.rs"] mod jobs;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page { Workspace, Media, Reels, Music, Downloader, Models, Queue, Settings }
impl Page {
    fn name(self) -> &'static str {
        match self { Self::Workspace=>"Workspace", Self::Media=>"Media Library", Self::Reels=>"Reel Studio", Self::Music=>"Music Categorizer", Self::Downloader=>"Downloader", Self::Models=>"AI Model Center", Self::Queue=>"Tasks & Queue", Self::Settings=>"Settings" }
    }
}
const PAGES: &[Page] = &[Page::Workspace,Page::Media,Page::Reels,Page::Music,Page::Downloader,Page::Models,Page::Queue,Page::Settings];

#[derive(Clone,Copy)]
struct Paint { bg: Color, pane: Color, pane2: Color, side: Color, red: Color, red_dim: Color, brass: Color, ink: Color, muted: Color, line: Color, success: Color }
fn hex(s: &str) -> Color { rune::color::hex(s).expect("static palette") }
fn paint(dark: bool) -> Paint {
    if dark { Paint { bg:hex("#070507"), pane:hex("#0D070A"), pane2:hex("#180B0F"), side:hex("#0B0508"), red:hex("#F02129"), red_dim:hex("#651118"), brass:hex("#DAA767"), ink:hex("#F9EEE5"), muted:hex("#AFA09D"), line:hex("#5B3030"), success:hex("#69CA83") } }
    else { Paint { bg:hex("#ECE8E3"), pane:hex("#FCF9F6"), pane2:hex("#E9DDDA"), side:hex("#252022"), red:hex("#B81620"), red_dim:hex("#EEC3C0"), brass:hex("#9C6B30"), ink:hex("#2A1B1E"), muted:hex("#795F65"), line:hex("#A88A72"), success:hex("#247445") } }
}
fn kit(dark: bool) -> &'static Kit {
    static DARK: OnceLock<Kit> = OnceLock::new(); static LIGHT: OnceLock<Kit> = OnceLock::new();
    let build = |dark| {
        let p = paint(dark);
        let mut k = Kit::from_palette(PaletteSeed { bg:p.bg, fg:p.ink, accent:p.red });
        k.radius.sm=2.0; k.radius.md=2.0; k.radius.lg=2.0; k.radius.xl=2.0; k
    };
    if dark { DARK.get_or_init(||build(true)) } else { LIGHT.get_or_init(||build(false)) }
}

#[derive(Clone)]
struct ModelRow { id:String, name:String, role:String, status:String, size:String }
#[derive(Clone, Copy)]
enum MediaRef { Audio(usize), Reel(usize) }
const REEL_CATEGORIES: &[&str] = &["Food and recipes","Fitness and health","Travel and places","Technology and gadgets","Home and DIY","Education and tutorials","Entertainment and humor","Shopping and products","Other"];
struct TaskResult { result:Result<model::Library,String> }
enum WorkerMsg { Scan(TaskResult), Reels(Result<Vec<instagram::Reel>,String>), Model(Result<String,String>), Analysis(Result<String,String>), Thumb(String,Vec<u8>), Wave(u64,Vec<f32>) }
struct App {
    page: Page, dark: bool, root: PathBuf, reel_root: PathBuf,
    lib: model::Library, reels: Vec<instagram::Reel>, queue: jobs::Queue,
    search: TextEdit, url: TextEdit, artist_edit: TextEdit, selected_track: Option<u64>, selected_reel: Option<String>,
    clip: Option<Player>, audio: Option<Player>, table_media: TableState, table_music: TableState, table_jobs: TableState,
    media_rows: Vec<MediaRef>, music_rows: Vec<usize>, queue_rows: Vec<usize>,
    filter: usize, source: usize, queue_filter: usize, category: usize, models: Vec<ModelRow>,
    confirm: Option<&'static str>, notice: String, busy: bool, rx: Receiver<WorkerMsg>, tx: mpsc::Sender<WorkerMsg>,
    video_error: Option<String>, model_busy:bool, analysis_busy:bool,
    images: Option<ImageCache>, emblem: Option<Image>, anvil: Option<Image>, metal: Option<Image>, thumbs: HashMap<String,Image>,
    waveform: Vec<f32>, cpu:f32, cpu_prev:Option<(u64,u64)>, ram_used:u64, ram_total:u64, last_stats:Instant,
}
fn prefs() -> (PathBuf,PathBuf,bool) {
    let settings=dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("gatherforge/settings.json");
    let value:Value=fs::read(&settings).ok().and_then(|bytes|serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null);
    let audio=value.get("library").and_then(Value::as_str).map(PathBuf::from).filter(|p|p.is_dir()).or_else(dirs::audio_dir).unwrap_or_else(||PathBuf::from("."));
    let reels=value.get("reels_folder").and_then(Value::as_str).map(PathBuf::from).unwrap_or_else(||dirs::home_dir().unwrap_or_else(||PathBuf::from(".")).join("Videos/GatherForge/Reels"));
    let dark=value.get("dark").and_then(Value::as_bool).unwrap_or(true);
    (audio,reels,dark)
}
fn model_rows() -> Vec<ModelRow> {
    let base=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models");
    let Ok(bytes)=fs::read(base.join("catalog.json")) else {return vec![]};
    let Ok(root)=serde_json::from_slice::<Value>(&bytes) else {return vec![]};
    let array=root.as_array().or_else(||root.get("models").and_then(Value::as_array));
    array.map(|rows|rows.iter().filter_map(|v|{
        let name=v.get("title").or_else(||v.get("name")).or_else(||v.get("id")).and_then(Value::as_str)?;
        let id=v.get("id").and_then(Value::as_str).unwrap_or(name);
        let role=v.get("role").or_else(||v.get("purpose")).or_else(||v.get("description")).or_else(||v.get("task")).and_then(Value::as_str).unwrap_or("Model");
        let size=v.get("size_mb").and_then(Value::as_u64).map(|x|format!("{x} MB")).unwrap_or_default();
        let relative=v.get("relative_path").and_then(Value::as_str).unwrap_or(id);
        let installed=base.join("installed").join(relative).exists() || base.join("bundled").join(relative).exists();
        Some(ModelRow{id:id.to_owned(),name:name.to_owned(),role:role.to_owned(),size,status:if installed{"On disk"}else{"Not found"}.into()})
    }).collect()).unwrap_or_default()
}
fn init() -> App {
    let (root,reel_root,dark)=prefs();
    let lib=library::load(&root).unwrap_or_default();
    let reels=instagram::refresh(&reel_root).unwrap_or_default();
    let queue=jobs::Queue::load();
    let (tx,rx)=mpsc::channel();
    let mut s=App { page:Page::Workspace,dark,root,reel_root,lib,reels,queue,
        search:TextEdit::new(""),url:TextEdit::new(""),artist_edit:TextEdit::new(""),selected_track:None,selected_reel:None,
        clip:None,audio:None,table_media:TableState::new(),table_music:TableState::new(),table_jobs:TableState::new(),
        media_rows:vec![],music_rows:vec![],queue_rows:vec![],filter:0,source:0,queue_filter:0,category:0,
        models:model_rows(),confirm:None,notice:"LOCAL FORGE ENGINE  /  READY".into(),busy:false,rx,tx,video_error:None,model_busy:false,analysis_busy:false,
        images:None,emblem:None,anvil:None,metal:None,thumbs:HashMap::new(),waveform:vec![],cpu:0.0,cpu_prev:None,ram_used:0,ram_total:0,last_stats:Instant::now() };
    s.recompute(); if !s.reels.is_empty(){s.choose_reel(0);} if let Some(t)=s.lib.tracks.first(){let id=t.id;s.select_track(id);} s
}
impl App {
    fn recompute(&mut self) {
        let needle=self.search.text.trim().to_lowercase();
        self.music_rows=self.lib.tracks.iter().enumerate().filter(|(_,t)|{
            needle.is_empty() || format!("{} {} {} {}",t.title,t.artist,t.filename,t.format).to_lowercase().contains(&needle)
        }).map(|(i,_)|i).collect();
        self.media_rows=Vec::new();
        if self.filter!=2 {self.media_rows.extend(self.music_rows.iter().copied().map(MediaRef::Audio));}
        if self.filter!=1 {self.media_rows.extend(self.reels.iter().enumerate().filter(|(_,r)|{
            needle.is_empty() || format!("{} {} {}",r.creator,r.caption,r.id).to_lowercase().contains(&needle)
        }).map(|(i,_)|MediaRef::Reel(i)));}
        self.queue_rows=self.queue.jobs().iter().enumerate().filter(|(_,j)|{
            let valid=match self.queue_filter {1=>j.state.is_pending(),2=>j.state==jobs::State::Completed,3=>j.state.can_retry(),_=>true};
            valid && (needle.is_empty() || format!("{} {}",j.source,j.current_item).to_lowercase().contains(&needle))
        }).map(|(i,_)|i).collect();
    }
    fn switch(&mut self,page:Page){self.page=page;self.search.text.clear();self.recompute();self.confirm=None;}
    fn choose_reel(&mut self,index:usize) {
        if let Some(reel)=self.reels.get(index) {
            let path=self.reel_root.join(&reel.video);
            self.selected_reel=Some(reel.id.clone());
            self.clip=None;
            if let (Ok(base),Ok(full))=(self.reel_root.canonicalize(),path.canonicalize()) {
                if full.starts_with(&base) && full.is_file() {
                    let player=Player::new(full.to_string_lossy().to_string().into());
                    self.clip=Some(player);
                    self.video_error=None;
                } else {self.video_error=Some("Media path is not valid".into());}
            } else {self.video_error=Some("Original video is missing".into());}
        }
    }
    fn select_track(&mut self,id:u64) {
        self.selected_track=Some(id);self.audio=None;self.waveform.clear();
        if let Some(t)=self.lib.tracks.iter().find(|t|t.id==id) {
            self.artist_edit.text=t.artist.clone();
            let source=self.root.join(&t.original);
            if let (Ok(base),Ok(path))=(self.root.canonicalize(),source.canonicalize()) {
                if path.starts_with(&base)&&path.is_file() {self.audio=Some(Player::audio(path.to_string_lossy().to_string()));
                    let sender=self.tx.clone();let media=path.clone();thread::spawn(move||{
                        if let Some(values)=wave_samples(&media){let _=sender.send(WorkerMsg::Wave(id,values));}
                    });}
            }
        }
    }
    fn scan(&mut self) {
        if self.busy {return;} self.busy=true;self.notice="Scanning audio library on a worker thread...".into();
        let root=self.root.clone();let old=self.lib.clone();let sender=self.tx.clone();
        thread::spawn(move||{let result=library::scan(&root,&old);let _=sender.send(WorkerMsg::Scan(TaskResult{result}));});
    }
    fn refresh_reels(&mut self) {
        let root=self.reel_root.clone();let sender=self.tx.clone();
        self.notice="Refreshing local Reel manifests...".into();
        thread::spawn(move||{let _=sender.send(WorkerMsg::Reels(instagram::refresh(&root)));});
    }
    fn add_url(&mut self) {
        let raw=self.url.text.trim().to_string();
        let kind=if self.source==0 {jobs::Kind::Music{root:self.root.clone(),mp3:false,browser:"none".into(),single:false}}
            else {jobs::Kind::Reel{root:self.reel_root.clone(),browser:"none".into()}};
        match self.queue.add_urls(&raw,kind){Ok(n)=>{self.url.text.clear();self.notice=format!("Queued {n} URL(s)");self.recompute();},Err(e)=>self.notice=e}
    }
    fn mark(&mut self,dec:model::Decision) {
        if let Some(id)=self.selected_track {match library::update_track(&self.root,&mut self.lib,id,None,Some(dec)) {Ok(())=>self.notice=format!("Track {id}: {}",dec.label()),Err(e)=>self.notice=e};}
    }
    fn rename(&mut self) {
        if let Some(id)=self.selected_track {
            let new=self.artist_edit.text.trim().to_owned();
            if new.is_empty(){self.notice="Artist cannot be empty".into();return;}
            match library::update_track(&self.root,&mut self.lib,id,Some(&new),None) {Ok(())=>self.notice="Artist saved".into(),Err(e)=>self.notice=e};
            self.recompute();
        }
    }
    fn model_operation(&mut self,id:String,command:&'static str) {
        if self.model_busy{return;}
        if !matches!(command,"verify"|"install"){self.notice="Unsupported model command".into();return;}
        self.model_busy=true;
        let sender=self.tx.clone();
        let executable=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../GatherForge-Rust-Linux");
        self.notice=format!("{command} model {id}...");
        thread::spawn(move||{
            let result=Command::new(&executable).arg("--forge-model").arg(command).arg(&id)
                .output().map_err(|e|e.to_string()).and_then(|output|if output.status.success(){
                    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
                }else{Err(String::from_utf8_lossy(&output.stderr).chars().take(500).collect())});
            let _=sender.send(WorkerMsg::Model(result));
        });
    }
    fn analyze_selected(&mut self) {
        if self.analysis_busy{return;}
        let Some(id)=self.selected_reel.as_ref() else {self.notice="Select a Reel".into();return};
        let Some(reel)=self.reels.iter().find(|r|&r.id==id) else {return};
        let source=self.reel_root.join(&reel.video);
        let (Ok(base),Ok(media))=(self.reel_root.canonicalize(),source.canonicalize()) else {self.notice="Original Reel unavailable".into();return};
        if !media.starts_with(&base)||!media.is_file() {self.notice="Unsafe Reel path".into();return;}
        let video=reel.video.clone();
        let root=self.reel_root.clone();
        let models=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models");
        let script=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/reel_analysis.py");
        let sender=self.tx.clone();
        self.analysis_busy=true;self.notice="Analyzing selected Reel using local tools...".into();
        thread::spawn(move||{
            let result=Command::new("python3").arg(&script).arg("analyze")
                .arg("--root").arg(&root).arg("--video").arg(&video).arg("--models").arg(&models)
                .output().map_err(|e|e.to_string()).and_then(|output|if output.status.success(){
                    Ok(String::from_utf8_lossy(&output.stdout).chars().take(250).collect())
                }else{Err(String::from_utf8_lossy(&output.stderr).chars().take(400).collect())});
            let _=sender.send(WorkerMsg::Analysis(result));
        });
    }
    fn reel_operation(&mut self,operation:&'static str) {
        if self.analysis_busy {return;}
        if !matches!(operation,"record-category"|"approve") {return;}
        let Some(id)=self.selected_reel.as_ref() else {self.notice="Choose a Reel".into();return};
        let Some(reel)=self.reels.iter().find(|r|&r.id==id) else {return};
        let root=self.reel_root.clone();
        let video=reel.video.clone();
        let category=REEL_CATEGORIES[self.category].to_string();
        let script=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/reel_analysis.py");
        let sender=self.tx.clone();
        self.analysis_busy=true;
        thread::spawn(move||{
            let result=Command::new("python3").arg(&script).arg(operation)
                .arg("--root").arg(&root).arg("--video").arg(&video)
                .arg("--category").arg(&category).output().map_err(|e|e.to_string())
                .and_then(|output|if output.status.success(){Ok(format!("{operation} completed"))}
                    else{Err(String::from_utf8_lossy(&output.stderr).chars().take(450).collect())});
            let _=sender.send(WorkerMsg::Analysis(result));
        });
    }
    fn commit_operation(&mut self) {
        let Some(op)=self.confirm.take() else {return};
        if op=="approve_reel" {self.reel_operation("approve");return;}
        let result=match op {"stage"=>library::stage(&self.root,&mut self.lib),"publish"=>library::publish(&self.root,&mut self.lib),"restore"=>library::restore(&self.root,&mut self.lib),"finalize"=>library::finalize(&self.root,&mut self.lib),_=>Err("Unknown operation".into())};
        self.notice=match result {Ok(())=>format!("{op} completed"),Err(e)=>e};self.recompute();
    }
}
// Images are derived from actual local media. No sample media or invented metadata is displayed.
fn video_png(path:&Path,second:f64)->Option<Vec<u8>> {
    let out=Command::new("ffmpeg").args(["-hide_banner","-loglevel","error","-ss"])
        .arg(format!("{second:.3}")).arg("-i").arg(path)
        .args(["-frames:v","1","-vf","scale=320:-2","-f","image2pipe","-vcodec","png","pipe:1"])
        .output().ok()?;
    (out.status.success() && out.stdout.len()>100 && out.stdout.len()<4_000_000).then_some(out.stdout)
}
fn artwork_png(path:&Path)->Option<Vec<u8>> {
    let out=Command::new("ffmpeg").args(["-hide_banner","-loglevel","error","-i"])
        .arg(path).args(["-an","-map","0:v:0","-frames:v","1","-vf","scale=96:-2","-f","image2pipe","-vcodec","png","pipe:1"])
        .output().ok()?;
    (out.status.success()&&out.stdout.len()>100&&out.stdout.len()<1_500_000).then_some(out.stdout)
}
fn wave_samples(path:&Path)->Option<Vec<f32>> {
    let output=Command::new("ffmpeg").args(["-hide_banner","-loglevel","error","-i"])
        .arg(path).args(["-t","15","-vn","-ac","1","-ar","2000","-f","f32le","pipe:1"])
        .output().ok()?;
    if !output.status.success() || output.stdout.len()>500_000 {return None;}
    let samples:Vec<f32>=output.stdout.chunks_exact(4).map(|c|f32::from_le_bytes([c[0],c[1],c[2],c[3]])).collect();
    if samples.is_empty(){return None;}
    let width=125;
    Some((0..width).map(|i|{
        let a=i*samples.len()/width;let b=((i+1)*samples.len()/width).max(a+1);
        samples[a..b].iter().copied().map(f32::abs).fold(0.0_f32,f32::max).clamp(0.0,1.0)
    }).collect())
}
fn cpu_ticks()->Option<(u64,u64)> {
    let t=fs::read_to_string("/proc/stat").ok()?;
    let v:Vec<u64>=t.lines().next()?.split_whitespace().skip(1).filter_map(|s|s.parse().ok()).collect();
    if v.len()<5{return None;} Some((v.iter().sum(),v[3]+v[4]))
}
fn memory_used()->Option<(u64,u64)> {
    let t=fs::read_to_string("/proc/meminfo").ok()?;
    let mem=|k:&str|t.lines().find(|l|l.starts_with(k)).and_then(|l|l.split_whitespace().nth(1)).and_then(|s|s.parse::<u64>().ok());
    let total=mem("MemTotal:")?;let available=mem("MemAvailable:")?;
    Some((total.saturating_sub(available)/1024,total/1024))
}
impl App {
    fn refresh_stats(&mut self) {
        if let Some((total,idle))=cpu_ticks(){
            if let Some((before,old_idle))=self.cpu_prev {
                let n=total.saturating_sub(before);let i=idle.saturating_sub(old_idle);
                if n>0 {self.cpu=100.0*(n-i) as f32/n as f32;}
            } self.cpu_prev=Some((total,idle));
        }
        if let Some((used,total))=memory_used(){self.ram_used=used;self.ram_total=total;}
    }
    fn request_thumbnails(&self) {
        let root=self.reel_root.clone();let sender=self.tx.clone();
        let reels:Vec<_>=self.reels.iter().take(32).map(|r|(r.id.clone(),r.video.clone(),r.duration.unwrap_or(0.0))).collect();
        thread::spawn(move||{
            let Ok(base)=root.canonicalize() else{return};
            for (id,relative,duration) in reels {
                let Ok(path)=base.join(&relative).canonicalize() else{continue};
                if !path.starts_with(&base)||!path.is_file(){continue;}
                let t=if duration>0.0{(duration*0.17).max(0.1).min(2.0)}else{0.3};
                if let Some(bytes)=video_png(&path,t){let _=sender.send(WorkerMsg::Thumb(format!("reel:{id}"),bytes));}
                // Five genuine stills from different timestamps, never the same thumbnail repeated.
                for slot in 0..5 {
                    let t=if duration>0.0 {duration*((slot as f64+0.5)/5.0)}else{(slot as f64)*0.4};
                    if let Some(bytes)=video_png(&path,t){let _=sender.send(WorkerMsg::Thumb(format!("frame:{id}:{slot}"),bytes));}
                }
            }
        });
        let root=self.root.clone();let sender=self.tx.clone();
        let tracks:Vec<_>=self.lib.tracks.iter().take(28).map(|t|(t.id,t.original.clone())).collect();
        thread::spawn(move||{
            let Ok(base)=root.canonicalize() else{return};
            for (id,relative) in tracks {
                let Ok(path)=base.join(&relative).canonicalize() else{continue};
                if !path.starts_with(&base)||!path.is_file(){continue;}
                if let Some(bytes)=artwork_png(&path){let _=sender.send(WorkerMsg::Thumb(format!("audio:{id}"),bytes));}
            }
        });
    }
    fn open_video_external(&mut self) {
        let Some(id)=self.selected_reel.as_ref() else{return};
        let Some(r)=self.reels.iter().find(|r|&r.id==id) else{return};
        let Ok(base)=self.reel_root.canonicalize() else{return};
        let Ok(path)=base.join(&r.video).canonicalize() else{return};
        if !path.starts_with(&base)||!path.is_file(){self.notice="Reel file cannot be opened".into();return;}
        let played=Command::new("ffplay").args(["-autoexit","-loglevel","error"]).arg(&path).spawn().is_ok();
        if !played {self.notice="ffplay unavailable. Install ffmpeg/ffplay for external video playback.".into();}
    }
}
fn setup(s:&mut App,waker:Waker){
    if let Some(clip)=s.clip.as_ref(){clip.set_waker(waker.clone())} if let Some(audio)=s.audio.as_ref(){audio.set_waker(waker.clone())}
    let cache=ImageCache::new(waker);
    s.emblem=Some(cache.load(&include_bytes!("../assets/forge_emblem.png")[..]));
    s.anvil=Some(cache.load(&include_bytes!("../assets/forge_anvil.png")[..]));
    s.metal=Some(cache.load(&include_bytes!("../assets/forge_metal_header.png")[..]));
    s.images=Some(cache);
    s.refresh_stats();s.request_thumbnails();
}
fn poll(s:&mut App,_elapsed:f64,tx:&mut Tx)->bool {
    let changed=s.queue.poll();
    let mut update=changed;
    while let Ok(message)=s.rx.try_recv() {
        update=true;
        match message {
            WorkerMsg::Scan(r)=>{s.busy=false;match r.result {Ok(lib)=>{s.lib=lib;s.notice=format!("Audio library: {} tracks",s.lib.tracks.len());},Err(e)=>s.notice=e}},
            WorkerMsg::Reels(r)=>match r {Ok(rows)=>{s.reels=rows;s.notice=format!("Reels: {} items",s.reels.len());s.request_thumbnails();},Err(e)=>s.notice=e},
            WorkerMsg::Model(r)=>{s.model_busy=false;s.notice=r.unwrap_or_else(|e|e);s.models=model_rows();},
            WorkerMsg::Analysis(r)=>{s.analysis_busy=false;s.notice=r.unwrap_or_else(|e|e);},
            WorkerMsg::Thumb(id,bytes)=>{if let Some(cache)=s.images.as_ref(){s.thumbs.insert(id,cache.load(bytes));}},
            WorkerMsg::Wave(id,values)=>{if s.selected_track==Some(id){s.waveform=values;}},
        }
    }
    if update {s.recompute();}
    if s.last_stats.elapsed()>=Duration::from_secs(2){s.refresh_stats();s.last_stats=Instant::now();}
    tx.wake_after(Duration::from_millis(if s.clip.as_ref().is_some_and(|c|c.is_playing()) || s.audio.as_ref().is_some_and(|c|c.is_playing()){33}else if s.queue.has_active(){100}else{750}));
    true
}
fn label(t:impl Into<String>,p:Paint)->El{text(t).color(p.ink)}
fn dim(t:impl Into<String>,p:Paint)->El{text(t).color(p.muted)}
fn heading(t:impl Into<String>,p:Paint)->El{text(t).font(rune::tok::type_::TITLE).color(p.ink)}
fn rule(p:Paint)->El{el().h(1.0).w_full().fill(p.line)}
// Forge chrome is composed from vector polygons, not rectangular borders and triangular stickers.
// All decoration is drawn behind the active elements, so controls remain native Rune controls.
fn forge_shell(surface:Color,p:Paint,signal:bool)->Shape {
    let rim=if signal{p.red}else{p.brass};
    let edge=if signal{p.brass}else{p.red};
    Shape::new()
        .fill(vec![[0.,20.],[20.,0.],[976.,0.],[1000.,22.],[1000.,979.],[980.,1000.],[20.,1000.],[0.,982.]],rim)
        .fill(vec![[2.,21.],[21.,2.],[975.,2.],[998.,23.],[998.,978.],[979.,998.],[21.,998.],[2.,981.]],surface)
        .stroke(vec![[22.,4.],[193.,4.],[216.,12.],[391.,12.]],2.8,p.red)
        .stroke(vec![[22.,10.],[150.,10.]],0.65,p.brass)
        .stroke(vec![[743.,989.],[971.,989.],[990.,972.]],1.8,p.brass)
        .stroke(vec![[3.,180.],[3.,780.]],1.1,edge)
        .stroke(vec![[997.,115.],[997.,884.]],1.0,p.line)
        .fill(vec![[920.,995.],[934.,995.],[948.,976.],[936.,976.]],p.red)
        .fill(vec![[940.,995.],[954.,995.],[968.,976.],[956.,976.]],p.red)
}
fn frame_cap(p:Paint,right:bool)->El {
    let glyph=if right{
        Shape::new()
            .fill(vec![[0.,0.],[16.,0.],[26.,10.],[10.,10.]],p.brass)
            .stroke(vec![[1.,8.],[17.,8.],[23.,10.]],1.0,p.red)
    }else{
        Shape::new()
            .fill(vec![[10.,0.],[126.,0.],[116.,10.],[0.,10.]],p.red)
            .stroke(vec![[10.,8.],[86.,8.]],1.3,p.brass)
    };
    canvas(glyph).size(if right{26.0}else{126.0},10.0)
}
fn chip(t:impl Into<String>,p:Paint,active:bool)->El{
    let surface=if active{hex("#8D1120")}else{p.pane2};
    let rim=if active{p.red}else{p.line};
    let shape=Shape::new()
        .fill(vec![[0.,0.],[92.,0.],[100.,8.],[100.,24.],[0.,24.]],rim)
        .fill(vec![[1.,1.],[91.,1.],[99.,9.],[99.,23.],[1.,23.]],surface)
        .stroke(vec![[3.,2.],[30.,2.]],1.2,if active{p.brass}else{p.red_dim});
    row().h(25.0).gap(4.0).center_y().fill(Color::TRANSPARENT)
        .child(canvas(shape.view_box(100.,24.)).w_full().h_full().absolute(0.,0.))
        .child(el().w(8.0))
        .child(text(t).font(rune::tok::type_::LABEL).color(if active{p.ink}else{p.brass}))
        .child(el().w(10.0))
}
fn action(t:impl Into<String>,p:Paint,primary:bool,press:impl Fn(&mut App)+'static)->El {
    let base=if primary{hex("#AD101E")}else{hex("#190D10")};
    let rim=if primary{p.red}else{p.brass};
    let plate=Shape::new()
        .fill(vec![[0.,7.],[7.,0.],[91.,0.],[100.,9.],[100.,32.],[0.,32.]],rim)
        .fill(vec![[2.,8.],[8.,2.],[90.,2.],[98.,10.],[98.,30.],[2.,30.]],base)
        .fill(vec![[3.,4.],[8.,2.],[37.,2.],[32.,4.]],if primary{p.brass}else{p.red})
        .stroke(vec![[4.,29.],[87.,29.]],0.9,p.red_dim);
    row().h(33.0).min_w(50.0).gap(6.0).center_y().fill(Color::TRANSPARENT)
        .child(canvas(plate.view_box(100.,32.)).w_full().h_full().absolute(0.,0.))
        .press_scale(0.98).focusable(true).on_press(press)
        .child(el().w(10.0).h(14.0).child(el().w(2.0).h(12.0).fill(if primary{p.brass}else{p.red})))
        .child(text(t).font(rune::tok::type_::LABEL).color(p.ink))
        .child(el().w(12.0))
}
fn panel(title:&str, subtitle:&str,p:Paint,child:El)->El{
    let icon=match title{
        "MEDIA LIBRARY"=>"◇", "REEL STUDIO"=>"⬡", "MUSIC CATEGORIZER"=>"♫",
        "DOWNLOADER"=>"↓", "DOWNLOAD QUEUE"=>"↓", "AI MODEL CENTER"=>"⬡", _=>"◆"};
    let display=match title{
        "MEDIA LIBRARY"=>"Media Library", "REEL STUDIO"=>"Reel Studio",
        "MUSIC CATEGORIZER"=>"Music Categorizer", "DOWNLOADER"=>"Downloader",
        "DOWNLOAD QUEUE"=>"Download Queue", "AI MODEL CENTER"=>"AI Model Center",
        "FORGE SETTINGS"=>"Forge Settings", _=>title};
    let frame=forge_shell(p.pane,p,true).view_box(1000.,1000.);
    el().key(format!("panel-{title}")).grow(1.0).min_h(0.0).min_w(0.0).fill(p.bg)
        .child(canvas(frame).w_full().h_full().absolute(0.,0.))
        .child(col().w_full().h_full().padding(7.0).gap(0.0).min_h(0.0).min_w(0.0)
            .child(row().h(10.0).w_full().center_y().gap(0.0)
                .child(frame_cap(p,false))
                .child(el().grow(1.0).h(1.0).fill(p.line))
                .child(frame_cap(p,true)))
            .child(row().h(52.0).px(12.0).gap(11.0).center_y().fill(p.pane)
                .child(el().size(37.0,37.0).fill(Color::TRANSPARENT)
                    .child(canvas(Shape::new()
                        .fill(vec![[18.,0.],[36.,9.],[36.,28.],[18.,37.],[0.,28.],[0.,9.]],p.brass)
                        .fill(vec![[18.,2.],[34.,10.],[34.,27.],[18.,35.],[2.,27.],[2.,10.]],p.pane2)
                        .stroke(vec![[2.,27.],[18.,35.],[34.,27.]],1.0,p.red)).size(37.0,37.0).absolute(0.,0.))
                    .center().child(text(icon).font(rune::tok::type_::TITLE).color(p.brass)))
                .child(col().grow(1.0).gap(3.0)
                    .child(heading(display.to_string(),p))
                    .child(dim(subtitle.to_string(),p)))
                .child(canvas(Shape::new().stroke(vec![[0.,30.],[16.,14.],[20.,14.]],1.5,p.red)
                    .stroke(vec![[6.,34.],[23.,17.]],0.75,p.brass)).size(26.0,37.0)))
            .child(row().h(3.0).w_full().gap(0.0)
                .child(el().w(89.0).h(2.0).fill(p.red))
                .child(el().grow(1.0).h(1.0).fill(p.line))
                .child(el().w(32.0).h(2.0).fill(p.brass)))
            .child(child.grow(1.0).min_h(0.0).min_w(0.0))
            .child(row().h(6.0).w_full().gap(0.0)
                .child(el().w(48.0).h(2.0).fill(p.brass))
                .child(el().grow(1.0).h(1.0).fill(p.line))
                .child(el().w(82.0).h(2.0).fill(p.red))))
}
fn nav_glyph(page:Page,p:Paint)->El {
    let color=p.brass;
    let line=match page {
        Page::Workspace=>vec![[2.,10.],[10.,2.],[18.,10.],[15.,10.],[15.,18.],[5.,18.],[5.,10.],[2.,10.]],
        Page::Media=>vec![[2.,4.],[18.,4.],[18.,16.],[2.,16.],[2.,4.]],
        Page::Reels=>vec![[2.,4.],[18.,4.],[18.,16.],[2.,16.],[2.,4.]],
        Page::Music=>vec![[14.,2.],[14.,14.],[11.,17.],[7.,17.],[7.,14.],[14.,14.]],
        Page::Downloader=>vec![[10.,2.],[10.,13.],[5.,9.],[10.,14.],[15.,9.],[10.,14.],[10.,2.]],
        Page::Models=>vec![[10.,1.],[17.,5.],[17.,14.],[10.,19.],[3.,14.],[3.,5.],[10.,1.]],
        Page::Queue=>vec![[4.,4.],[17.,4.],[17.,5.],[4.,5.],[4.,10.],[17.,10.],[17.,11.],[4.,11.],[4.,16.],[17.,16.]],
        Page::Settings=>vec![[10.,1.],[17.,5.],[17.,15.],[10.,19.],[3.,15.],[3.,5.],[10.,1.]],
    };
    canvas(Shape::new().stroke(line,2.0,color)).size(20.0,20.0)
}
fn tab(t:&str,page:Page,s:&App,p:Paint)->El{
    let active=s.page==page;
    let surface=if active{hex("#8C0D1B")}else{p.side};
    let tab_shape=Shape::new()
        .fill(vec![[0.,0.],[176.,0.],[194.,21.],[176.,42.],[0.,42.]],surface)
        .stroke(if active{vec![[0.,0.],[176.,0.],[194.,21.],[176.,42.],[0.,42.]]}else{vec![[0.,41.],[176.,41.]]},
            if active{1.7}else{0.7},if active{p.red}else{p.line})
        .fill(vec![[0.,0.],[4.,0.],[4.,42.],[0.,42.]],if active{p.red}else{p.side})
        .stroke(vec![[7.,4.],[40.,4.]],if active{1.4}else{0.5},if active{p.brass}else{p.line});
    row().key(format!("nav-{}",page.name())).h(43.0).w_full().gap(10.0).center_y().fill(Color::TRANSPARENT)
        .child(canvas(tab_shape.view_box(194.,42.)).w_full().h_full().absolute(0.,0.))
        .focusable(true).on_press(move|state:&mut App|state.switch(page))
        .child(el().w(10.0))
        .child(nav_glyph(page,p))
        .child(text(t).color(if active{p.ink}else{p.muted}).grow(1.0))
        .child(el().w(23.0))
}
fn sidebar(s:&App,p:Paint)->El{
    let background=forge_shell(p.side,p,false).view_box(1000.,1000.);
    let mut menu=col().h_full().w_full().padding(8.0).gap(3.0).min_h(0.0)
        .child(col().h(83.0).px(10.0).gap(4.0).center_y()
            .child(row().gap(9.0).center_y()
                .child(if let Some(img)=s.emblem.as_ref(){el().size(49.0,49.0).pixels(img.pixels())}else{
                    el().size(49.0,49.0).fill(p.red_dim).center().child(text("G").color(p.brass))})
                .child(col().gap(4.0)
                    .child(text("GatherForge").font(rune::tok::type_::TITLE).color(p.ink))
                    .child(text("FORGE EDITION").font(rune::tok::type_::LABEL).color(p.red))))
            .child(text("FORGE ENGINE  /  MEDIA COMMAND").color(p.brass)))
        .child(row().h(5.0).w_full()
            .child(el().w(49.0).h(2.0).fill(p.red))
            .child(el().grow(1.0).h(1.0).fill(p.line)));
    let menus=[(Page::Workspace,"Workspace"),(Page::Media,"Media Library"),(Page::Reels,"Reel Studio"),
        (Page::Music,"Music Categorizer"),(Page::Downloader,"Downloader"),(Page::Models,"AI Model Center"),
        (Page::Queue,"Tasks & Queue"),(Page::Settings,"Settings")];
    for (page,title) in menus {menu=menu.child(tab(title,page,s,p));}
    menu=menu.child(el().grow(1.0));
    if let Some(img)=s.anvil.as_ref(){menu=menu.child(el().h(161.0).w_full().pixels(img.pixels()));}
    menu=menu.child(col().gap(5.0).padding(8.0).fill(p.pane2)
        .child(row().h(2.0).child(el().w(48.0).h(2.0).fill(p.red)).child(el().grow(1.0).h(1.0).fill(p.line)))
        .child(text("LOCAL-FIRST / PRIVATE").color(p.brass))
        .child(dim(format!("{} tracks  /  {} reels",s.lib.tracks.len(),s.reels.len()),p))
        .child(dim(format!("{} active operations",s.queue.active_count()),p)));
    el().key("sidebar").w(215.0).h_full().fill(p.bg)
        .child(canvas(background).w_full().h_full().absolute(0.,0.))
        .child(menu)
}
fn meter(caption:&str,value:String,ratio:f32,p:Paint)->El {
    col().gap(3.0).w(81.0)
        .child(text(format!("{caption} {value}")).font(rune::tok::type_::LABEL).color(p.ink))
        .child(el().h(5.0).w_full().fill(hex("#25151A"))
            .child(el().h(5.0).w((ratio*77.0).clamp(0.0,77.0)).fill(p.red)))
}
fn header(s:&App,p:Paint)->El{
    let k=kit(s.dark);
    let mut r=row().key("command-bar").w_full().h(76.0).fill(p.bg)
        .gap(12.0).center_y();
    if let Some(texture)=s.metal.as_ref(){
        r=r.child(el().w_full().h_full().pixels(texture.pixels()).absolute(0.,0.));
    }
    let silhouette=Shape::new()
        .stroke(vec![[0.,74.],[212.,74.],[238.,12.],[270.,2.],[810.,2.],[832.,12.]],1.4,p.red)
        .stroke(vec![[863.,4.],[1090.,4.],[1112.,30.],[1640.,30.]],1.0,p.brass)
        .stroke(vec![[230.,69.],[800.,69.]],2.5,p.red)
        .stroke(vec![[1090.,73.],[1610.,73.]],1.5,p.red);
    r=r.child(canvas(silhouette.view_box(1640.,76.)).w_full().h_full().absolute(0.,0.));
    let brand=row().w(271.0).h_full().center_y().gap(7.0).fill(Color::TRANSPARENT)
        .child(if let Some(emblem)=&s.emblem{el().w(74.0).h(68.0).pixels(emblem.pixels())}else{el().w(74.0).h(68.0)})
        .child(col().gap(4.0)
            .child(text("GatherForge").font(rune::tok::type_::TITLE).color(p.ink))
            .child(text("Forge Your Media").color(p.red)))
        .child(canvas(Shape::new().stroke(vec![[0.,72.],[26.,1.]],2.0,p.red)
            .stroke(vec![[3.,72.],[24.,12.]],0.9,p.brass)).size(27.0,72.0));
    r=r.child(brand);
    let input_plate=Shape::new()
        .fill(vec![[0.,0.],[95.,0.],[100.,10.],[100.,48.],[4.,48.],[0.,43.]],p.red)
        .fill(vec![[0.6,1.],[94.,1.],[99.,10.],[99.,46.],[4.5,46.],[1.,42.]],hex("#0C0608"))
        .stroke(vec![[6.,3.],[58.,3.]],1.2,p.brass);
    r=r.child(row().h(48.0).w(426.0).gap(8.0).center_y().fill(Color::TRANSPARENT)
        .child(canvas(input_plate.view_box(100.,48.)).w_full().h_full().absolute(0.,0.))
        .child(el().w(12.0))
        .child(canvas(Shape::new().stroke(vec![[3.,8.],[6.,5.],[9.,8.]],2.0,p.brass)).size(17.0,17.0))
        .child(TextInput::new(&s.search).placeholder("Search library, Reels, or jobs…  Ctrl+K").width(None).plain()
            .on_edit(|s:&mut App|&mut s.search).on_change(|s:&mut App,_|s.recompute()).build(k).grow(1.0))
        .child(el().w(20.0)));
    r=r.child(el().grow(1.0));
    r=r.child(meter("CPU",format!("{:.0}%",s.cpu),s.cpu/100.0,p));
    r=r.child(meter("GPU","—".into(),0.0,p));
    r=r.child(meter("RAM",if s.ram_total>0{format!("{:.1}G",s.ram_used as f32/1024.0)}else{"N/A".into()},
        if s.ram_total>0{s.ram_used as f32/s.ram_total as f32}else{0.0},p));
    r=r.child(chip(format!("{} ACTIVE",s.queue.active_count()),p,s.queue.has_active()));
    r=r.child(action("SETTINGS",p,false,|s|s.switch(Page::Settings)));
    r.child(action(if s.dark{"LIGHT"}else{"DARK"},p,false,|s|s.dark=!s.dark))
        .child(el().w(12.0))
}
fn status(s:&App,p:Paint)->El{
    row().key("status-bar").h(28.0).w_full().px(14.0).gap(14.0).center_y()
        .fill(hex("#090406")).border(1.0,p.brass)
        .child(text("◆  GATHERFORGE 3.10 • FORGE EDITION").color(p.brass))
        .child(chip(if s.busy{"WORKING"}else{"READY"},p,s.busy))
        .child(dim(format!("{} MEDIA  /  {} REELS",s.lib.tracks.len(),s.reels.len()),p))
        .child(dim(format!("{} QUEUED",s.queue.queued_count()),p))
        .child(el().grow(1.0))
        .child(text(s.notice.clone().chars().take(115).collect::<String>()).color(p.muted))
        .child(canvas(Shape::new()
            .fill(vec![[0.,18.],[10.,18.],[26.,0.],[16.,0.]],p.red)
            .fill(vec![[17.,18.],[27.,18.],[43.,0.],[33.,0.]],p.red)
            .fill(vec![[34.,18.],[44.,18.],[60.,0.],[50.,0.]],p.red)).size(65.0,18.0))
        .child(text("FORGE / BUILD / ORGANIZE").color(p.red))
}
fn thumb(s:&App,key:&str,p:Paint,w:f32,h:f32)->El {
    if let Some(img)=s.thumbs.get(key) {
        return el().size(w,h).fill(p.pane2).pixels(img.pixels()).border(1.0,p.brass);
    }
    // A geometric, non-pretend fallback for tracks without embedded cover artwork.
    let icon=if key.starts_with("reel:"){"▶"}else{"♫"};
    el().size(w,h).fill(hex("#220B0F"))
        .child(canvas(Shape::new()
            .fill(vec![[0.,6.],[6.,0.],[93.,0.],[100.,7.],[100.,93.],[93.,100.],[6.,100.],[0.,93.]],p.line)
            .fill(vec![[3.,7.],[7.,3.],[92.,3.],[97.,8.],[97.,92.],[92.,97.],[7.,97.],[3.,92.]],hex("#240E13"))
            .stroke(vec![[9.,11.],[57.,11.]],1.0,p.red)
            .view_box(100.,100.)).w_full().h_full().absolute(0.,0.))
        .center().child(text(icon).font(rune::tok::type_::TITLE).color(p.brass))
}
fn forge_table_style(p:Paint,k:&Kit)->TableStyle {
    let mut style=TableStyle::from_kit(k);
    style.row_height=34.0;
    style.header_height=33.0;
    style.cell_pad=9.0;
    style.rule=p.line;
    style.text=p.ink;
    style.muted=p.muted;
    style.header_text=p.brass;
    style.selected=p.red_dim;
    style.stripe=hex("#180C10");
    style
}
fn media_table(s:&App,p:Paint,small:bool)->El{
    let k=kit(s.dark);
    let table=Table::new(|s:&App|&s.table_media,|s:&mut App|&mut s.table_media,
        |s:&App|s.media_rows.len(),|s:&App,i| match s.media_rows[i] {
            MediaRef::Audio(t)=>Cow::Owned(format!("audio:{}",s.lib.tracks[t].id)),
            MediaRef::Reel(r)=>Cow::Owned(format!("reel:{}",s.reels[r].id)),
        })
        .name("Forge Combined Media Library")
        .column(TableColumn::new("art","Art").width(55.0).resizable(false))
        .column(TableColumn::new("title","Title").flex(3.0).min(130.0).sortable())
        .column(TableColumn::new("artist","Artist / Creator").flex(2.0).min(100.0).sortable())
        .column(TableColumn::new("format","Type").width(77.0).sortable())
        .column(TableColumn::new("review","Status").width(85.0).sortable())
        .cell(move|s:&App,i,col|match s.media_rows[i] {
            MediaRef::Audio(t)=>{let t=&s.lib.tracks[t];match col {
                "art"=>Cell::el(thumb(s,&format!("audio:{}",t.id),p,41.0,26.0)),
                "title"=>Cell::text(&t.title),"artist"=>Cell::text(&t.artist),
                "format"=>Cell::text(&t.format).muted(),_=>Cell::el(chip(t.action.label(),p,t.action==model::Decision::Review))}},
            MediaRef::Reel(r)=>{let r=&s.reels[r];match col {
                "art"=>Cell::el(thumb(s,&format!("reel:{}",r.id),p,41.0,26.0)),
                "title"=>Cell::text(if r.caption.is_empty(){r.id.clone()}else{r.caption.chars().take(75).collect()}),
                "artist"=>Cell::text(&r.creator),"format"=>Cell::text("VIDEO").muted(),
                _=>Cell::el(chip(if r.instagram_unsaved{"Unsaved"}else{"Local"},p,r.instagram_unsaved))}},
        })
        .on_open(|s:&mut App,id,_| {
            if let Some(t)=id.strip_prefix("audio:").and_then(|v|v.parse::<u64>().ok()) {
                s.switch(Page::Music);s.select_track(t);
            } else if let Some(name)=id.strip_prefix("reel:") {
                if let Some(i)=s.reels.iter().position(|r|r.id==name){s.switch(Page::Reels);s.choose_reel(i);}
            }
        })
        .on_sort(|s:&mut App,col,dir,_|{
            let tracks=&s.lib.tracks;let reels=&s.reels;
            s.media_rows.sort_by(|a,b|{
                let key=|r:&MediaRef|match r{
                    MediaRef::Audio(i)=>{let t=&tracks[*i];match col{"title"=>t.title.clone(),"artist"=>t.artist.clone(),"format"=>t.format.clone(),_=>t.action.label().into()}},
                    MediaRef::Reel(i)=>{let r=&reels[*i];match col{"title"=>r.caption.clone(),"artist"=>r.creator.clone(),"format"=>"VIDEO".into(),_=>"Local".into()}}
                };
                dir.apply(key(a).to_lowercase().cmp(&key(b).to_lowercase()))
            });
        })
        .style(forge_table_style(p,k)).zebra(true).build(s,k).grow(1.0).min_h(0.0);
    col().gap(8.0).padding(if small{7.0}else{12.0}).min_h(0.0).grow(1.0)
        .child(row().gap(6.0).center_y()
            .child(chip(format!("{} FILES",s.media_rows.len()),p,false))
            .child(action("ALL",p,s.filter==0,|s|{s.filter=0;s.recompute()}))
            .child(action("AUDIO",p,s.filter==1,|s|{s.filter=1;s.recompute()}))
            .child(action("VIDEO",p,s.filter==2,|s|{s.filter=2;s.recompute()}))
            .child(el().grow(1.0))
            .child(action("+ SCAN",p,true,|s|s.scan())))
        .child(table)
}
fn reel_row(reel:&instagram::Reel,index:usize,p:Paint,active:bool)->El{
    let title=if reel.creator.is_empty(){&reel.id}else{&reel.creator};
    row().key(format!("reel-{index}")).h(43.0).w_full().px(7.0).gap(7.0).center_y()
        .fill(if active{p.red_dim}else{p.pane2}).border(1.0,if active{p.red}else{p.line})
        .hover(p.red_dim).focusable(true).on_press(move|s:&mut App|s.choose_reel(index))
        .child(text("▣").color(p.brass)).child(col().gap(2.0).grow(1.0)
            .child(text(title.to_string()).color(p.ink))
            .child(text(format!("{} / {}",reel.collection,reel.id)).color(p.muted)))
}
fn reel_list(s:&App,p:Paint,limit:usize)->El{
    let needle=s.search.text.trim().to_lowercase();
    let mut out=col().gap(5.0).min_h(0.0);
    for (index,r) in s.reels.iter().enumerate().filter(|(_,r)| needle.is_empty()||format!("{} {} {}",r.id,r.creator,r.caption).to_lowercase().contains(&needle)).take(limit){
        out=out.child(reel_row(r,index,p,s.selected_reel.as_deref()==Some(&r.id)));
    }
    out.scroll(ScrollSpec { stick_bottom: false }).grow(1.0).min_h(0.0)
}
fn reel_evidence(s:&App,p:Paint)->El{
    let selected=s.selected_reel.as_ref().and_then(|id|s.reels.iter().find(|r|&r.id==id));
    let Some(r)=selected else{return col().center().grow(1.0).child(dim("Select a Reel",p))};
    let mut items=col().padding(8.0).gap(6.0)
        .child(row().gap(5.0).child(text("TRANSCRIPT").color(p.ink)).child(chip("LOCAL",p,false)))
        .child(rule(p));
    let path=s.reel_root.join(&r.video);
    let evidence=path.parent().map(|p|p.join("analysis.json"));
    let data=evidence.and_then(|path|fs::read(path).ok().filter(|b|b.len()<100_000).and_then(|b|serde_json::from_slice::<Value>(&b).ok()));
    if let Some(v)=data {
        // Analyzer schema stores transcript/OCR inside named stage objects, not at the root.
        for (key,title) in [("asr","SPEECH / ASR"),("ocr","SCREEN TEXT / OCR"),("vision","VISUAL")]{
            if let Some(stage)=v.get(key){
                let status=stage.get("state").and_then(Value::as_str).unwrap_or("unknown");
                let result=stage.get("text").and_then(Value::as_str).unwrap_or("");
                let reason=stage.get("reason").and_then(Value::as_str).unwrap_or("");
                items=items.child(text(format!("{title}  /  {}",status.to_uppercase())).font(rune::tok::type_::LABEL).color(p.brass));
                if !result.trim().is_empty(){items=items.child(text(result.chars().take(480).collect::<String>()).color(p.ink).wrap());}
                else if !reason.is_empty(){items=items.child(dim(reason.chars().take(180).collect::<String>(),p));}
                items=items.child(rule(p));
            }
        }
        for (key,title) in [("approved_category","APPROVED CATEGORY"),("category_suggested","SUGGESTED CATEGORY")]{
            if let Some(value)=v.get(key).and_then(Value::as_str).filter(|x|!x.trim().is_empty()){
                items=items.child(text(title).color(p.brass)).child(text(value.to_owned()).color(p.ink));
            }
        }
        if let Some(warnings)=v.get("review_warnings").and_then(Value::as_array){
            for warning in warnings.iter().filter_map(Value::as_str).take(3){
                items=items.child(text(format!("! {}",warning.chars().take(155).collect::<String>())).color(p.muted).wrap());
            }
        }
    }else{items=items.child(dim("No verified ASR/OCR analysis yet",p));}
    items=items.child(text("CAPTION").font(rune::tok::type_::LABEL).color(p.brass))
        .child(text(if r.caption.trim().is_empty(){"No caption in metadata".to_owned()}else{r.caption.chars().take(360).collect::<String>()}).color(p.ink).wrap());
    items.scroll(ScrollSpec { stick_bottom: false }).min_h(0.0).grow(1.0)
}
fn player(s:&App,p:Paint)->El{
    let mut player_col=col().gap(6.0).grow(1.0).min_h(0.0).min_w(0.0);
    let current=s.selected_reel.as_ref();
    let poster=current.and_then(|id|s.thumbs.get(&format!("reel:{id}")));
    if let Some(clip)=s.clip.as_ref(){
        let failure=match clip.state(){PlayerState::Failed(reason)=>Some(reason),_=>None};
        let show_still=failure.is_some() || matches!(clip.state(),PlayerState::Loading);
        let video=if !show_still {
            el().key("selected-video").w_full().grow(1.0).min_h(100.0).fill(p.bg).video(clip,rune::SurfaceFit::Fit)
        }else if let Some(img)=poster {
            el().key("video-still-fallback").w_full().grow(1.0).min_h(100.0).fill(p.bg).pixels(img.pixels())
        }else{
            col().w_full().grow(1.0).min_h(100.0).center().fill(hex("#070609"))
                .child(text("VIDEO DECODER UNAVAILABLE").color(p.red))
        };
        player_col=player_col.child(video);
        let status=match clip.state(){
            PlayerState::Playing=>"❚❚ PAUSE",PlayerState::Ready=>"▶ PLAY",PlayerState::Loading=>"LOADING",
            PlayerState::Failed(_)=>"▶ EXTERNAL",PlayerState::Ended=>"↻ REPLAY",_=>"▶ PLAY"};
        let failed=failure.is_some();
        let duration=clip.duration().unwrap_or(0.0);
        let progress=if duration>0.0{(clip.time()/duration).clamp(0.0,1.0) as f32}else{0.0};
        player_col=player_col.child(row().h(31.0).gap(6.0).center_y()
            .child(action(status,p,true,move|s|{
                if failed {s.open_video_external();}else if let Some(c)=s.clip.as_ref(){if c.is_playing(){c.pause()}else{c.play()}}
            }))
            .child(action("«",p,false,|s|if let Some(c)=s.clip.as_ref(){c.seek((c.time()-10.0).max(0.0))}))
            .child(action("»",p,false,|s|if let Some(c)=s.clip.as_ref(){c.seek(c.time()+10.0)}))
            .child(dim(format!("{:02}:{:02} / {:02}:{:02}",(clip.time() as u64)/60,(clip.time() as u64)%60,
                (duration as u64)/60,(duration as u64)%60),p))
            .child(el().grow(1.0).h(5.0).fill(p.pane2).border(1.0,p.line)
                .child(el().w(progress*125.0).h(4.0).fill(p.red))));
        if let Some(reason)=failure {player_col=player_col.child(text(format!("Decoder: {}",reason.chars().take(95).collect::<String>())).color(p.muted));}
    }else if let Some(img)=poster{
        player_col=player_col.child(el().key("preview-still").w_full().grow(1.0).min_h(100.0).pixels(img.pixels()))
            .child(action("OPEN VIDEO",p,true,|s|s.open_video_external()));
    }else {player_col=player_col.child(col().w_full().grow(1.0).min_h(100.0).fill(p.bg).border(1.0,p.line).center()
        .child(text("NO VIDEO PREVIEW").color(p.brass))
        .child(dim(s.video_error.clone().unwrap_or_else(||"Select a local Reel".into()),p)));}
    player_col
}
fn filmstrip(s:&App,p:Paint,small:bool)->El{
    let Some(id)=s.selected_reel.as_deref() else{return col().h(58.0).child(dim("No reel selected",p))};
    let mut strip=row().h(if small{55.0}else{74.0}).gap(5.0).fill(p.bg).px(5.0).center_y();
    let selected=s.reels.iter().find(|r|r.id==id);
    strip=strip.child(col().w(72.0).gap(3.0)
        .child(text("FILMSTRIP").font(rune::tok::type_::LABEL).color(p.brass))
        .child(dim(format!("{} frames",(0..5).filter(|i|s.thumbs.contains_key(&format!("frame:{id}:{i}"))).count()),p)));
    for slot in 0..5{
        let key=format!("frame:{id}:{slot}");let img=s.thumbs.get(&key);
        let duration=selected.and_then(|r|r.duration).unwrap_or(0.0);
        let at=if duration>0.0{duration*((slot as f64+0.5)/5.0)}else{(slot as f64)*0.4};
        let el=if let Some(img)=img{
            el().w(if small{69.0}else{96.0}).h(if small{44.0}else{60.0}).pixels(img.pixels()).border(1.0,p.brass)
        }else{el().w(if small{69.0}else{96.0}).h(if small{44.0}else{60.0}).fill(p.pane2)
            .border(1.0,p.line).center().child(text(format!("{:02}",slot+1)).color(p.brass))};
        strip=strip.child(el.key(format!("filmstrip-{slot}")).hover(p.red_dim).focusable(true)
            .on_press(move|s:&mut App|if let Some(clip)=s.clip.as_ref(){clip.seek(at)}));
    }
    strip.scroll_x().min_w(0.0)
}
fn reels_panel(s:&App,p:Paint,small:bool)->El{
    let mut out=col().padding(if small{7.0}else{11.0}).gap(7.0).min_h(0.0).grow(1.0);
    if small{
        out=out.child(row().h(31.0).gap(6.0)
            .child(chip(format!("{} LOCAL REELS",s.reels.len()),p,true))
            .child(action("OPEN STUDIO",p,true,|s|s.switch(Page::Reels)))
            .child(action("NEXT",p,false,|s|{
                if !s.reels.is_empty(){let i=s.selected_reel.as_ref().and_then(|id|s.reels.iter().position(|r|&r.id==id)).unwrap_or(0);s.choose_reel((i+1)%s.reels.len());}
            })));
        out=out.child(row().gap(8.0).grow(1.0).min_h(0.0)
            .child(player(s,p).grow(1.5))
            .child(col().w(180.0).grow(0.5).min_h(0.0) .fill(Color::TRANSPARENT)
                .child(canvas(forge_shell(p.pane2,p,false).view_box(1000.,1000.)).w_full().h_full().absolute(0.,0.))
                .child(reel_evidence(s,p))));
        out.child(filmstrip(s,p,true))
    }else{
        out=out.child(row().gap(6.0).h(32.0)
            .child(chip(format!("{} LOCAL REELS",s.reels.len()),p,true))
            .child(action("REFRESH",p,false,|s|s.refresh_reels()))
            .child(action("ANALYZE",p,true,|s|s.analyze_selected()))
            .child(action("CATEGORY",p,false,|s|s.category=(s.category+1)%REEL_CATEGORIES.len()))
            .child(action("RECORD",p,false,|s|s.reel_operation("record-category")))
            .child(action("APPROVE",p,true,|s|s.confirm=Some("approve_reel"))));
        out=out.child(row().gap(10.0).grow(1.0).min_h(0.0)
            .child(col().w(182.0).gap(5.0).min_h(0.0)
                .child(text("REEL INDEX").color(p.brass)).child(reel_list(s,p,100)))
            .child(player(s,p).grow(1.4))
            .child(col().w(260.0).grow(0.7).min_h(0.0) .fill(Color::TRANSPARENT)
                .child(canvas(forge_shell(p.pane2,p,false).view_box(1000.,1000.)).w_full().h_full().absolute(0.,0.))
                .child(reel_evidence(s,p))));
        out=out.child(filmstrip(s,p,false));
        out=out.child(dim(format!("CATEGORY: {}  •  Evidence must be reviewed before approval",REEL_CATEGORIES[s.category]),p));
        if s.confirm==Some("approve_reel"){
            out=out.child(row().h(37.0).gap(8.0).fill(p.red_dim).center_y()
                .child(label("Approve this Reel and organize its copy?",p))
                .child(action("CONFIRM",p,true,|s|s.commit_operation()))
                .child(action("CANCEL",p,false,|s|s.confirm=None)));
        }
        out
    }
}
fn music_table(s:&App,p:Paint)->El{
    let k=kit(s.dark);
    Table::new(|s:&App|&s.table_music,|s:&mut App|&mut s.table_music,
        |s:&App|s.music_rows.len(),|s:&App,i|Cow::Owned(s.lib.tracks[s.music_rows[i]].id.to_string()))
        .name("Music Categorizer")
        .column(TableColumn::new("art","Cover").width(53.0).resizable(false))
        .column(TableColumn::new("title","Title").flex(3.0).min(105.0).sortable())
        .column(TableColumn::new("artist","Artist").flex(2.0).min(75.0).sortable())
        .column(TableColumn::new("format","Type").width(65.0).sortable())
        .column(TableColumn::new("review","Decision").width(90.0).sortable())
        .cell(move|s:&App,i,col|{let t=&s.lib.tracks[s.music_rows[i]];match col {
            "art"=>Cell::el(thumb(s,&format!("audio:{}",t.id),p,39.0,25.0)),
            "title"=>Cell::text(&t.title),"artist"=>Cell::text(&t.artist),"format"=>Cell::text(&t.format).muted(),
            _=>Cell::el(chip(t.action.label(),p,t.action==model::Decision::Review))}})
        .on_select(|s:&mut App,id,_| {if let Ok(id)=id.parse(){s.select_track(id)}})
        .style(forge_table_style(p,k)).zebra(true).build(s,k).grow(1.0).min_h(0.0)
}
fn waveform(s:&App,p:Paint,compact:bool)->El {
    if s.waveform.is_empty(){return row().center_y().child(dim("Select music to generate waveform",p));}
    let width=if compact{254.0}else{480.0};let height=if compact{32.0}else{48.0};
    let mut shape=Shape::new();
    for (i,amp) in s.waveform.iter().enumerate(){
        let x=(i as f32+0.5)*width/s.waveform.len() as f32;
        let bar=3.0+amp.sqrt().min(1.0)*height*0.45;
        shape=shape.stroke(vec![[x,height/2.0-bar],[x,height/2.0+bar]],1.6,p.red);
    }
    canvas(shape).size(width,height)
}
fn music_player(s:&App,p:Paint,small:bool)->El {
    let current=s.selected_track.and_then(|id|s.lib.tracks.iter().find(|t|t.id==id));
    let status=if let Some(audio)=s.audio.as_ref(){if audio.is_playing(){"❚❚"}else{"▶"}}else{"▶"};
    let plate=forge_shell(hex("#10070A"),p,false).view_box(1000.,1000.);
    let mut bar=row().h(if small{52.0}else{69.0}).px(13.0).gap(11.0).center_y().fill(Color::TRANSPARENT)
        .child(canvas(plate).w_full().h_full().absolute(0.,0.))
        .child(action(status,p,true,|s|if let Some(audio)=s.audio.as_ref(){if audio.is_playing(){audio.pause()}else{audio.play()}}));
    if let Some(t)=current{bar=bar.child(thumb(s,&format!("audio:{}",t.id),p,46.0,46.0));}
    if !small {bar=bar.child(col().w(166.0).gap(3.0)
        .child(text("NOW PLAYING").font(rune::tok::type_::LABEL).color(p.brass))
        .child(label(current.map(|t|t.title.clone()).unwrap_or_else(||"No selected track".into()).chars().take(25).collect::<String>(),p))
        .child(dim(current.map(|t|t.artist.clone()).unwrap_or_default().chars().take(28).collect::<String>(),p)));}
    bar=bar.child(waveform(s,p,small).grow(1.0).min_w(70.0));
    if let Some(audio)=s.audio.as_ref(){bar=bar.child(dim(format!("{:02}:{:02}",(audio.time() as u64)/60,(audio.time() as u64)%60),p));}
    bar.child(el().w(10.0))
}
fn music_panel(s:&App,p:Paint,small:bool)->El{
    let cats=s.lib.folders();
    let mut groups=col().gap(2.0).padding(4.0).fill(hex("#13080C"))
        .child(row().h(28.0).px(7.0).center_y().child(text("ARTISTS / GROUPS").color(p.brass)));
    groups=groups.child(action(format!("ALL MUSIC   {}",s.lib.tracks.len()),p,true,|s|{s.search.text.clear();s.recompute()}));
    for (name,count) in cats.iter().take(if small{7}else{100}){
        let query=name.clone();
        let active=s.search.text.trim()==name.as_str();
        let plate=Shape::new()
            .fill(vec![[0.,0.],[95.,0.],[100.,50.],[95.,100.],[0.,100.]],if active{p.red_dim}else{p.pane2})
            .stroke(vec![[0.,0.],[95.,0.],[100.,50.],[95.,100.],[0.,100.]],if active{1.2}else{0.4},if active{p.red}else{p.line})
            .fill(vec![[0.,0.],[2.,0.],[2.,100.],[0.,100.]],if active{p.red}else{p.pane2});
        groups=groups.child(row().h(29.0).px(7.0).gap(5.0).center_y().fill(Color::TRANSPARENT)
            .child(canvas(plate.view_box(100.,100.)).w_full().h_full().absolute(0.,0.))
            .focusable(true).on_press(move|s:&mut App|{s.search.text=query.clone();s.recompute();})
            .child(text("♫").color(p.brass))
            .child(text(name.chars().take(19).collect::<String>()).color(if active{p.ink}else{p.muted}).grow(1.0))
            .child(chip(count.to_string(),p,false)));
    }
    let mut body=col().padding(if small{6.0}else{10.0}).gap(6.0).grow(1.0).min_h(0.0)
        .child(row().gap(7.0).center_y()
            .child(chip(format!("{} TRACKS",s.lib.tracks.len()),p,true))
            .child(chip(format!("{} DUPLICATES",s.lib.duplicates()),p,false))
            .child(el().grow(1.0))
            .child(action("SCAN",p,false,|s|s.scan())));
    body=body.child(row().gap(6.0).grow(1.0).min_h(0.0)
        .child(groups.w(if small{177.0}else{230.0}).scroll(ScrollSpec { stick_bottom: false }).min_h(0.0))
        .child(music_table(s,p)));
    body=body.child(music_player(s,p,small));
    if !small {
        body=body.child(row().gap(8.0).h(38.0).center_y()
            .child(action("KEEP",p,false,|s|s.mark(model::Decision::Keep)))
            .child(action("REVIEW",p,false,|s|s.mark(model::Decision::Review)))
            .child(action("SKIP",p,false,|s|s.mark(model::Decision::Skip)))
            .child(TextInput::new(&s.artist_edit).placeholder("Artist name").width(Some(180.0)).on_edit(|s:&mut App|&mut s.artist_edit).build(kit(s.dark)))
            .child(action("SAVE ARTIST",p,true,|s|s.rename())));
        body=body.child(row().gap(8.0).h(34.0)
            .child(action("STAGE",p,false,|s|s.confirm=Some("stage")))
            .child(action("PUBLISH",p,false,|s|s.confirm=Some("publish")))
            .child(action("RESTORE",p,false,|s|s.confirm=Some("restore")))
            .child(action("FINALIZE",p,false,|s|s.confirm=Some("finalize"))));
        if let Some(op)=s.confirm{body=body.child(row().gap(10.0).h(36.0).center_y().fill(p.red_dim)
            .child(label(format!("Confirm {op}? This changes files on disk."),p))
            .child(action("CONFIRM",p,true,|s|s.commit_operation()))
            .child(action("CANCEL",p,false,|s|s.confirm=None)));}
    }
    body
}
fn job_row(s:&App,p:Paint,index:usize)->El{
    let j=&s.queue.jobs()[index];let id=j.id;
    let progress=j.progress.unwrap_or(0.0).clamp(0.0,1.0);
    col().key(format!("job-{id}")).w_full().gap(5.0).padding(9.0).fill(Color::TRANSPARENT)
        .child(canvas(forge_shell(p.pane2,p,false).view_box(1000.,1000.)).w_full().h_full().absolute(0.,0.))
        .child(row().gap(8.0).center_y()
            .child(chip(j.kind.label(),p,false))
            .child(text(j.current_item.clone().chars().take(44).collect::<String>()).color(p.ink).grow(1.0))
            .child(chip(j.state.label(),p,j.state.is_pending()))
            .child(action(if j.state.can_retry(){"RETRY"}else{"STOP"},p,false,move|s|{
                if s.queue.jobs().iter().any(|j|j.id==id&&j.state.can_retry()){s.queue.retry(id)}else{s.queue.cancel(id)}s.recompute();})))
        .child(row().h(12.0).gap(8.0).center_y()
            .child(el().h(6.0).grow(1.0).fill(p.bg).border(1.0,p.line)
                .child(el().h(4.0).w(progress*230.0).fill(p.red)))
            .child(dim(format!("{:.0}%",progress*100.0),p))
            .child(dim(j.speed.clone(),p)))
        .child(dim(j.source.chars().take(90).collect::<String>(),p))
}
fn progress_ring(percent:f32,p:Paint)->El {
    let mut background=Shape::new();let mut foreground=Shape::new();
    let n=72;
    let points:Vec<[f32;2]>=(0..=n).map(|i|{
        let a=std::f32::consts::PI*2.0*(i as f32/n as f32)-std::f32::consts::FRAC_PI_2;
        [43.0+34.0*a.cos(),43.0+34.0*a.sin()]
    }).collect();
    background=background.stroke(points.clone(),7.0,p.line);
    foreground=foreground.stroke(points.into_iter().take((n as f32*percent.clamp(0.0,1.0)) as usize+1).collect::<Vec<_>>(),7.0,p.red);
    el().size(86.0,86.0).shape(background).child(canvas(foreground).size(86.0,86.0).absolute(0.0,0.0))
        .center().child(text(format!("{:.0}%",100.0*percent)).font(rune::tok::type_::TITLE).color(p.ink))
}
fn downloader_panel(s:&App,p:Paint,small:bool)->El{
    let mut list=col().gap(4.0);
    for &i in s.queue_rows.iter().take(if small{3}else{100}){list=list.child(job_row(s,p,i));}
    let queued=s.queue.queued_count();let active=s.queue.active_count();
    let done=s.queue.jobs().iter().filter(|j|j.state==jobs::State::Completed).count();
    let failed=s.queue.jobs().iter().filter(|j|j.state.can_retry()).count();
    let mut content=col().padding(if small{7.0}else{11.0}).gap(7.0).min_h(0.0).grow(1.0)
        .child(row().gap(6.0).center_y()
            .child(action(format!("QUEUE ({queued})"),p,s.queue_filter==1,|s|{s.queue_filter=1;s.recompute()}))
            .child(action(format!("ACTIVE ({active})"),p,s.queue_filter==0,|s|{s.queue_filter=0;s.recompute()}))
            .child(action(format!("DONE ({done})"),p,s.queue_filter==2,|s|{s.queue_filter=2;s.recompute()}))
            .child(action(format!("FAILED ({failed})"),p,s.queue_filter==3,|s|{s.queue_filter=3;s.recompute()})));
    content=content.child(list.scroll(ScrollSpec{stick_bottom:false}).grow(1.0).min_h(0.0));
    let selected=s.queue.jobs().iter().find(|j|j.state.is_pending()).or_else(||s.queue.jobs().last());
    let progress=selected.and_then(|j|j.progress).unwrap_or(0.0).clamp(0.0,1.0);
    let speed=selected.map(|j|j.speed.as_str()).filter(|s|!s.is_empty()).unwrap_or("No active throughput");
    content=content.child(row().gap(14.0).h(if small{86.0}else{110.0}).padding(8.0).fill(hex("#12080A")).border(1.0,p.line)
        .child(col().grow(1.0).gap(6.0)
            .child(text("DOWNLOAD STATUS").font(rune::tok::type_::LABEL).color(p.brass))
            .child(text(speed.to_owned()).font(rune::tok::type_::TITLE).color(p.ink))
            .child(dim(format!("{} completed / {} failed / {} queued",done,failed,queued),p))
            .child(dim(selected.map(|j|j.eta.clone()).unwrap_or_default(),p)))
        .child(progress_ring(progress,p)));
    if !small {
        content=content.child(row().gap(8.0).center_y().h(38.0)
            .child(TextInput::new(&s.url).placeholder("Paste HTTPS media URL").width(None)
                .on_edit(|s:&mut App|&mut s.url).build(kit(s.dark)).grow(1.0))
            .child(action(if s.source==0{"SOURCE: MUSIC"}else{"SOURCE: REEL"},p,false,|s|s.source=1-s.source))
            .child(action("+ ADD",p,true,|s|s.add_url())));
    }
    content
}
fn models_panel(s:&App,p:Paint)->El{
    let mut list=col().padding(12.0).gap(8.0)
        .child(dim("Project-local model catalog • install and verify pinned assets explicitly",p));
    for m in &s.models {
        let verify_id=m.id.clone();let install_id=m.id.clone();
        list=list.child(row().h(55.0).px(10.0).gap(12.0).center_y().fill(p.pane2).border(1.0,p.line)
            .child(text("⬡").color(p.brass))
            .child(col().grow(1.0).gap(4.0).child(label(m.name.clone(),p)).child(dim(m.role.clone(),p)))
            .child(dim(m.size.clone(),p)).child(chip(m.status.clone(),p,m.status=="On disk"))
            .child(action("VERIFY",p,false,move|s|s.model_operation(verify_id.clone(),"verify")))
            .child(action("INSTALL",p,true,move|s|s.model_operation(install_id.clone(),"install"))));
    }
    list.scroll(ScrollSpec { stick_bottom: false }).grow(1.0).min_h(0.0)
}
fn workspace(s:&App,p:Paint)->El{
    col().key("forge-workspace").gap(8.0).grow(1.0).min_h(0.0)
        .child(row().gap(8.0).grow(1.0).min_h(0.0)
            .child(panel("MEDIA LIBRARY","Organize and manage your indexed collection",p,media_table(s,p,true)).grow(1.07))
            .child(panel("REEL STUDIO","Preview and analyze local Reel evidence",p,reels_panel(s,p,true)).grow(1.0)))
        .child(row().gap(8.0).grow(1.0).min_h(0.0)
            .child(panel("MUSIC CATEGORIZER","Track decisions, duplicates and artist folders",p,music_panel(s,p,true)).grow(1.07))
            .child(panel("DOWNLOADER","Persistent queue, transfers and recent results",p,downloader_panel(s,p,true)).grow(1.0)))
}
fn settings_panel(s:&App,p:Paint)->El{
    col().padding(18.0).gap(15.0).child(heading("APPEARANCE",p))
        .child(action("TOGGLE LIGHT / DARK",p,true,|s|s.dark=!s.dark))
        .child(rule(p)).child(heading("LOCAL PATHS",p))
        .child(label(format!("Music: {}",s.root.display()),p))
        .child(label(format!("Reels: {}",s.reel_root.display()),p))
        .child(label("Models: <project>/models",p))
        .child(rule(p)).child(dim("Palette: forged black / crimson / brushed brass. Strict 2px radius. No glass blur.",p))
}
fn view(s:&App,_cx:&mut Cx<App>)->El {
    let p=paint(s.dark);
    let content=match s.page {
        Page::Workspace=>workspace(s,p),
        Page::Media=>panel("MEDIA LIBRARY","Indexed audio + original media",p,media_table(s,p,false)),
        Page::Reels=>panel("REEL STUDIO","Local preview • Caption • Recognition evidence",p,reels_panel(s,p,false)),
        Page::Music=>panel("MUSIC CATEGORIZER","Review, retag, stage and publish with confirmation",p,music_panel(s,p,false)),
        Page::Downloader|Page::Queue=>panel("DOWNLOAD QUEUE","Real yt-dlp jobs and persisted progress",p,downloader_panel(s,p,false)),
        Page::Models=>panel("AI MODEL CENTER","Local verified model inventory",p,models_panel(s,p)),
        Page::Settings=>panel("FORGE SETTINGS","Appearance and storage roots",p,settings_panel(s,p)),
    };
    rune_kit::root(
        col().key("forge-window").w_full().h_full().fill(p.bg)
            .child(header(s,p))
            .child(row().gap(6.0).padding(6.0).grow(1.0).min_h(0.0)
                .child(sidebar(s,p))
                .child(content.grow(1.0).min_h(0.0).min_w(0.0)))
            .child(status(s,p)),
        Vec::new())
}
fn main(){
    rune::run(AppSpec {setup:Some(setup),poll:Some(poll),..AppSpec::new("GatherForge — Forge Edition",(1640.0,930.0),init,view)});
}
