use std::collections::BTreeMap;

use crate::model::{new_id, EffectInstance, EffectKind, TitleStyle, TransitionKind};

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

pub const TRANSITIONS: &[(&str, TransitionKind)] = &[
    ("Crossfade", TransitionKind::Fade),
    ("Dissolve", TransitionKind::Dissolve),
    ("Wipe Left", TransitionKind::WipeLeft),
    ("Wipe Right", TransitionKind::WipeRight),
    ("Wipe Up", TransitionKind::WipeUp),
    ("Wipe Down", TransitionKind::WipeDown),
    ("Slide Left", TransitionKind::SlideLeft),
    ("Slide Right", TransitionKind::SlideRight),
    ("Circle Open", TransitionKind::CircleOpen),
    ("Circle Close", TransitionKind::CircleClose),
];

pub const EFFECTS: &[EffectDef] = &[
    EffectDef {
        kind: EffectKind::Brightness,
        name: "Brightness",
        description: "Lift or crush the image with a broadcast-safe EQ.",
        params: &[ParamDef {
            key: "brightness",
            label: "Brightness",
            min: -0.4,
            max: 0.4,
            default: 0.08,
            step: 0.01,
        }],
    },
    EffectDef {
        kind: EffectKind::Contrast,
        name: "Contrast",
        description: "Increase or soften tonal separation.",
        params: &[ParamDef {
            key: "contrast",
            label: "Contrast",
            min: 0.4,
            max: 2.0,
            default: 1.18,
            step: 0.01,
        }],
    },
    EffectDef {
        kind: EffectKind::Saturation,
        name: "Saturation",
        description: "Control colour intensity.",
        params: &[ParamDef {
            key: "saturation",
            label: "Saturation",
            min: 0.0,
            max: 2.0,
            default: 1.25,
            step: 0.01,
        }],
    },
    EffectDef {
        kind: EffectKind::HueRotate,
        name: "Hue Rotate",
        description: "Rotate all hues around the colour wheel.",
        params: &[ParamDef {
            key: "degrees",
            label: "Degrees",
            min: -180.0,
            max: 180.0,
            default: 18.0,
            step: 1.0,
        }],
    },
    EffectDef {
        kind: EffectKind::Monochrome,
        name: "Monochrome",
        description: "Blend from the original colour into black and white.",
        params: &[ParamDef {
            key: "amount",
            label: "Amount",
            min: 0.0,
            max: 1.0,
            default: 1.0,
            step: 0.01,
        }],
    },
    EffectDef {
        kind: EffectKind::Vignette,
        name: "Vignette",
        description: "Darken the frame corners.",
        params: &[ParamDef {
            key: "strength",
            label: "Strength",
            min: 0.1,
            max: 1.0,
            default: 0.55,
            step: 0.01,
        }],
    },
    EffectDef {
        kind: EffectKind::Sharpen,
        name: "Sharpen",
        description: "Crisp up edges with a small unsharp mask.",
        params: &[ParamDef {
            key: "amount",
            label: "Amount",
            min: 0.0,
            max: 2.0,
            default: 0.8,
            step: 0.01,
        }],
    },
    EffectDef {
        kind: EffectKind::Blur,
        name: "Soft Blur",
        description: "Gaussian blur for dreamy or privacy looks.",
        params: &[ParamDef {
            key: "sigma",
            label: "Sigma",
            min: 0.3,
            max: 24.0,
            default: 6.0,
            step: 0.1,
        }],
    },
    EffectDef {
        kind: EffectKind::PanZoom,
        name: "Pan & Zoom",
        description: "Animated Ken Burns camera move on stills or video.",
        params: &[
            ParamDef {
                key: "zoom",
                label: "Zoom",
                min: 1.0,
                max: 2.0,
                default: 1.22,
                step: 0.01,
            },
            ParamDef {
                key: "pan_x",
                label: "Pan X",
                min: -1.0,
                max: 1.0,
                default: 0.0,
                step: 0.01,
            },
            ParamDef {
                key: "pan_y",
                label: "Pan Y",
                min: -1.0,
                max: 1.0,
                default: 0.0,
                step: 0.01,
            },
        ],
    },
    EffectDef {
        kind: EffectKind::Temperature,
        name: "Temperature",
        description: "Cool down or warm up the picture.",
        params: &[ParamDef {
            key: "temperature",
            label: "Kelvin",
            min: 3000.0,
            max: 10000.0,
            default: 7400.0,
            step: 100.0,
        }],
    },
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
