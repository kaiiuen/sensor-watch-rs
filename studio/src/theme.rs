//! Theme selection.
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
