//! Theme selection and semantic UI colors.
//!
//! Supports Light, Dark, and Auto (follows the system). Defaults to Dark.

use eframe::egui;

/// The theme mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    Auto,
}

/// Semantic colors for text, surfaces, states, and focus indicators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticColors {
    pub primary_text: egui::Color32,
    pub secondary_text: egui::Color32,
    pub disabled_text: egui::Color32,
    pub surface: egui::Color32,
    pub border: egui::Color32,
    pub success: egui::Color32,
    pub warning: egui::Color32,
    pub error: egui::Color32,
    pub info: egui::Color32,
    pub pending: egui::Color32,
    pub focus: egui::Color32,
}

/// Returns colors that remain readable against the current egui background.
fn palette(dark: bool) -> SemanticColors {
    if dark {
        SemanticColors {
            primary_text: egui::Color32::from_rgb(248, 250, 252),
            secondary_text: egui::Color32::from_rgb(203, 213, 225),
            disabled_text: egui::Color32::from_rgb(148, 163, 184),
            surface: egui::Color32::from_rgb(30, 41, 59),
            border: egui::Color32::from_rgb(100, 116, 139),
            success: egui::Color32::from_rgb(134, 239, 172),
            warning: egui::Color32::from_rgb(253, 186, 116),
            error: egui::Color32::from_rgb(252, 165, 165),
            info: egui::Color32::from_rgb(147, 197, 253),
            pending: egui::Color32::from_rgb(253, 224, 71),
            focus: egui::Color32::from_rgb(147, 197, 253),
        }
    } else {
        SemanticColors {
            primary_text: egui::Color32::from_rgb(15, 23, 42),
            secondary_text: egui::Color32::from_rgb(51, 65, 85),
            disabled_text: egui::Color32::from_rgb(71, 85, 105),
            surface: egui::Color32::from_rgb(248, 250, 252),
            border: egui::Color32::from_rgb(100, 116, 139),
            success: egui::Color32::from_rgb(22, 101, 52),
            warning: egui::Color32::from_rgb(146, 64, 14),
            error: egui::Color32::from_rgb(185, 28, 28),
            info: egui::Color32::from_rgb(29, 78, 216),
            pending: egui::Color32::from_rgb(133, 77, 14),
            focus: egui::Color32::from_rgb(29, 78, 216),
        }
    }
}

/// Returns colors that remain readable against the current egui background.
pub fn semantic(ui: &egui::Ui) -> SemanticColors {
    palette(ui.visuals().dark_mode)
}

impl Theme {
    /// All themes, in display order.
    pub const ALL: [Theme; 3] = [Theme::Light, Theme::Dark, Theme::Auto];

    /// The display name of the theme.
    pub fn name(self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
            Theme::Auto => "Auto",
        }
    }

    /// Applies the theme to the egui context.
    pub fn apply(self, ctx: &egui::Context) {
        match self {
            Theme::Light => ctx.set_visuals(egui::Visuals::light()),
            Theme::Dark => ctx.set_visuals(egui::Visuals::dark()),
            Theme::Auto => {
                // Follow the system preference.
                let dark = ctx.style().visuals.dark_mode;
                ctx.set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::palette;
    use eframe::egui;

    fn luminance(color: egui::Color32) -> f32 {
        fn channel(value: u8) -> f32 {
            let value = value as f32 / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }

    fn contrast(foreground: egui::Color32, background: egui::Color32) -> f32 {
        let light = luminance(foreground).max(luminance(background));
        let dark = luminance(foreground).min(luminance(background));
        (light + 0.05) / (dark + 0.05)
    }

    #[test]
    fn secondary_text_meets_instructional_contrast_floor() {
        for dark in [false, true] {
            let colors = palette(dark);
            let background = if dark {
                egui::Color32::from_rgb(30, 41, 59)
            } else {
                egui::Color32::WHITE
            };
            assert!(contrast(colors.secondary_text, background) >= 4.5);
        }
    }

    #[test]
    fn light_and_dark_palettes_are_distinct() {
        let light = palette(false);
        let dark = palette(true);
        assert_ne!(light.primary_text, dark.primary_text);
        assert_ne!(light.surface, dark.surface);
    }
}
