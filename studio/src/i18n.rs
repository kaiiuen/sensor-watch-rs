//! Internationalization (i18n).
//!
//! Provides a typed language system. Only English is implemented for now, but
//! the structure supports adding more languages later. All user-facing strings
//! go through this module so the app can be localized.

/// The supported languages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English,
}

impl Language {
    /// All supported languages, in display order.
    pub const ALL: [Language; 1] = [Language::English];

    /// The display name of the language.
    pub fn name(self) -> &'static str {
        match self {
            Language::English => "English",
        }
    }
}

/// A localized string key.
///
/// Each variant maps to a string in every language. Adding a new UI string
/// means adding a variant here and a translation in `tr`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    // App
    AppTitle,
    // Navigation
    Dashboard,
    WatchFaces,
    Build,
    Flash,
    Settings,
    // Dashboard
    Target,
    FlashRam,
    LastBuild,
    NoBuildYet,
    // Faces
    // Build
    AssembleFirmware,
    BuildUf2,
    Building,
    Output,
    // Flash
    FlashFirmware,
    CopyToWatch,
    Firmware,
    // Settings
    ConfigureApp,
    FirmwareProject,
    Language,
    Theme,
    // Status
    Ready,
    BuildComplete,
    BuildFailed,
    BuildThreadPanicked,
    // Debug
    DebugOutput,
    Clear,
}

/// Translates a key into the given language.
pub fn tr(lang: Language, key: Key) -> &'static str {
    match lang {
        Language::English => match key {
            Key::AppTitle => "Firmware Studio",
            Key::Dashboard => "Dashboard",
            Key::WatchFaces => "Watch Faces",
            Key::Build => "Build",
            Key::Flash => "Flash",
            Key::Settings => "Settings",
            Key::Target => "Target: Microchip SAM L22J18A (ARM Cortex-M0+)",
            Key::FlashRam => "Flash: 256 KB  |  RAM: 32 KB  |  Faces: {faces}",
            Key::LastBuild => "Last build: {path}",
            Key::NoBuildYet => "No build yet. Go to the Build panel.",
            Key::AssembleFirmware => "Assemble the firmware and produce a .uf2 file.",
            Key::BuildUf2 => "Build .uf2",
            Key::Building => "Building...",
            Key::Output => "Output: {path}",
            Key::FlashFirmware => "Flash the firmware to the watch over USB.",
            Key::CopyToWatch => "Copy to watch",
            Key::Firmware => "Firmware: {path}",
            Key::ConfigureApp => "Configure the app and the watch.",
            Key::FirmwareProject => "Firmware project:",
            Key::Language => "Language",
            Key::Theme => "Theme",
            Key::Ready => "Ready",
            Key::BuildComplete => "Build complete",
            Key::BuildFailed => "Build failed",
            Key::BuildThreadPanicked => "Build thread panicked",
            Key::DebugOutput => "Debug Output",
            Key::Clear => "Clear",
        },
    }
}
