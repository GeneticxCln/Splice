use std::collections::BTreeMap;

use crate::model::{
    EffectInstance, EffectKind, TitleMotionPreset, TitleStyle, TransitionKind, new_id,
};

#[derive(Debug, Clone, Copy)]
pub struct ParamDef {
    pub key: &'static str,
    pub label: &'static str,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub step: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct EffectDef {
    pub kind: EffectKind,
    pub name: &'static str,
    pub description: &'static str,
    pub params: &'static [ParamDef],
}

const fn p(
    key: &'static str,
    label: &'static str,
    min: f64,
    max: f64,
    default: f64,
    step: f64,
) -> ParamDef {
    ParamDef {
        key,
        label,
        min,
        max,
        default,
        step,
    }
}

/// Every transition exposed by ffmpeg's `xfade` filter in this build. The
/// project stores the snake_case enum value; `TransitionKind::xfade_name`
/// supplies the ffmpeg name.
pub const TRANSITIONS: &[(&str, TransitionKind)] = &[
    ("Crossfade", TransitionKind::Fade),
    ("Dissolve", TransitionKind::Dissolve),
    ("Wipe Left", TransitionKind::WipeLeft),
    ("Wipe Right", TransitionKind::WipeRight),
    ("Wipe Up", TransitionKind::WipeUp),
    ("Wipe Down", TransitionKind::WipeDown),
    ("Slide Left", TransitionKind::SlideLeft),
    ("Slide Right", TransitionKind::SlideRight),
    ("Slide Up", TransitionKind::SlideUp),
    ("Slide Down", TransitionKind::SlideDown),
    ("Circle Open", TransitionKind::CircleOpen),
    ("Circle Close", TransitionKind::CircleClose),
    ("Circle Crop", TransitionKind::CircleCrop),
    ("Rect Crop", TransitionKind::RectCrop),
    ("Radial", TransitionKind::Radial),
    ("Distance", TransitionKind::Distance),
    ("Pixelize", TransitionKind::Pixelize),
    ("Fade to Black", TransitionKind::FadeBlack),
    ("Fade to White", TransitionKind::FadeWhite),
    ("Fade to Gray", TransitionKind::FadeGrays),
    ("Fast Fade", TransitionKind::FadeFast),
    ("Slow Fade", TransitionKind::FadeSlow),
    ("Smooth Left", TransitionKind::SmoothLeft),
    ("Smooth Right", TransitionKind::SmoothRight),
    ("Smooth Up", TransitionKind::SmoothUp),
    ("Smooth Down", TransitionKind::SmoothDown),
    ("Vertical Open", TransitionKind::VertOpen),
    ("Vertical Close", TransitionKind::VertClose),
    ("Horizontal Open", TransitionKind::HorzOpen),
    ("Horizontal Close", TransitionKind::HorzClose),
    ("Diagonal TL", TransitionKind::DiagTl),
    ("Diagonal TR", TransitionKind::DiagTr),
    ("Diagonal BL", TransitionKind::DiagBl),
    ("Diagonal BR", TransitionKind::DiagBr),
    ("Wipe TL", TransitionKind::WipeTl),
    ("Wipe TR", TransitionKind::WipeTr),
    ("Wipe BL", TransitionKind::WipeBl),
    ("Wipe BR", TransitionKind::WipeBr),
    ("Slice HL", TransitionKind::HlSlice),
    ("Slice HR", TransitionKind::HrSlice),
    ("Slice VU", TransitionKind::VuSlice),
    ("Slice VD", TransitionKind::VdSlice),
    ("Wind HL", TransitionKind::HlWind),
    ("Wind HR", TransitionKind::HrWind),
    ("Wind VU", TransitionKind::VuWind),
    ("Wind VD", TransitionKind::VdWind),
    ("Cover Left", TransitionKind::CoverLeft),
    ("Cover Right", TransitionKind::CoverRight),
    ("Cover Up", TransitionKind::CoverUp),
    ("Cover Down", TransitionKind::CoverDown),
    ("Reveal Left", TransitionKind::RevealLeft),
    ("Reveal Right", TransitionKind::RevealRight),
    ("Reveal Up", TransitionKind::RevealUp),
    ("Reveal Down", TransitionKind::RevealDown),
    ("Squeeze H", TransitionKind::SqueezeH),
    ("Squeeze V", TransitionKind::SqueezeV),
    ("Zoom In", TransitionKind::ZoomIn),
    ("H Blur", TransitionKind::HBlur),
];

