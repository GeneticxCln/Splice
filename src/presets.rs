use clap::ValueEnum;

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
}

impl ExportPreset {
    pub const ALL: [ExportPreset; 3] = [
        ExportPreset::P1080p,
        ExportPreset::P720p,
        ExportPreset::P480p,
    ];

    pub fn width(self) -> u32 {
        match self {
            ExportPreset::P1080p => 1920,
            ExportPreset::P720p => 1280,
            ExportPreset::P480p => 854,
        }
    }

    pub fn height(self) -> u32 {
        match self {
            ExportPreset::P1080p => 1080,
            ExportPreset::P720p => 720,
            ExportPreset::P480p => 480,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExportPreset::P1080p => "1080p",
            ExportPreset::P720p => "720p",
            ExportPreset::P480p => "480p",
        }
    }

    pub fn level(self) -> &'static str {
        match self {
            ExportPreset::P1080p => "4.1",
            ExportPreset::P720p => "3.2",
            ExportPreset::P480p => "3.1",
        }
    }
}
