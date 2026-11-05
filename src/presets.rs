use clap::ValueEnum;

use crate::model::Settings;

/// Export presets are deliberately locked. The renderer always writes H.264
/// video at 30 fps and AAC stereo at 48 kHz, with exact encoder settings
/// documented in docs/EXPORT_PRESETS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ExportPreset {
    #[value(name = "1080p")]
    P1080p,
    #[value(name = "720p")]
    P720p,
    #[value(name = "480p")]
    P480p,
    #[value(name = "shorts-1080p")]
    PShorts1080p,
    #[value(name = "shorts-720p")]
    PShorts720p,
}

impl ExportPreset {
    pub const ALL: [ExportPreset; 5] = [
        ExportPreset::P1080p,
        ExportPreset::P720p,
        ExportPreset::P480p,
        ExportPreset::PShorts1080p,
        ExportPreset::PShorts720p,
    ];

    pub fn width(self) -> u32 {
        match self {
            ExportPreset::P1080p => 1920,
            ExportPreset::P720p => 1280,
            ExportPreset::P480p => 854,
            ExportPreset::PShorts1080p => 1080,
            ExportPreset::PShorts720p => 720,
        }
    }

    pub fn height(self) -> u32 {
        match self {
            ExportPreset::P1080p => 1080,
            ExportPreset::P720p => 720,
            ExportPreset::P480p => 480,
            ExportPreset::PShorts1080p => 1920,
            ExportPreset::PShorts720p => 1280,
        }
    }

    pub fn is_vertical(self) -> bool {
        matches!(self, ExportPreset::PShorts1080p | ExportPreset::PShorts720p)
    }

    pub fn matching(settings: &Settings) -> Self {
        if settings.is_short() {
            ExportPreset::PShorts1080p
        } else {
            ExportPreset::P1080p
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExportPreset::P1080p => "1080p",
            ExportPreset::P720p => "720p",
            ExportPreset::P480p => "480p",
            ExportPreset::PShorts1080p => "shorts-1080p",
            ExportPreset::PShorts720p => "shorts-720p",
        }
    }

    pub fn level(self) -> &'static str {
        match self {
            ExportPreset::P1080p => "4.1",
            ExportPreset::P720p => "3.2",
            ExportPreset::P480p => "3.1",
            ExportPreset::PShorts1080p => "4.1",
            ExportPreset::PShorts720p => "3.2",
        }
    }
}