pub const EFFECTS: &[EffectDef] = &[
    EffectDef {
        kind: EffectKind::Brightness,
        name: "Brightness",
        description: "Lift or crush the image with a broadcast-safe EQ.",
        params: &[p("brightness", "Brightness", -0.4, 0.4, 0.08, 0.01)],
    },
    EffectDef {
        kind: EffectKind::Contrast,
        name: "Contrast",
        description: "Increase or soften tonal separation.",
        params: &[p("contrast", "Contrast", 0.4, 2.0, 1.18, 0.01)],
    },
    EffectDef {
        kind: EffectKind::Saturation,
        name: "Saturation",
        description: "Control colour intensity.",
        params: &[p("saturation", "Saturation", 0.0, 2.0, 1.25, 0.01)],
    },
    EffectDef {
        kind: EffectKind::HueRotate,
        name: "Hue Rotate",
        description: "Rotate all hues around the colour wheel.",
        params: &[p("degrees", "Degrees", -180.0, 180.0, 18.0, 1.0)],
    },
    EffectDef {
        kind: EffectKind::Monochrome,
        name: "Monochrome",
        description: "Blend from the original colour into black and white.",
        params: &[p("amount", "Amount", 0.0, 1.0, 1.0, 0.01)],
    },
    EffectDef {
        kind: EffectKind::Vignette,
        name: "Vignette",
        description: "Darken the frame corners.",
        params: &[p("strength", "Strength", 0.1, 1.0, 0.55, 0.01)],
    },
    EffectDef {
        kind: EffectKind::Sharpen,
        name: "Sharpen",
        description: "Crisp up edges with a small unsharp mask.",
        params: &[p("amount", "Amount", 0.0, 2.0, 0.8, 0.01)],
    },
    EffectDef {
        kind: EffectKind::Blur,
        name: "Soft Blur",
        description: "Gaussian blur for dreamy or privacy looks.",
        params: &[p("sigma", "Sigma", 0.3, 24.0, 6.0, 0.1)],
    },
    EffectDef {
        kind: EffectKind::PanZoom,
        name: "Pan & Zoom",
        description: "Animated Ken Burns camera move on stills or video.",
        params: &[
            p("zoom", "Zoom", 1.0, 2.0, 1.22, 0.01),
            p("pan_x", "Pan X", -1.0, 1.0, 0.0, 0.01),
            p("pan_y", "Pan Y", -1.0, 1.0, 0.0, 0.01),
        ],
    },
    EffectDef {
        kind: EffectKind::Temperature,
        name: "Temperature",
        description: "Cool down or warm up the picture.",
        params: &[p("temperature", "Kelvin", 3000.0, 10000.0, 7400.0, 100.0)],
    },
    EffectDef {
        kind: EffectKind::Gamma,
        name: "Gamma",
        description: "Adjust midtone lift without moving black or white.",
        params: &[p("gamma", "Gamma", 0.1, 4.0, 1.1, 0.01)],
    },
    EffectDef {
        kind: EffectKind::ColorBalance,
        name: "Color Balance",
        description: "Push shadows, midtones and highlights toward a primary.",
        params: &[
            p("red", "Red", -0.5, 0.5, 0.0, 0.01),
            p("green", "Green", -0.5, 0.5, 0.0, 0.01),
            p("blue", "Blue", -0.5, 0.5, 0.0, 0.01),
        ],
    },
    EffectDef {
        kind: EffectKind::Vibrance,
        name: "Vibrance",
        description: "Boost muted colours while protecting skin tones.",
        params: &[p("intensity", "Intensity", -1.0, 1.0, 0.35, 0.01)],
    },
    EffectDef {
        kind: EffectKind::HueSaturation,
        name: "Hue & Saturation",
        description: "Shift hue, saturation and intensity together.",
        params: &[
            p("hue", "Hue", -180.0, 180.0, 0.0, 1.0),
            p("saturation", "Saturation", -1.0, 1.0, 0.15, 0.01),
            p("intensity", "Intensity", -1.0, 1.0, 0.0, 0.01),
        ],
    },
    EffectDef {
        kind: EffectKind::Colorize,
        name: "Colorize",
        description: "Wash the image in a selectable colour tint.",
        params: &[
            p("hue", "Hue", 0.0, 360.0, 210.0, 1.0),
            p("saturation", "Saturation", 0.0, 1.0, 0.35, 0.01),
            p("lightness", "Lightness", 0.0, 1.0, 0.5, 0.01),
            p("mix", "Mix", 0.0, 1.0, 0.6, 0.01),
        ],
    },
    EffectDef {
        kind: EffectKind::Invert,
        name: "Invert",
        description: "Negative image with a blendable amount.",
        params: &[p("amount", "Amount", 0.0, 1.0, 1.0, 0.01)],
    },
    EffectDef {
        kind: EffectKind::EdgeDetect,
        name: "Edge Detect",
        description: "Draw the frame as white edge wires on black.",
        params: &[
            p("low", "Low", 0.0, 1.0, 0.08, 0.01),
            p("high", "High", 0.0, 1.0, 0.2, 0.01),
        ],
    },
    EffectDef {
        kind: EffectKind::FilmGrain,
        name: "Film Grain",
        description: "Add temporal, uniform grain over the picture.",
        params: &[p("strength", "Strength", 0.0, 100.0, 12.0, 1.0)],
    },
    EffectDef {
        kind: EffectKind::Posterize,
        name: "Posterize",
        description: "Reduce the palette to flat colour bands.",
        params: &[p("levels", "Levels", 2.0, 64.0, 8.0, 1.0)],
    },
    EffectDef {
        kind: EffectKind::Median,
        name: "Median",
        description: "Smooth noise while keeping hard edges.",
        params: &[p("radius", "Radius", 1.0, 9.0, 2.0, 1.0)],
    },
    EffectDef {
        kind: EffectKind::SmartBlur,
        name: "Smart Blur",
        description: "Blur flat areas but protect detected outlines.",
        params: &[
            p("radius", "Radius", 0.1, 5.0, 1.5, 0.1),
            p("strength", "Strength", -1.0, 1.0, 0.6, 0.01),
        ],
    },
    EffectDef {
        kind: EffectKind::BoxBlur,
        name: "Box Blur",
        description: "Fast, repeated box blur for a soft frame.",
        params: &[
            p("radius", "Radius", 1.0, 20.0, 3.0, 1.0),
            p("power", "Passes", 1.0, 4.0, 2.0, 1.0),
        ],
    },
    EffectDef {
        kind: EffectKind::Bilateral,
        name: "Bilateral",
        description: "Edge-preserving smoothing for skin and flat areas.",
        params: &[
            p("space", "Spatial", 1.0, 50.0, 8.0, 1.0),
            p("range", "Range", 0.01, 1.0, 0.1, 0.01),
        ],
    },
    EffectDef {
        kind: EffectKind::Deband,
        name: "Deband",
        description: "Break up colour banding in gradients and skies.",
        params: &[
            p("threshold", "Threshold", 0.0001, 0.2, 0.02, 0.0001),
            p("range", "Range", 4.0, 64.0, 16.0, 1.0),
        ],
    },
    EffectDef {
        kind: EffectKind::Denoise,
        name: "Denoise",
        description: "High-quality spatial and temporal noise reduction.",
        params: &[
            p("spatial", "Spatial", 0.0, 10.0, 2.0, 0.1),
            p("temporal", "Temporal", 0.0, 10.0, 1.0, 0.1),
        ],
    },
    EffectDef {
        kind: EffectKind::Deblock,
        name: "Deblock",
        description: "Soften compression blocks with adjustable thresholds.",
        params: &[
            p("block", "Block", 4.0, 16.0, 8.0, 1.0),
            p("strength", "Strength", 0.0, 1.0, 0.2, 0.01),
        ],
    },
    EffectDef {
        kind: EffectKind::ChromaShift,
        name: "Chroma Shift",
        description: "Offset the chroma planes for a misregistered look.",
        params: &[
            p("horizontal", "Horizontal", -30.0, 30.0, 4.0, 1.0),
            p("vertical", "Vertical", -30.0, 30.0, 0.0, 1.0),
        ],
    },
    EffectDef {
        kind: EffectKind::RgbSplit,
        name: "RGB Split",
        description: "Separate red and blue channels for chromatic fringing.",
        params: &[p("offset", "Offset", 0.0, 30.0, 4.0, 1.0)],
    },
    EffectDef {
        kind: EffectKind::LensCorrection,
        name: "Lens Correction",
        description: "Correct or exaggerate barrel and pincushion distortion.",
        params: &[
            p("k1", "K1", -0.5, 0.5, -0.12, 0.01),
            p("k2", "K2", -0.5, 0.5, 0.0, 0.01),
        ],
    },
    EffectDef {
        kind: EffectKind::Rotate,
        name: "Rotate",
        description: "Rotate the frame around its centre over black.",
        params: &[p("angle", "Angle", -45.0, 45.0, 5.0, 0.5)],
    },
    EffectDef {
        kind: EffectKind::FlipHorizontal,
        name: "Flip Horizontal",
        description: "Mirror the frame left to right.",
        params: &[],
    },
    EffectDef {
        kind: EffectKind::FlipVertical,
        name: "Flip Vertical",
        description: "Mirror the frame top to bottom.",
        params: &[],
    },
    EffectDef {
        kind: EffectKind::Levels,
        name: "Levels",
        description: "Remap black and white points across all channels.",
        params: &[
            p("black", "Black point", 0.0, 0.4, 0.02, 0.01),
            p("white", "White point", 0.6, 1.0, 0.98, 0.01),
        ],
    },
    EffectDef {
        kind: EffectKind::FilmCurve,
        name: "Film Curve",
        description: "One of ffmpeg's 11 colour-curve looks (0 none … 10 vintage).",
        params: &[p("preset", "Preset", 0.0, 10.0, 10.0, 1.0)],
    },
    EffectDef {
        kind: EffectKind::HeatMap,
        name: "Heat Map",
        description: "Map luminance to a perceptual colour ramp (0 magma … 20).",
        params: &[p("preset", "Ramp", 0.0, 20.0, 4.0, 1.0)],
    },
    EffectDef {
        kind: EffectKind::Pixelate,
        name: "Pixelate",
        description: "Mosaic the frame into square blocks.",
        params: &[p("size", "Block", 1.0, 64.0, 12.0, 1.0)],
    },
    EffectDef {
        kind: EffectKind::ColorContrast,
        name: "Color Contrast",
        description: "Push colour separation between opposing primaries.",
        params: &[
            p("red_cyan", "Red / Cyan", -1.0, 1.0, 0.1, 0.01),
            p("green_magenta", "Green / Magenta", -1.0, 1.0, 0.0, 0.01),
            p("blue_yellow", "Blue / Yellow", -1.0, 1.0, 0.0, 0.01),
        ],
    },
    EffectDef {
        kind: EffectKind::ColorCorrect,
        name: "Color Correct",
        description: "Balance shadows and highlights with a saturation trim.",
        params: &[
            p("shadows_red", "Shadows R", -1.0, 1.0, 0.0, 0.01),
            p("shadows_blue", "Shadows B", -1.0, 1.0, 0.0, 0.01),
            p("highlights_red", "Highlights R", -1.0, 1.0, 0.0, 0.01),
            p("highlights_blue", "Highlights B", -1.0, 1.0, 0.0, 0.01),
            p("saturation", "Saturation", -3.0, 3.0, 1.0, 0.05),
        ],
    },
];

