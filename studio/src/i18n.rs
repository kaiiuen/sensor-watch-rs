//! Internationalization (i18n).
//!
//! Provides a typed language system. All user-facing strings go through this
//! module so the app can be localized. English, Simplified Chinese, and
//! Traditional Chinese are implemented. Adding a new UI string means adding a
//! variant to [`Key`] and a translation for every language in [`tr`].

/// The supported languages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English,
    ChineseSimplified,
    ChineseTraditional,
}

impl Language {
    /// All supported languages, in display order.
    pub const ALL: [Language; 3] = [
        Language::English,
        Language::ChineseSimplified,
        Language::ChineseTraditional,
    ];

    /// The display name of the language.
    pub fn name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::ChineseSimplified => "简体中文",
            Language::ChineseTraditional => "繁體中文",
        }
    }
}

/// A localized string key.
///
/// Each variant maps to a string in every language. Adding a new UI string
/// means adding a variant here and a translation in `tr`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Key {
    // App
    AppTitle,
    // Navigation
    Dashboard,
    WatchFaces,
    Tutorials,
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
            Key::Tutorials => "Tutorials",
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
        Language::ChineseSimplified => match key {
            Key::AppTitle => "固件工作室",
            Key::Dashboard => "仪表盘",
            Key::WatchFaces => "表盘",
            Key::Tutorials => "教程",
            Key::Build => "构建",
            Key::Flash => "刷写",
            Key::Settings => "设置",
            Key::Target => "目标：Microchip SAM L22J18A（ARM Cortex-M0+）",
            Key::FlashRam => "闪存：256 KB  |  内存：32 KB  |  表盘：{faces}",
            Key::LastBuild => "上次构建：{path}",
            Key::NoBuildYet => "尚未构建。请前往构建面板。",
            Key::AssembleFirmware => "组装固件并生成 .uf2 文件。",
            Key::BuildUf2 => "构建 .uf2",
            Key::Building => "正在构建...",
            Key::Output => "输出：{path}",
            Key::FlashFirmware => "通过 USB 将固件刷写到手表。",
            Key::CopyToWatch => "复制到手表",
            Key::Firmware => "固件：{path}",
            Key::ConfigureApp => "配置应用和手表。",
            Key::FirmwareProject => "固件项目：",
            Key::Language => "语言",
            Key::Theme => "主题",
            Key::Ready => "就绪",
            Key::BuildComplete => "构建完成",
            Key::BuildFailed => "构建失败",
            Key::BuildThreadPanicked => "构建线程崩溃",
            Key::DebugOutput => "调试输出",
            Key::Clear => "清除",
        },
        Language::ChineseTraditional => match key {
            Key::AppTitle => "韌體工作室",
            Key::Dashboard => "儀表板",
            Key::WatchFaces => "錶盤",
            Key::Tutorials => "教學",
            Key::Build => "建置",
            Key::Flash => "燒錄",
            Key::Settings => "設定",
            Key::Target => "目標：Microchip SAM L22J18A（ARM Cortex-M0+）",
            Key::FlashRam => "快閃：256 KB  |  記憶體：32 KB  |  錶盤：{faces}",
            Key::LastBuild => "上次建置：{path}",
            Key::NoBuildYet => "尚未建置。請前往建置面板。",
            Key::AssembleFirmware => "組裝韌體並產生 .uf2 檔案。",
            Key::BuildUf2 => "建置 .uf2",
            Key::Building => "正在建置...",
            Key::Output => "輸出：{path}",
            Key::FlashFirmware => "透過 USB 將韌體燒錄到手錶。",
            Key::CopyToWatch => "複製到手錶",
            Key::Firmware => "韌體：{path}",
            Key::ConfigureApp => "設定應用程式和手錶。",
            Key::FirmwareProject => "韌體專案：",
            Key::Language => "語言",
            Key::Theme => "主題",
            Key::Ready => "就緒",
            Key::BuildComplete => "建置完成",
            Key::BuildFailed => "建置失敗",
            Key::BuildThreadPanicked => "建置執行緒當機",
            Key::DebugOutput => "偵錯輸出",
            Key::Clear => "清除",
        },
    }
}
