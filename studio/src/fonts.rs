//! Font installation.
//!
//! egui's default font has no CJK (Chinese/Japanese/Korean) glyphs, so Chinese
//! text would otherwise render as empty boxes. This module finds a system CJK
//! font at runtime and adds it to egui's font definitions, falling back to the
//! default fonts if none is found.

use eframe::egui;

/// Candidates for a CJK-capable system font, checked in order. These are the
/// common bundled fonts on Windows, macOS, and Linux.
const CJK_FONT_CANDIDATES: [&str; 8] = [
    "C:\\Windows\\Fonts\\msyh.ttc", // Windows: Microsoft YaHei (Simplified)
    "C:\\Windows\\Fonts\\simhei.ttf", // Windows: SimHei
    "C:\\Windows\\Fonts\\simsun.ttc", // Windows: SimSun
    "/System/Library/Fonts/PingFang.ttc", // macOS: PingFang SC
    "/System/Library/Fonts/STHeiti Light.ttc", // macOS
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", // Linux (Noto CJK)
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", // Linux (WenQuanYi)
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc", // Linux (WenQuanYi ZenHei)
];

fn find_cjk_font_path() -> Option<std::path::PathBuf> {
    for path in CJK_FONT_CANDIDATES {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Installs a CJK font into the egui context so Chinese text renders instead of
/// showing empty boxes. Call this once before the first frame.
pub fn install(ctx: &egui::Context) {
    let path = match find_cjk_font_path() {
        Some(p) => p,
        None => return,
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => return,
    };

    let mut fonts = egui::FontDefinitions::default();
    // Register the CJK font as a fallback for the proportional and monospace
    // families so Latin text keeps the default look but Chinese fills in.
    fonts
        .font_data
        .insert("cjk".to_owned(), egui::FontData::from_owned(bytes));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("cjk".to_owned());
    }
    ctx.set_fonts(fonts);
}
