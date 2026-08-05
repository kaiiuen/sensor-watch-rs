//! Casio F-91W digital display renderer.
//!
//! Renders the actual F-91W SVG (a 1:1 replica of the online simulator) using
//! `usvg` + `resvg`. Each segment/indicator is toggled on/off by injecting an
//! `opacity` attribute into the SVG source before rendering — exactly like the
//! JS `displayScreen` sets `el.style.opacity`.

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

use super::watch_sim::Display;

/// The SVG source, embedded at compile time.
const WATCH_SVG: &str = include_str!("../assets/watch.svg");

/// A parsed, renderable watch.
pub struct WatchRenderer {
    /// The parsed SVG tree (rebuilt each render from the modified source).
    tree: usvg::Tree,
}

impl WatchRenderer {
    /// Parses the embedded SVG.
    pub fn new() -> Self {
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_str(WATCH_SVG, &opt).expect("failed to parse watch SVG");
        WatchRenderer { tree }
    }

    /// Renders the watch with the given display state into a ColorImage.
    pub fn render(&mut self, display: &Display, size: [u32; 2]) -> ColorImage {
        // Build the SVG source with the display state applied.
        let svg = apply_display_to_svg(WATCH_SVG, display);

        // Re-parse and render.
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_str(&svg, &opt).expect("failed to re-parse watch SVG");
        self.tree = tree;

        let mut pixmap = resvg::tiny_skia::Pixmap::new(size[0], size[1]).expect("pixmap");
        let transform = resvg::tiny_skia::Transform::from_scale(
            size[0] as f32 / 1480.0,
            size[1] as f32 / 1311.0,
        );
        resvg::render(&self.tree, transform, &mut pixmap.as_mut());

        let data = pixmap.data();
        let mut rgba = Vec::with_capacity(data.len());
        for px in data.chunks(4) {
            rgba.extend_from_slice(&[px[0], px[1], px[2], px[3]]);
        }
        ColorImage::from_rgba_unmultiplied([size[0] as usize, size[1] as usize], &rgba)
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
    let seven: Vec<&str> = match c {
        '0' => vec!["A", "B", "C", "D", "E", "F"],
        '1' => vec!["B", "C"],
        '2' => vec!["A", "B", "D", "E", "G"],
        '3' => vec!["A", "B", "C", "D", "G"],
        '4' => vec!["B", "C", "F", "G"],
        '5' => vec!["A", "C", "D", "F", "G"],
        '6' => vec!["A", "C", "D", "E", "F", "G"],
        '7' => vec!["A", "B", "C"],
        '8' => vec!["A", "B", "C", "D", "E", "F", "G"],
        '9' => vec!["A", "B", "C", "D", "F", "G"],
        'A' => vec!["A", "B", "C", "E", "F", "G"],
        'C' => vec!["A", "D", "E", "F"],
        'E' => vec!["A", "D", "E", "F", "G"],
        'F' => vec!["A", "E", "F", "G"],
        'H' => vec!["B", "C", "E", "F", "G"],
        'I' => vec!["B", "C"],
        'L' => vec!["D", "E", "F"],
        'O' => vec!["A", "B", "C", "D", "E", "F"],
        'S' => vec!["A", "C", "D", "F", "G"],
        'U' => vec!["B", "C", "D", "E", "F"],
        ' ' => vec![],
        _ => vec![],
    };

    if display_id == "mode_1" {
        return match c {
            'T' => vec!["A", "E", "F", "H"],
            'R' => vec!["A", "B", "C", "E", "F", "G", "H"],
            _ => seven,
        };
    }
    if display_id == "mode_2" {
        return match c {
            'M' => vec!["A", "B", "C", "E", "F", "H", "I"],
            'T' => vec!["A", "H", "I"],
            'H' => vec!["B", "C", "E", "F", "G"],
            'W' => vec!["B", "C", "D", "E", "F", "H", "I"],
            _ => seven,
        };
    }
    seven
}

/// Renders the watch to an egui texture.
pub fn render_to_texture(
    renderer: &mut WatchRenderer,
    display: &Display,
    size: [u32; 2],
    ctx: &egui::Context,
) -> TextureHandle {
    let image = renderer.render(display, size);
    ctx.load_texture("watch", image, TextureOptions::LINEAR)
}

impl Default for WatchRenderer {
    fn default() -> Self {
        Self::new()
    }
}