/// Effect-picker groupings. Every `EffectKind` must appear exactly once.
pub const EFFECT_CATEGORIES: &[(&str, &[EffectKind])] = &[
    (
        "Tone & Colour",
        &[
            EffectKind::Brightness,
            EffectKind::Contrast,
            EffectKind::Saturation,
            EffectKind::HueRotate,
            EffectKind::Monochrome,
            EffectKind::Temperature,
            EffectKind::Gamma,
            EffectKind::ColorBalance,
            EffectKind::Vibrance,
            EffectKind::HueSaturation,
            EffectKind::Colorize,
            EffectKind::Invert,
            EffectKind::Levels,
            EffectKind::FilmCurve,
            EffectKind::HeatMap,
            EffectKind::ColorContrast,
            EffectKind::ColorCorrect,
        ],
    ),
    (
        "Blur & Detail",
        &[
            EffectKind::Sharpen,
            EffectKind::Blur,
            EffectKind::Median,
            EffectKind::SmartBlur,
            EffectKind::BoxBlur,
            EffectKind::Bilateral,
            EffectKind::Deband,
            EffectKind::Denoise,
            EffectKind::Deblock,
        ],
    ),
    (
        "Stylize",
        &[
            EffectKind::Vignette,
            EffectKind::EdgeDetect,
            EffectKind::FilmGrain,
            EffectKind::Posterize,
            EffectKind::Pixelate,
        ],
    ),
    (
        "Distortion",
        &[
            EffectKind::ChromaShift,
            EffectKind::RgbSplit,
            EffectKind::LensCorrection,
            EffectKind::Rotate,
            EffectKind::FlipHorizontal,
            EffectKind::FlipVertical,
        ],
    ),
    ("Motion", &[EffectKind::PanZoom]),
];

