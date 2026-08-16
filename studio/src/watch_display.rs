//! Casio F-91W digital display renderer.
//!
//! Renders the actual F-91W SVG (a 1:1 replica of the online simulator) using
//! `usvg` + `resvg`. Each segment/indicator is toggled on/off by injecting an
//! `opacity` attribute into the SVG source before rendering - exactly like the
//! JS `displayScreen` sets `el.style.opacity`.

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

use super::face_sim::FaceDisplay;
use super::watch_sim::Display;

/// The SVG source, embedded at compile time.
const WATCH_SVG: &str = include_str!("../assets/watch.svg");

/// A parsed, renderable watch.
pub struct WatchRenderer {
    /// The parsed SVG tree for the last display state.
    tree: usvg::Tree,
    last_display: Option<Display>,
    cached_texture: Option<(Display, [u32; 2], TextureHandle)>,
}

impl WatchRenderer {
    /// Parses the embedded SVG.
    pub fn new() -> Self {
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_str(WATCH_SVG, &opt).expect("failed to parse watch SVG");
        WatchRenderer {
            tree,
            last_display: None,
            cached_texture: None,
        }
    }

    /// Renders the watch with the given display state into a ColorImage.
    /// Returns None if rendering fails (e.g. invalid SVG or zero size) instead
    /// of panicking, so a render hiccup can't crash the app.
    pub fn render(&mut self, display: &Display, size: [u32; 2]) -> Option<ColorImage> {
        if size[0] == 0 || size[1] == 0 {
            return None;
        }
        // Reparse only when the LCD state changes. The simulator repaints for
        // input and background activity far more often than the display changes.
        if self.last_display != Some(*display) {
            let svg = apply_display_to_svg(WATCH_SVG, display);
            let opt = usvg::Options::default();
            self.tree = usvg::Tree::from_str(&svg, &opt).ok()?;
            self.last_display = Some(*display);
        }

        let mut pixmap = resvg::tiny_skia::Pixmap::new(size[0], size[1])?;
        let transform = resvg::tiny_skia::Transform::from_scale(
            size[0] as f32 / 1480.0,
            size[1] as f32 / 1311.0,
        );
        resvg::render(&self.tree, transform, &mut pixmap.as_mut());

        // tiny-skia already stores tightly packed RGBA pixels; copy the slice
        // directly instead of allocating and rebuilding it per pixel.
        Some(ColorImage::from_rgba_unmultiplied(
            [size[0] as usize, size[1] as usize],
            pixmap.data(),
        ))
    }
}

/// Injects `opacity` attributes into the SVG source for each segment/indicator.
fn apply_display_to_svg(svg: &str, d: &Display) -> String {
    let mut out = String::with_capacity(svg.len() + 512);
    let mut rest = svg;
    // Process the SVG element by element, looking for `id="..."`.
    while let Some(pos) = rest.find("id=\"") {
        // Copy everything up to the id.
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        // Read the id value.
        let id_start = 4; // after `id="`
        let id_end = rest[id_start..]
            .find('"')
            .map(|e| id_start + e)
            .unwrap_or(0);
        let id = &rest[4..id_end];
        // Determine the opacity for this id.
        let opacity = element_opacity(id, d);
        // Copy the id attribute and the rest of the tag.
        // We need to find the end of the opening tag (`>` or `/>`).
        let tag_end = rest[id_end + 1..]
            .find('>')
            .map(|e| id_end + 1 + e)
            .unwrap_or(rest.len());
        let tag = &rest[..tag_end + 1];
        // Insert opacity into the tag if it's an element with this id.
        if let Some(op) = opacity {
            // Remove any existing `opacity="..."` attribute, then insert ours
            // before the closing `>` (or `/>`) of the tag.
            let cleaned = remove_opacity_attr(tag);
            // Find the closing: either `/>` (self-closing) or `>`.
            if let Some(slash) = cleaned.rfind("/>") {
                out.push_str(&cleaned[..slash]);
                out.push_str(&format!(" opacity=\"{op}\""));
                out.push_str(&cleaned[slash..]);
            } else if let Some(gt) = cleaned.rfind('>') {
                out.push_str(&cleaned[..gt]);
                out.push_str(&format!(" opacity=\"{op}\""));
                out.push_str(&cleaned[gt..]);
            } else {
                out.push_str(&cleaned);
            }
        } else {
            out.push_str(tag);
        }
        rest = &rest[tag_end + 1..];
    }
    out.push_str(rest);
    out
}

