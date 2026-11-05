use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const PROJECT_FORMAT: &str = "splice-project";
pub const PROJECT_VERSION: u32 = 1;
pub const PROJECT_EXTENSION: &str = "splice";
pub const FPS: u32 = 30;
pub const SAMPLE_RATE: u32 = 48_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub media: Vec<MediaAsset>,
    #[serde(default)]
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub sample_rate: u32,
    #[serde(default = "default_background")]
    pub background: String,
}

fn default_background() -> String {
    "#0d1016".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: FPS,
            sample_rate: SAMPLE_RATE,
            background: default_background(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Video,
    Image,
    Audio,
}

impl MediaKind {
    pub fn label(self) -> &'static str {
        match self {
            MediaKind::Video => "Video",
            MediaKind::Image => "Image",
            MediaKind::Audio => "Audio",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaAsset {
    pub id: String,
    pub name: String,
    /// Relative to the project file where possible, otherwise absolute.
    pub path: String,
    pub kind: MediaKind,
    /// Source duration in seconds. 0 for still images.
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub has_audio: bool,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub channels: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Video,
    Overlay,
    Audio,
}

impl TrackKind {
    pub fn label(self) -> &'static str {
        match self {
            TrackKind::Video => "Video",
            TrackKind::Overlay => "Overlay",
            TrackKind::Audio => "Audio",
        }
    }

    pub fn is_visual(self) -> bool {
        !matches!(self, TrackKind::Audio)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub kind: TrackKind,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub clips: Vec<Clip>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Clip {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub media_id: Option<String>,
    /// Timeline position in seconds.
    pub start: f64,
    #[serde(default)]
    pub in_point: f64,
    pub out_point: f64,
    #[serde(default)]
    pub effects: Vec<EffectInstance>,
    #[serde(default)]
    pub transition: Option<Transition>,
    #[serde(default)]
    pub audio: AudioParams,
    #[serde(default)]
    pub title: Option<Title>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioParams {
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default)]
    pub fade_in: f64,
    #[serde(default)]
    pub fade_out: f64,
    #[serde(default)]
    pub muted: bool,
}

fn default_volume() -> f32 {
    1.0
}

impl Default for AudioParams {
    fn default() -> Self {
        Self {
            volume: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            muted: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EffectInstance {
    pub id: String,
    pub kind: EffectKind,
    #[serde(default)]
    pub params: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    Brightness,
    Contrast,
    Saturation,
    HueRotate,
    Monochrome,
    Vignette,
    Sharpen,
    Blur,
    PanZoom,
    Temperature,
    Gamma,
    ColorBalance,
    Vibrance,
    HueSaturation,
    Colorize,
    Invert,
    EdgeDetect,
    FilmGrain,
    Posterize,
    Median,
    SmartBlur,
    BoxBlur,
    Bilateral,
    Deband,
    Denoise,
    Deblock,
    ChromaShift,
    RgbSplit,
    LensCorrection,
    Rotate,
    FlipHorizontal,
    FlipVertical,
    Levels,
    FilmCurve,
    HeatMap,
    Pixelate,
    ColorContrast,
    ColorCorrect,
}

impl EffectKind {
    /// Every effect this build can render, in catalog order.
    pub const ALL: &'static [EffectKind] = &[
        EffectKind::Brightness,
        EffectKind::Contrast,
        EffectKind::Saturation,
        EffectKind::HueRotate,
        EffectKind::Monochrome,
        EffectKind::Vignette,
        EffectKind::Sharpen,
        EffectKind::Blur,
        EffectKind::PanZoom,
        EffectKind::Temperature,
        EffectKind::Gamma,
        EffectKind::ColorBalance,
        EffectKind::Vibrance,
        EffectKind::HueSaturation,
        EffectKind::Colorize,
        EffectKind::Invert,
        EffectKind::EdgeDetect,
        EffectKind::FilmGrain,
        EffectKind::Posterize,
        EffectKind::Median,
        EffectKind::SmartBlur,
        EffectKind::BoxBlur,
        EffectKind::Bilateral,
        EffectKind::Deband,
        EffectKind::Denoise,
        EffectKind::Deblock,
        EffectKind::ChromaShift,
        EffectKind::RgbSplit,
        EffectKind::LensCorrection,
        EffectKind::Rotate,
        EffectKind::FlipHorizontal,
        EffectKind::FlipVertical,
        EffectKind::Levels,
        EffectKind::FilmCurve,
        EffectKind::HeatMap,
        EffectKind::Pixelate,
        EffectKind::ColorContrast,
        EffectKind::ColorCorrect,
    ];
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transition {
    pub kind: TransitionKind,
    pub duration: f64,
}

macro_rules! transition_kinds {
    ($($variant:ident => $xfade:literal,)*) => {
        #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
        #[serde(rename_all = "snake_case")]
        pub enum TransitionKind {
            $($variant,)*
        }

        impl TransitionKind {
            /// Every transition this build can render, in catalog order.
            pub const ALL: &'static [TransitionKind] = &[$(TransitionKind::$variant,)*];

            pub fn xfade_name(self) -> &'static str {
                match self {
                    $(TransitionKind::$variant => $xfade,)*
                }
            }
        }
    };
}

// The 58 transitions supported by ffmpeg's xfade filter (ffmpeg n9.0.2).
transition_kinds! {
    Fade => "fade",
    Dissolve => "dissolve",
    WipeLeft => "wipeleft",
    WipeRight => "wiperight",
    WipeUp => "wipeup",
    WipeDown => "wipedown",
    SlideLeft => "slideleft",
    SlideRight => "slideright",
    SlideUp => "slideup",
    SlideDown => "slidedown",
    CircleOpen => "circleopen",
    CircleClose => "circleclose",
    CircleCrop => "circlecrop",
    RectCrop => "rectcrop",
    Radial => "radial",
    Distance => "distance",
    Pixelize => "pixelize",
    FadeBlack => "fadeblack",
    FadeWhite => "fadewhite",
    FadeGrays => "fadegrays",
    FadeFast => "fadefast",
    FadeSlow => "fadeslow",
    SmoothLeft => "smoothleft",
    SmoothRight => "smoothright",
    SmoothUp => "smoothup",
    SmoothDown => "smoothdown",
    VertOpen => "vertopen",
    VertClose => "vertclose",
    HorzOpen => "horzopen",
    HorzClose => "horzclose",
    DiagTl => "diagtl",
    DiagTr => "diagtr",
    DiagBl => "diagbl",
    DiagBr => "diagbr",
    WipeTl => "wipetl",
    WipeTr => "wipetr",
    WipeBl => "wipebl",
    WipeBr => "wipebr",
    HlSlice => "hlslice",
    HrSlice => "hrslice",
    VuSlice => "vuslice",
    VdSlice => "vdslice",
    HlWind => "hlwind",
    HrWind => "hrwind",
    VuWind => "vuwind",
    VdWind => "vdwind",
    CoverLeft => "coverleft",
    CoverRight => "coverright",
    CoverUp => "coverup",
    CoverDown => "coverdown",
    RevealLeft => "revealleft",
    RevealRight => "revealright",
    RevealUp => "revealup",
    RevealDown => "revealdown",
    SqueezeH => "squeezeh",
    SqueezeV => "squeezev",
    ZoomIn => "zoomin",
    HBlur => "hblur",
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Title {
    pub text: String,
    #[serde(default)]
    pub subtitle: String,
    pub font: String,
    pub size: f32,
    pub color: String,
    pub background: String,
    #[serde(default)]
    pub background_opacity: f32,
    pub position_x: f32,
    pub position_y: f32,
    pub style: TitleStyle,
    #[serde(default)]
    pub bold: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TitleStyle {
    Fade,
    Rise,
    SlideLeft,
    Pop,
    BlurIn,
    Tracking,
    LowerThird,
    CreditRoll,
    SpinIn,
    Pulse,
}

pub fn new_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{nanos:x}{count:04x}")
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            format: PROJECT_FORMAT.to_string(),
            version: PROJECT_VERSION,
            name: name.into(),
            settings: Settings::default(),
            media: Vec::new(),
            tracks: vec![
                Track {
                    id: "track-video-1".to_string(),
                    name: "Video 1".to_string(),
                    kind: TrackKind::Video,
                    muted: false,
                    clips: Vec::new(),
                },
                Track {
                    id: "track-overlay-1".to_string(),
                    name: "Overlay".to_string(),
                    kind: TrackKind::Overlay,
                    muted: false,
                    clips: Vec::new(),
                },
                Track {
                    id: "track-audio-music".to_string(),
                    name: "Music".to_string(),
                    kind: TrackKind::Audio,
                    muted: false,
                    clips: Vec::new(),
                },
                Track {
                    id: "track-audio-voice".to_string(),
                    name: "Voiceover".to_string(),
                    kind: TrackKind::Audio,
                    muted: false,
                    clips: Vec::new(),
                },
            ],
        }
    }

    pub fn primary_video_track(&self) -> Option<&Track> {
        self.tracks
            .iter()
            .find(|track| track.kind == TrackKind::Video)
    }

    pub fn primary_video_track_mut(&mut self) -> Option<&mut Track> {
        self.tracks
            .iter_mut()
            .find(|track| track.kind == TrackKind::Video)
    }

    pub fn overlay_track_mut(&mut self) -> Option<&mut Track> {
        self.tracks
            .iter_mut()
            .find(|track| track.kind == TrackKind::Overlay)
    }

    pub fn first_audio_track_mut(&mut self) -> Option<&mut Track> {
        self.tracks
            .iter_mut()
            .find(|track| track.kind == TrackKind::Audio)
    }

    pub fn audio_track_mut(&mut self, id: &str) -> Option<&mut Track> {
        self.tracks
            .iter_mut()
            .find(|track| track.id == id && track.kind == TrackKind::Audio)
    }

    pub fn media_by_id(&self, id: &str) -> Option<&MediaAsset> {
        self.media.iter().find(|asset| asset.id == id)
    }

    pub fn track_by_id(&self, id: &str) -> Option<&Track> {
        self.tracks.iter().find(|track| track.id == id)
    }

    pub fn track_by_id_mut(&mut self, id: &str) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|track| track.id == id)
    }

    pub fn clip_by_id(&self, id: &str) -> Option<(&Track, &Clip)> {
        self.tracks.iter().find_map(|track| {
            track
                .clips
                .iter()
                .find(|clip| clip.id == id)
                .map(|clip| (track, clip))
        })
    }

    pub fn clip_by_id_mut(&mut self, id: &str) -> Option<(String, &mut Clip)> {
        for track in &mut self.tracks {
            if let Some(clip) = track.clips.iter_mut().find(|clip| clip.id == id) {
                return Some((track.id.clone(), clip));
            }
        }
        None
    }

    pub fn timeline_duration(&self) -> f64 {
        self.tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .map(|clip| clip.start + clip.duration())
            .fold(0.0_f64, f64::max)
    }

    pub fn primary_end(&self) -> f64 {
        self.primary_video_track()
            .map(|track| {
                track
                    .clips
                    .iter()
                    .map(|clip| clip.start + clip.duration())
                    .fold(0.0_f64, f64::max)
            })
            .unwrap_or(0.0)
    }

    pub fn missing_assets(&self, project_dir: &Path) -> Vec<&MediaAsset> {
        self.media
            .iter()
            .filter(|asset| !resolve_media_path(project_dir, asset).is_file())
            .collect()
    }

    pub fn is_media_online(&self, media_id: Option<&str>, project_dir: &Path) -> bool {
        let Some(media_id) = media_id else {
            return true; // title clips have no source media
        };
        self.media_by_id(media_id)
            .map(|asset| resolve_media_path(project_dir, asset).is_file())
            .unwrap_or(false)
    }

    pub fn title_clips(&self) -> Vec<&Clip> {
        let mut titles: Vec<&Clip> = self
            .tracks
            .iter()
            .flat_map(|track| track.clips.iter())
            .filter(|clip| clip.title.is_some())
            .collect();
        titles.sort_by(|a, b| a.start.total_cmp(&b.start));
        titles
    }

    pub fn add_title(&mut self, title: Title, start: f64, duration: f64) -> String {
        let id = new_id("title");
        let clip = Clip {
            id: id.clone(),
            name: title.text.lines().next().unwrap_or("Title").to_string(),
            media_id: None,
            start,
            in_point: 0.0,
            out_point: duration,
            effects: Vec::new(),
            transition: None,
            audio: AudioParams::default(),
            title: Some(title),
        };
        let track = self.overlay_track_mut().expect("overlay track");
        track.clips.push(clip);
        id
    }

    /// Recomputes sequential start positions on the primary video track while
    /// honoring overlap introduced by transitions.
    pub fn reflow_primary(&mut self) {
        let Some(track) = self.primary_video_track_mut() else {
            return;
        };
        track.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
        let mut cursor = 0.0_f64;
        for (index, clip) in track.clips.iter_mut().enumerate() {
            let overlap = clip
                .transition
                .as_ref()
                .map(|transition| transition.duration.max(0.0))
                .unwrap_or(0.0);
            if index == 0 {
                clip.start = 0.0;
            } else {
                let overlap = overlap.min(cursor * 0.5).min(clip.duration() * 0.5);
                clip.start = (cursor - overlap).max(0.0);
            }
            cursor = clip.start + clip.duration();
        }
    }

    pub fn unique_name(&self, base: &str) -> String {
        if !self.name.trim().is_empty() {
            base.to_string()
        } else {
            "Untitled".to_string()
        }
    }
}

impl Clip {
    pub fn duration(&self) -> f64 {
        (self.out_point - self.in_point).max(0.0)
    }

    pub fn is_title(&self) -> bool {
        self.title.is_some()
    }

    pub fn is_still(&self, project: &Project) -> bool {
        self.media_id
            .as_deref()
            .and_then(|id| project.media_by_id(id))
            .map(|asset| asset.kind == MediaKind::Image)
            .unwrap_or(false)
    }
}

impl Title {
    pub fn opening(project_name: &str) -> Self {
        Self {
            text: project_name.to_string(),
            subtitle: "A Splice production".to_string(),
            font: "DejaVu Sans".to_string(),
            size: 96.0,
            color: "#F5F2EA".to_string(),
            background: "#0D1016".to_string(),
            background_opacity: 0.0,
            position_x: 0.5,
            position_y: 0.44,
            style: TitleStyle::Fade,
            bold: true,
        }
    }

    pub fn credits() -> Self {
        Self {
            text: "THE END".to_string(),
            subtitle: "Edited with Splice".to_string(),
            font: "DejaVu Serif".to_string(),
            size: 78.0,
            color: "#F5F2EA".to_string(),
            background: "#0D1016".to_string(),
            background_opacity: 0.0,
            position_x: 0.5,
            position_y: 0.5,
            style: TitleStyle::CreditRoll,
            bold: false,
        }
    }
}

pub fn resolve_media_path(project_dir: &Path, asset: &MediaAsset) -> PathBuf {
    let stored = Path::new(&asset.path);
    if stored.is_absolute() {
        stored.to_path_buf()
    } else {
        project_dir.join(stored)
    }
}

/// Paths inside the project directory are stored relative. Anything outside
/// keeps its absolute path as a fallback.
pub fn stored_media_path(project_dir: &Path, absolute: &Path) -> String {
    if let Ok(relative) = absolute.strip_prefix(project_dir) {
        relative.to_string_lossy().replace('\\', "/")
    } else {
        absolute.to_string_lossy().replace('\\', "/")
    }
}

pub fn default_clip_for(asset: &MediaAsset, start: f64) -> Clip {
    let duration = match asset.kind {
        MediaKind::Image => 5.0,
        _ => asset.duration.max(0.25),
    };
    Clip {
        id: new_id("clip"),
        name: asset.name.clone(),
        media_id: Some(asset.id.clone()),
        start,
        in_point: 0.0,
        out_point: duration,
        effects: Vec::new(),
        transition: None,
        audio: AudioParams::default(),
        title: None,
    }
}