pub const TITLE_STYLES: &[(&str, TitleStyle)] = &[
    ("Fade", TitleStyle::Fade),
    ("Rise", TitleStyle::Rise),
    ("Slide Left", TitleStyle::SlideLeft),
    ("Pop", TitleStyle::Pop),
    ("Blur In", TitleStyle::BlurIn),
    ("Tracking", TitleStyle::Tracking),
    ("Lower Third", TitleStyle::LowerThird),
    ("Credit Roll", TitleStyle::CreditRoll),
    ("Spin In", TitleStyle::SpinIn),
    ("Pulse", TitleStyle::Pulse),
];

pub const TITLE_MOTIONS: &[(&str, TitleMotionPreset)] = &[
    ("Fade", TitleMotionPreset::Fade),
    ("Rise", TitleMotionPreset::Rise),
    ("Slide", TitleMotionPreset::Slide),
    ("Pop", TitleMotionPreset::Pop),
    ("Blur", TitleMotionPreset::Blur),
    ("Tracking", TitleMotionPreset::Tracking),
    ("Typewriter", TitleMotionPreset::Typewriter),
];

pub const FONTS: &[&str] = &[
    "DejaVu Sans",
    "DejaVu Serif",
    "DejaVu Sans Mono",
    "Liberation Sans",
    "Liberation Serif",
    "Liberation Mono",
    "Noto Sans",
    "Noto Serif",
];