/// Removes an existing `opacity="..."` attribute from a tag.
fn remove_opacity_attr(tag: &str) -> String {
    let mut out = String::with_capacity(tag.len());
    let mut rest = tag;
    while let Some(pos) = rest.find("opacity=\"") {
        out.push_str(&rest[..pos]);
        rest = &rest[pos + "opacity=\"".len()..];
        let end = rest.find('"').unwrap_or(rest.len());
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Returns the opacity (0.0 or 1.0) for an element ID, or None if not a segment.
fn element_opacity(id: &str, d: &Display) -> Option<f32> {
    // Indicator elements.
    match id {
        "alarmOnMark" => return Some(if d.alarm_on_mark { 1.0 } else { 0.0 }),
        "timeSignalOnMark" => return Some(if d.time_signal_on_mark { 1.0 } else { 0.0 }),
        "timeMode24" => return Some(if d.time_mode_24 { 1.0 } else { 0.0 }),
        "timeMode12" => return Some(if d.time_mode_12 { 1.0 } else { 0.0 }),
        "lap" => return Some(if d.lap { 1.0 } else { 0.0 }),
        "dot-top" | "dot-bottom" => return Some(if d.dots { 1.0 } else { 0.0 }),
        "light" => return Some(if d.light { 0.4 } else { 0.0 }),
        _ => {}
    }

    // Segment displays: id like "second_1_G".
    let (display_id, segment) = match id.rsplit_once('_') {
        Some((d, s)) if s.len() == 1 => (d, s),
        _ => return None,
    };
    // Only treat as a segment if the suffix is a real segment letter (A-I).
    // This ignores group ids like "mode_2_2" so the whole display isn't hidden.
    if !matches!(segment, "A" | "B" | "C" | "D" | "E" | "F" | "G" | "H" | "I") {
        return None;
    }
    let c = match display_id {
        "mode_2" => d.mode_2,
        "mode_1" => d.mode_1,
        "day_2" => d.day_2,
        "day_1" => d.day_1,
        "hour_2" => d.hour_2,
        "hour_1" => d.hour_1,
        "minute_2" => d.minute_2,
        "minute_1" => d.minute_1,
        "second_2" => d.second_2,
        "second_1" => d.second_1,
        _ => return None,
    };
    let segments = char_segments(display_id, c);
    Some(if segments.contains(&segment) {
        1.0
    } else {
        0.0
    })
}

/// Returns the set of on-segments for a character on a given display.
fn char_segments(display_id: &str, c: char) -> Vec<&'static str> {
    // Use the firmware's real character set so all letters render correctly.
    let segdata = if (c as u32) >= 0x20 && (c as u32) < 0x7F {
        crate::face_sim::CHARACTER_SET[(c as usize) - 0x20]
    } else {
        0
    };
    let seg_names = ["A", "B", "C", "D", "E", "F", "G", "H"];
    let mut out = Vec::new();
    for (i, seg) in seg_names[..7].iter().enumerate() {
        if segdata & (1 << i) != 0 {
            out.push(*seg);
        }
    }
    // Legibility overrides: render 'T' as a backwards 7 (top + right verticals)
    // and 'I' as a left-side bar, which read better on the 7-segment LCD.
    if c == 'T' {
        out = vec!["A", "B", "C"];
    } else if c == 'I' {
        out = vec!["E", "F"];
    }
    // Keep the special mode_1/mode_2 handling for the F-91W weekday display.
    if display_id == "mode_1" {
        return match c {
            'T' => vec!["A", "E", "F", "H"],
            'R' => vec!["A", "B", "C", "E", "F", "G", "H"],
            _ => out,
        };
    }
    if display_id == "mode_2" {
        return match c {
            'M' => vec!["A", "B", "C", "E", "F", "H", "I"],
            'T' => vec!["A", "H", "I"],
            'H' => vec!["B", "C", "E", "F", "G"],
            'W' => vec!["B", "C", "D", "E", "F", "H", "I"],
            _ => out,
        };
    }
    out
}

/// Renders the watch to an egui texture.
pub fn render_to_texture(
    renderer: &mut WatchRenderer,
    display: &Display,
    size: [u32; 2],
    ctx: &egui::Context,
) -> Option<TextureHandle> {
    if let Some((cached_display, cached_size, texture)) = &renderer.cached_texture {
        if cached_display == display && *cached_size == size {
            return Some(texture.clone());
        }
    }
    let image = renderer.render(display, size)?;
    let texture = ctx.load_texture("watch", image, TextureOptions::LINEAR);
    renderer.cached_texture = Some((*display, size, texture.clone()));
    Some(texture)
}

/// Converts a firmware-style `FaceDisplay` (10 chars + indicators) into the
/// SVG `Display` state, using the firmware's real character set so text renders
/// correctly on the 7-segment display.
#[allow(clippy::field_reassign_with_default)]
pub fn face_display_to_svg(fd: &FaceDisplay) -> Display {
    let mut d = Display::default();
    d.dots = fd.colon;
    d.alarm_on_mark = fd.signal;
    d.time_signal_on_mark = fd.bell;
    d.time_mode_24 = fd.h24;
    // The SVG's `timeMode12` group is the physical PM glyph (despite its
    // historical ID), not a generic "12-hour mode" indicator. PM is never
    // shown alongside the 24-hour glyph.
    d.time_mode_12 = fd.pm && !fd.h24;
    d.lap = fd.lap;
    // Map the 10 LCD positions to the SVG display IDs.
    // positions: 0,1 -> mode_2,mode_1 ; 2,3 -> day_2,day_1 ; 4,5 -> hour_2,hour_1
    //            6,7 -> minute_2,minute_1 ; 8,9 -> second_2,second_1
    d.mode_2 = fd.chars[0];
    d.mode_1 = fd.chars[1];
    d.day_2 = fd.chars[2];
    d.day_1 = fd.chars[3];
    d.hour_2 = fd.chars[4];
    d.hour_1 = fd.chars[5];
    d.minute_2 = fd.chars[6];
    d.minute_1 = fd.chars[7];
    d.second_2 = fd.chars[8];
    d.second_1 = fd.chars[9];
    d
}

impl Default for WatchRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pm_maps_to_pm_glyph_without_24_hour_glyph() {
        let fd = FaceDisplay {
            pm: true,
            h24: false,
            ..FaceDisplay::default()
        };
        let display = face_display_to_svg(&fd);
        assert!(!display.time_mode_24);
        assert!(display.time_mode_12);
        assert_eq!(element_opacity("timeMode12", &display), Some(1.0));
        assert_eq!(element_opacity("timeMode24", &display), Some(0.0));
    }

    #[test]
    fn am_hides_pm_glyph_in_12_hour_mode() {
        let fd = FaceDisplay {
            pm: false,
            h24: false,
            ..FaceDisplay::default()
        };
        let display = face_display_to_svg(&fd);
        // `timeMode12` is the PM glyph, not a generic 12-hour-mode marker.
        assert!(!display.time_mode_12);
        assert!(!display.time_mode_24);
        assert_eq!(element_opacity("timeMode12", &display), Some(0.0));
    }

    #[test]
    fn twenty_four_hour_display_never_maps_pm_to_mode_glyph() {
        let fd = FaceDisplay {
            pm: true,
            h24: true,
            ..FaceDisplay::default()
        };
        let display = face_display_to_svg(&fd);
        assert!(display.time_mode_24);
        assert!(!display.time_mode_12);
        assert_eq!(element_opacity("timeMode24", &display), Some(1.0));
        assert_eq!(element_opacity("timeMode12", &display), Some(0.0));
    }

    #[test]
    fn false_indicators_are_hidden_without_inverting_their_state() {
        let mut hidden = Display::default();
        assert_eq!(element_opacity("alarmOnMark", &hidden), Some(0.0));
        assert_eq!(element_opacity("timeSignalOnMark", &hidden), Some(0.0));
        assert_eq!(element_opacity("timeMode24", &hidden), Some(0.0));
        assert_eq!(element_opacity("timeMode12", &hidden), Some(0.0));
        assert_eq!(element_opacity("lap", &hidden), Some(0.0));
        assert_eq!(element_opacity("dot-top", &hidden), Some(0.0));

        hidden.alarm_on_mark = true;
        hidden.time_signal_on_mark = true;
        hidden.time_mode_24 = true;
        hidden.time_mode_12 = true;
        hidden.lap = true;
        hidden.dots = true;
        hidden.light = true;
        assert_eq!(element_opacity("alarmOnMark", &hidden), Some(1.0));
        assert_eq!(element_opacity("timeSignalOnMark", &hidden), Some(1.0));
        assert_eq!(element_opacity("timeMode24", &hidden), Some(1.0));
        assert_eq!(element_opacity("timeMode12", &hidden), Some(1.0));
        assert_eq!(element_opacity("lap", &hidden), Some(1.0));
        assert_eq!(element_opacity("dot-bottom", &hidden), Some(1.0));
        assert_eq!(element_opacity("light", &hidden), Some(0.4));
    }
}
