//! Casio F-91W digital display renderer.
//!
//! Draws the watch's LCD using egui's painter: a monochrome (white-on-dark)
//! 7-segment display with the same layout as the real F-91W. Each digit is
//! drawn as 7 segments (A-G), plus the special 8/9-segment mode displays.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

use super::watch_sim::Display;

/// The segment bit layout for each character (7-segment).
/// Bits: 0=A, 1=B, 2=C, 3=D, 4=E, 5=F, 6=G.
fn char_segments(c: char) -> u8 {
    match c {
        '0' => 0b1111110,
        '1' => 0b0110000,
        '2' => 0b1101101,
        '3' => 0b1111001,
        '4' => 0b0110011,
        '5' => 0b1011011,
        '6' => 0b1011111,
        '7' => 0b1110000,
        '8' => 0b1111111,
        '9' => 0b1111011,
        'A' => 0b1110111,
        'C' => 0b1001110,
        'E' => 0b1001111,
        'F' => 0b1000111,
        'H' => 0b0110111,
        'I' => 0b0110000,
        'L' => 0b0001110,
        'O' => 0b1111110,
        'S' => 0b1011011,
        'U' => 0b0111110,
        ' ' => 0,
        _ => 0,
    }
}

/// Draws a single 7-segment digit at the given position.
fn draw_digit(
    painter: &egui::Painter,
    origin: Pos2,
    size: Vec2,
    c: char,
    on: Color32,
    off: Color32,
) {
    let seg = char_segments(c);
    let w = size.x;
    let h = size.y;
    let thick = (w * 0.18).max(2.0);
    let thin = thick * 0.6;

    // Segment geometry (A top, B top-right, C bottom-right, D bottom,
    // E bottom-left, F top-left, G middle).
    let segs: [(usize, Vec2, Vec2); 7] = [
        // A: top horizontal
        (0, Vec2::new(thin, 0.0), Vec2::new(w - thin, 0.0)),
        // B: top-right vertical
        (1, Vec2::new(w, thin), Vec2::new(w, h / 2.0 - thin)),
        // C: bottom-right vertical
        (2, Vec2::new(w, h / 2.0 + thin), Vec2::new(w, h - thin)),
        // D: bottom horizontal
        (3, Vec2::new(thin, h), Vec2::new(w - thin, h)),
        // E: bottom-left vertical
        (4, Vec2::new(0.0, h / 2.0 + thin), Vec2::new(0.0, h - thin)),
        // F: top-left vertical
        (5, Vec2::new(0.0, thin), Vec2::new(0.0, h / 2.0 - thin)),
        // G: middle horizontal
        (6, Vec2::new(thin, h / 2.0), Vec2::new(w - thin, h / 2.0)),
    ];

    for (bit, start, end) in segs {
        let color = if seg & (1 << bit) != 0 { on } else { off };
        let thickness = if bit == 0 || bit == 3 || bit == 6 {
            thick
        } else {
            thin
        };
        painter.line_segment(
            [origin + start, origin + end],
            Stroke::new(thickness, color),
        );
    }
}

/// Draws the full F-91W display.
pub fn draw_display(painter: &egui::Painter, rect: Rect, display: &Display) {
    let on = Color32::from_rgb(0x30, 0x42, 0x46); // the LCD "on" color
    let off = Color32::from_rgb(0x20, 0x28, 0x2a); // faint off segments
    if display.light {
        // Backlight: brighten the whole display.
    }

    let w = rect.width();
    let h = rect.height();
    let origin = rect.min;

    // Digit size for the main time (hours/minutes/seconds).
    let digit_w = w * 0.11;
    let digit_h = h * 0.30;

    // Layout positions (approximate the real F-91W).
    // Row 1: mode (day letters) + day.
    // Row 2: hours : minutes : seconds.
    let row1_y = origin.y + h * 0.08;
    let row2_y = origin.y + h * 0.45;

    // Mode letters (2 chars, 9/8-segment) at top-left.
    let mode_x = origin.x + w * 0.06;
    draw_digit(
        painter,
        Pos2::new(mode_x, row1_y),
        Vec2::new(digit_w * 0.7, digit_h * 0.5),
        display.mode_2,
        on,
        off,
    );
    draw_digit(
        painter,
        Pos2::new(mode_x + digit_w * 0.75, row1_y),
        Vec2::new(digit_w * 0.7, digit_h * 0.5),
        display.mode_1,
        on,
        off,
    );

    // Day (2 digits) at top-right.
    let day_x = origin.x + w * 0.70;
    draw_digit(
        painter,
        Pos2::new(day_x, row1_y),
        Vec2::new(digit_w * 0.7, digit_h * 0.5),
        display.day_2,
        on,
        off,
    );
    draw_digit(
        painter,
        Pos2::new(day_x + digit_w * 0.75, row1_y),
        Vec2::new(digit_w * 0.7, digit_h * 0.5),
        display.day_1,
        on,
        off,
    );

    // Hours (2 digits).
    let hour_x = origin.x + w * 0.06;
    draw_digit(
        painter,
        Pos2::new(hour_x, row2_y),
        Vec2::new(digit_w, digit_h),
        display.hour_2,
        on,
        off,
    );
    draw_digit(
        painter,
        Pos2::new(hour_x + digit_w * 1.1, row2_y),
        Vec2::new(digit_w, digit_h),
        display.hour_1,
        on,
        off,
    );

    // Colon dots.
    let colon_x = origin.x + w * 0.34;
    if display.dots {
        painter.circle_filled(Pos2::new(colon_x, row2_y + digit_h * 0.25), 3.0, on);
        painter.circle_filled(Pos2::new(colon_x, row2_y + digit_h * 0.75), 3.0, on);
    }

    // Minutes (2 digits).
    let min_x = origin.x + w * 0.40;
    draw_digit(
        painter,
        Pos2::new(min_x, row2_y),
        Vec2::new(digit_w, digit_h),
        display.minute_2,
        on,
        off,
    );
    draw_digit(
        painter,
        Pos2::new(min_x + digit_w * 1.1, row2_y),
        Vec2::new(digit_w, digit_h),
        display.minute_1,
        on,
        off,
    );

    // Seconds (2 digits), smaller.
    let sec_x = origin.x + w * 0.72;
    let sec_w = digit_w * 0.7;
    let sec_h = digit_h * 0.7;
    let sec_y = row2_y + digit_h * 0.15;
    draw_digit(
        painter,
        Pos2::new(sec_x, sec_y),
        Vec2::new(sec_w, sec_h),
        display.second_2,
        on,
        off,
    );
    draw_digit(
        painter,
        Pos2::new(sec_x + sec_w * 1.1, sec_y),
        Vec2::new(sec_w, sec_h),
        display.second_1,
        on,
        off,
    );
}