pub const PALETTE: &[&str] = &[
    "#F5F2EA", "#FFFFFF", "#FFB15C", "#FF6B4A", "#F45B69", "#B78CFF", "#5CC8FF", "#5EE2B5",
    "#101318", "#000000", "#2A3340", "#7D8A9A",
];

pub fn default_transition_duration() -> f64 {
    0.6
}

pub fn make_effect(kind: EffectKind) -> Option<EffectInstance> {
    let def = EFFECTS.iter().find(|def| def.kind == kind)?;
    let mut params = BTreeMap::new();
    for param in def.params {
        params.insert(param.key.to_string(), param.default);
    }
    Some(EffectInstance {
        id: new_id("fx"),
        kind,
        params,
    })
}

pub fn effect_def(kind: EffectKind) -> Option<&'static EffectDef> {
    EFFECTS.iter().find(|def| def.kind == kind)
}

pub fn effect_name(kind: EffectKind) -> &'static str {
    effect_def(kind).map(|def| def.name).unwrap_or("Effect")
}

pub fn transition_name(kind: TransitionKind) -> &'static str {
    TRANSITIONS
        .iter()
        .find(|(_, candidate)| *candidate == kind)
        .map(|(name, _)| *name)
        .unwrap_or("Transition")
}

pub fn title_style_name(style: TitleStyle) -> &'static str {
    TITLE_STYLES
        .iter()
        .find(|(_, candidate)| *candidate == style)
        .map(|(name, _)| *name)
        .unwrap_or("Style")
}

pub fn title_style_description(style: TitleStyle) -> &'static str {
    match style {
        TitleStyle::Fade => "Slow opacity fade with a soft hold.",
        TitleStyle::Rise => "Text rises into place from below.",
        TitleStyle::SlideLeft => "A clean horizontal entrance.",
        TitleStyle::Pop => "Scale pop with a short settle.",
        TitleStyle::BlurIn => "Defocus that resolves into sharp type.",
        TitleStyle::Tracking => "Letter spacing tightens as it fades in.",
        TitleStyle::LowerThird => "Broadcast lower-third slide.",
        TitleStyle::CreditRoll => "Classic end-credit vertical roll.",
        TitleStyle::SpinIn => "Small rotation that settles upright.",
        TitleStyle::Pulse => "A gentle repeating scale pulse.",
    }
}

pub fn title_motion_name(preset: TitleMotionPreset) -> &'static str {
    TITLE_MOTIONS
        .iter()
        .find(|(_, candidate)| *candidate == preset)
        .map(|(name, _)| *name)
        .unwrap_or("Motion")
}

pub fn title_motion_description(preset: TitleMotionPreset) -> &'static str {
    match preset {
        TitleMotionPreset::Fade => "A clean opacity fade in and out.",
        TitleMotionPreset::Rise => "Text rises into place from below.",
        TitleMotionPreset::Slide => "Text slides in from the chosen direction.",
        TitleMotionPreset::Pop => "A short scale pop with a settle.",
        TitleMotionPreset::Blur => "Defocus that resolves into sharp type.",
        TitleMotionPreset::Tracking => "Letter spacing tightens as it fades in.",
        TitleMotionPreset::Typewriter => "Characters appear one by one, left to right.",
    }
}
