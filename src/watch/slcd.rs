//! Segment LCD display driver.
//!
//! Port of the C `watch_slcd.c`, `watch_private_display.c`, and the SLCD HAL
//! init from the Sensor-Watch reference. This drives the 10-digit segment LCD
//! plus the indicator segments and colon.

use atsaml22j::slcd::RegisterBlock as Slcd;
use atsaml22j::slcd::ctrla::{
    Biasselect, Dutyselect, Prescselect, Prfselect, Rrfselect, Wmodselect,
};

/// Returns a reference to the SLCD peripheral register block.
fn slcd() -> &'static Slcd {
    // SAFETY: the SLCD register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { &*atsaml22j::Slcd::PTR }
}

/// Returns a reference to the MCLK peripheral register block.
fn mclk() -> &'static atsaml22j::mclk::RegisterBlock {
    // SAFETY: the MCLK register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { &*atsaml22j::Mclk::PTR }
}

/// Waits for the SLCD to finish synchronizing.
fn sync() {
    while slcd().syncbusy().read().bits() != 0 {}
}

/// The indicator segments available on the watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Indicator {
    /// The hourly signal indicator; also useful for indicating sensors are on.
    Signal = 0,
    /// The small bell indicating an alarm is set.
    Bell,
    /// The PM indicator.
    Pm,
    /// The 24H indicator.
    H24,
    /// The LAP indicator.
    Lap,
}

/// Segment IDs for each indicator, as (com, seg) pairs.
const INDICATOR_SEGMENTS: [(u8, u8); 5] = [
    (0, 17), // Signal
    (0, 16), // Bell
    (2, 17), // PM
    (2, 16), // 24H
    (1, 10), // Lap
];

/// Character set: 7-segment bit patterns indexed by (ASCII - 0x20).
/// Bit 0 = segment A, bit 1 = B, ... bit 6 = G, bit 7 = DP.
const CHARACTER_SET: [u8; 95] = [
    0b00000000, // ' ' (0x20)
    0b01100000, // '!' (L in the top half for positions 4 and 6)
    0b00100010, // '"'
    0b01100011, // '#' (degree symbol, hash mark doesn't fit)
    0b00101101, // '$' (S without the center segment)
    0b00000000, // '%' (unused)
    0b01000100, // '&' ("lowercase 7" for positions 4 and 6)
    0b00100000, // '\''
    0b00111001, // '('
    0b00001111, // ')'
    0b11000000, // '*' (the + sign for use in position 0)
    0b01110000, // '+' (segments E, F and G; looks like ┣╸)
    0b00000100, // ','
    0b01000000, // '-'
    0b01000000, // '.' (same as -, semantically most useful)
    0b00010010, // '/'
    0b00111111, // '0'
    0b00000110, // '1'
    0b01011011, // '2'
    0b01001111, // '3'
    0b01100110, // '4'
    0b01101101, // '5'
    0b01111101, // '6'
    0b00000111, // '7'
    0b01111111, // '8'
    0b01101111, // '9'
    0b00000000, // ':' (unused)
    0b00000000, // ';' (unused)
    0b01011000, // '<'
    0b01001000, // '='
    0b01001100, // '>'
    0b01010011, // '?'
    0b11111111, // '@' (all segments on)
    0b01110111, // 'A'
    0b01111111, // 'B'
    0b00111001, // 'C'
    0b00111111, // 'D'
    0b01111001, // 'E'
    0b01110001, // 'F'
    0b00111101, // 'G'
    0b01110110, // 'H'
    0b10001001, // 'I' (only works in position 0)
    0b00001110, // 'J'
    0b01110101, // 'K'
    0b00111000, // 'L'
    0b10110111, // 'M' (only works in position 0)
    0b00110111, // 'N'
    0b00111111, // 'O'
    0b01110011, // 'P'
    0b01100111, // 'Q'
    0b11110111, // 'R' (only works in position 1)
    0b01101101, // 'S'
    0b10000001, // 'T' (only works in position 0; set (1,12) to make it work in position 1)
    0b00111110, // 'U'
    0b00111110, // 'V'
    0b10111110, // 'W' (only works in position 0)
    0b01111110, // 'X'
    0b01101110, // 'Y'
    0b00011011, // 'Z'
    0b00111001, // '['
    0b00100100, // '\'
    0b00001111, // ']'
    0b00100011, // '^'
    0b00001000, // '_'
    0b00000010, // '`'
    0b01011111, // 'a'
    0b01111100, // 'b'
    0b01011000, // 'c'
    0b01011110, // 'd'
    0b01111011, // 'e'
    0b01110001, // 'f'
    0b01101111, // 'g'
    0b01110100, // 'h'
    0b00010000, // 'i'
    0b01000010, // 'j' (appears as superscript to work in more positions)
    0b01110101, // 'k'
    0b00110000, // 'l'
    0b10110111, // 'm' (only works in position 0)
    0b01010100, // 'n'
    0b01011100, // 'o'
    0b01110011, // 'p'
    0b01100111, // 'q'
    0b01010000, // 'r'
    0b01101101, // 's'
    0b01111000, // 't'
    0b01100010, // 'u' (appears in (u)pper half to work in more positions)
    0b00011100, // 'v' (looks like u but in the lower half)
    0b10111110, // 'w' (only works in position 0)
    0b01111110, // 'x'
    0b01101110, // 'y'
    0b00011011, // 'z'
    0b00010110, // '{' (overridden to represent "il")
    0b00110110, // '|' (overridden to represent "ll")
    0b00110100, // '}' (overridden to represent "li")
    0b00000001, // '~'
];

/// Segment map: for each of the 10 digit positions, 8 packed (com, seg) pairs.
/// Each byte holds a COM number in bits 6-7 and a segment number in bits 0-5.
/// COM 3 means no segment exists for that slot.
const SEGMENT_MAP: [u64; 10] = [
    0x4e4f0e8e8f8d4d0d, // Position 0, mode
    0xc8c4c4c8b4b4b0b,  // Position 1, mode (Segments B and C shared, as are E and F)
    0xc049c00a49890949, // Position 2, day of month
    0xc048088886874707, // Position 3, day of month
    0xc053921252139352, // Position 4, clock hours (Segments A and D shared)
    0xc054511415559594, // Position 5, clock hours
    0xc057965616179716, // Position 6, clock minutes (Segments A and D shared)
    0xc041804000018a81, // Position 7, clock minutes
    0xc043420203048382, // Position 8, clock seconds
    0xc045440506468584, // Position 9, clock seconds
];

/// Number of display characters.
const NUM_CHARS: u8 = 10;

/// Enables the Segment LCD display.
///
/// Call this before attempting to set pixels or display strings.
pub fn enable_display() {
    init();
    slcd().ctrla().modify(|_, w| w.enable().set_bit());
}

/// Initializes the SLCD peripheral (port of `_slcd_sync_init` + `SEGMENT_LCD_0_init`).
fn init() {
    // Enable the SLCD APB clock (SLCD is on the APBC bus).
    mclk().apbcmask().modify(|_, w| w.slcd_().set_bit());

    // Software reset (if not already syncing).
    if !slcd().syncbusy().read().swrst().bit_is_set() {
        if slcd().ctrla().read().enable().bit_is_set() {
            slcd().ctrla().modify(|_, w| w.enable().clear_bit());
            sync();
        }
        slcd().ctrla().modify(|_, w| w.swrst().set_bit());
    }
    sync();

    // Configure the SLCD registers.
    // SAFETY: all field values below are valid per the reference config.
    unsafe {
        slcd().ctrla().write(|w| {
            w.duty().variant(Dutyselect::Third); // CONF_SLCD_COM_NUM = 2 -> 3 COM lines
            w.wmod().variant(Wmodselect::Lp); // CONF_SLCD_WMOD = 0
            w.runstdby().set_bit(); // CONF_SLCD_RUNSTDBY = 1
            w.presc().variant(Prescselect::Presc64); // CONF_SLCD_PRESC = 2
            w.ckdiv().bits(4); // CONF_SLCD_CKDIV = 4
            w.bias().variant(Biasselect::Third); // CONF_SLCD_BIAS = 2
            w.xvlcd().clear_bit(); // CONF_SLCD_XVLCD = 0
            w.prf().variant(Prfselect::Pr250); // CONF_SLCD_PRF = 3
            w.rrf().variant(Rrfselect::Rr62) // CONF_SLCD_RRF = 5
        });
        slcd().ctrlb().modify(|_, w| {
            w.bben().set_bit(); // CONF_SLCD_BBEN = 1
            w.bbd().bits(1) // CONF_SLCD_BBD = 2 -> 2-1
        });
        slcd().ctrlc().modify(|_, w| w.ctst().bits(14)); // CONF_SLCD_CONTRAST_ADJUST = 14
    }
    slcd().ctrld().modify(|_, w| w.dispen().set_bit()); // SLCD_CTRLD_DISPEN

    // Clear all segment data.
    clear_display();

    // Set blink mode to "blink selected segments".
    slcd().bcfg().modify(|_, w| w.mode().blinksel());
}

/// Sets a pixel with the given common and segment number.
pub fn set_pixel(com: u8, seg: u8) {
    set_segment(com, seg, true);
}

/// Clears a pixel with the given common and segment number.
pub fn clear_pixel(com: u8, seg: u8) {
    set_segment(com, seg, false);
}

/// Sets or clears a segment. Port of `_slcd_sync_set_segment`.
fn set_segment(com: u8, seg: u8, on: bool) {
    // The watch only uses segments 0-23, so seg < 32 always; we only touch the
    // SDATAL registers (bit `seg` of SDATAL{com}).
    let mask = 1u32 << seg;
    // SAFETY: writing a valid segment-data bitmask.
    match com {
        0 => {
            if on {
                unsafe { slcd().sdatal0().modify(|r, w| w.bits(r.bits() | mask)) };
            } else {
                unsafe { slcd().sdatal0().modify(|r, w| w.bits(r.bits() & !mask)) };
            }
        }
        1 => {
            if on {
                unsafe { slcd().sdatal1().modify(|r, w| w.bits(r.bits() | mask)) };
            } else {
                unsafe { slcd().sdatal1().modify(|r, w| w.bits(r.bits() & !mask)) };
            }
        }
        2 => {
            if on {
                unsafe { slcd().sdatal2().modify(|r, w| w.bits(r.bits() | mask)) };
            } else {
                unsafe { slcd().sdatal2().modify(|r, w| w.bits(r.bits() & !mask)) };
            }
        }
        _ => {}
    }
}

/// Clears all segments of the display, including indicators and the colon.
pub fn clear_display() {
    // SAFETY: writing zero to the segment data registers is always valid.
    unsafe {
        slcd().sdatal0().write(|w| w.bits(0));
        slcd().sdatal1().write(|w| w.bits(0));
        slcd().sdatal2().write(|w| w.bits(0));
    }
}

/// Displays a single character at the given position (0-9).
pub fn display_character(character: u8, position: u8) {
    let mut character = character;

    // Special cases for positions 4 and 6.
    if position == 4 || position == 6 {
        if character == b'7' {
            character = b'&';
        } else if character == b'A' {
            character = b'a';
        } else if character == b'o' {
            character = b'O';
        } else if character == b'L' {
            character = b'!';
        } else if character == b'M' || character == b'm' || character == b'N' {
            character = b'n';
        } else if character == b'c' {
            character = b'C';
        } else if character == b'J' {
            character = b'j';
        } else if character == b't' || character == b'T' {
            character = b'+';
        } else if character == b'y' || character == b'Y' {
            character = b'4';
        } else if character == b'v'
            || character == b'V'
            || character == b'U'
            || character == b'W'
            || character == b'w'
        {
            character = b'u';
        }
    } else {
        if character == b'u' {
            character = b'v';
        } else if character == b'j' {
            character = b'J';
        }
    }
    if position > 1 && character == b'T' {
        character = b't';
    }
    if position == 1 {
        if character == b'a' {
            character = b'A';
        } else if character == b'o' {
            character = b'O';
        } else if character == b'i' {
            character = b'l';
        } else if character == b'n' {
            character = b'N';
        } else if character == b'r' {
            character = b'R';
        } else if character == b'd' {
            character = b'D';
        } else if character == b'v' || character == b'V' || character == b'u' {
            character = b'U';
        } else if character == b'b' {
            character = b'B';
        } else if character == b'c' {
            character = b'C';
        }
    } else {
        if character == b'R' {
            character = b'r';
        }
    }
    if position == 0 {
        clear_pixel(0, 15); // clear funky ninth segment
    } else {
        if character == b'I' {
            character = b'l';
        }
    }

    let segmap = SEGMENT_MAP[position as usize];
    let segdata = CHARACTER_SET[(character - 0x20) as usize];

    for i in 0..8 {
        let com = ((segmap >> (i * 8)) & 0xFF) >> 6;
        if com > 2 {
            // COM3 means no segment exists; skip it.
            continue;
        }
        let seg = (segmap >> (i * 8)) & 0x3F;
        if (segdata >> i) & 1 != 0 {
            set_pixel(com as u8, seg as u8);
        } else {
            clear_pixel(com as u8, seg as u8);
        }
    }

    if character == b'T' && position == 1 {
        set_pixel(1, 12); // add descender
    } else if position == 0 && (character == b'B' || character == b'D' || character == b'@') {
        set_pixel(0, 15); // add funky ninth segment
    } else if position == 1 && (character == b'B' || character == b'D' || character == b'@') {
        set_pixel(0, 12); // add funky ninth segment
    }
}

/// Displays a string at the given position (0-9). A space clears that digit.
pub fn display_string(string: &str, position: u8) {
    let bytes = string.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        display_character(bytes[i], position + i as u8);
        i += 1;
        if position + i as u8 >= NUM_CHARS {
            break;
        }
    }
}

/// Turns the colon segment on.
pub fn set_colon() {
    set_pixel(1, 16);
}

/// Turns the colon segment off.
pub fn clear_colon() {
    clear_pixel(1, 16);
}

/// Sets an indicator segment on the LCD.
pub fn set_indicator(indicator: Indicator) {
    let (com, seg) = INDICATOR_SEGMENTS[indicator as usize];
    set_pixel(com, seg);
}

/// Clears an indicator segment on the LCD.
pub fn clear_indicator(indicator: Indicator) {
    let (com, seg) = INDICATOR_SEGMENTS[indicator as usize];
    clear_pixel(com, seg);
}

/// Clears all indicator segments.
pub fn clear_all_indicators() {
    clear_pixel(2, 17);
    clear_pixel(2, 16);
    clear_pixel(0, 17);
    clear_pixel(0, 16);
    clear_pixel(1, 10);
}

/// Starts blinking a single character in position 7.
pub fn start_character_blink(character: u8, duration: u32) {
    slcd().ctrld().modify(|_, w| w.fc0en().clear_bit());
    sync();

    // Set the frame counter 0 overflow value.
    let frames = duration / (1000 / FRAME_FREQUENCY);
    if duration <= FC_BYPASS_MAX_MS {
        // SAFETY: computed overflow value is valid.
        unsafe {
            slcd()
                .fc0()
                .write(|w| w.pb().set_bit().ovf().bits((frames - 1) as u8));
        }
    } else {
        // SAFETY: computed overflow value is valid.
        unsafe {
            slcd()
                .fc0()
                .write(|w| w.ovf().bits(((frames / 8) - 1) as u8));
        }
    }
    slcd().ctrld().modify(|_, w| w.fc0en().set_bit());

    display_character(character, 7);
    clear_pixel(2, 10); // clear segment B of position 7 since it can't blink

    slcd().ctrld().modify(|_, w| w.blink().clear_bit());
    slcd().ctrla().modify(|_, w| w.enable().clear_bit());
    sync();

    // SAFETY: 0x07 is a valid blink-segment-selection value.
    unsafe {
        slcd().bcfg().modify(|_, w| {
            w.bss0().bits(0x07);
            w.bss1().bits(0x07)
        });
    }

    slcd().ctrld().modify(|_, w| w.blink().set_bit());
    sync();
    slcd().ctrla().modify(|_, w| w.enable().set_bit());
    sync();
}

/// Stops and clears all blinking segments.
pub fn stop_blink() {
    slcd().ctrld().modify(|_, w| {
        w.fc0en().clear_bit();
        w.blink().clear_bit()
    });
}

/// Starts a two-segment "tick-tock" animation in position 8.
pub fn start_tick_animation(duration: u32) {
    display_character(b' ', 8);
    start_animation(&[(0, 2)], duration);
}

/// Checks if the tick animation is currently running.
pub fn tick_animation_is_running() -> bool {
    slcd().ctrld().read().csren().bit_is_set()
}

/// Stops the tick/tock animation and clears all animating segments.
pub fn stop_tick_animation() {
    stop_animation();
    display_character(b' ', 8);
}

/// Starts a circular-shift-register animation on the given segments.
/// Port of `_slcd_sync_start_animation`.
fn start_animation(segs: &[(u8, u8)], period: u32) {
    // Set the animation period using frame counter 1.
    slcd().ctrld().modify(|_, w| w.fc1en().clear_bit());
    sync();
    let frames = period / (1000 / FRAME_FREQUENCY);
    if period <= FC_BYPASS_MAX_MS {
        // SAFETY: computed overflow value is valid.
        unsafe {
            slcd()
                .fc1()
                .write(|w| w.pb().set_bit().ovf().bits((frames - 1) as u8));
        }
    } else {
        // SAFETY: computed overflow value is valid.
        unsafe {
            slcd()
                .fc1()
                .write(|w| w.ovf().bits(((frames / 8) - 1) as u8));
        }
    }
    slcd().ctrld().modify(|_, w| w.fc1en().set_bit());

    // Set animation segments.
    slcd().ctrla().modify(|_, w| w.enable().clear_bit());
    slcd().ctrld().modify(|_, w| w.csren().clear_bit());
    sync();

    let mut csrlen = 0;
    for &(com, seg) in segs {
        let idx = (com as u32 * 2) + (seg as u32 - 2);
        if idx > csrlen {
            csrlen = idx;
        }
        // SAFETY: computed CSRCFG data value is valid.
        unsafe {
            slcd().csrcfg().modify(|r, w| w.bits(r.bits() | (1 << idx)));
        }
    }
    // SAFETY: computed CSRCFG size value is valid.
    unsafe {
        slcd()
            .csrcfg()
            .modify(|r, w| w.bits(r.bits() | ((csrlen + 1) << 8)));
    }
    slcd().bcfg().modify(|_, w| w.mode().blinksel());
    slcd().ctrld().modify(|_, w| w.csren().set_bit());
    slcd().ctrla().modify(|_, w| w.enable().set_bit());
}

/// Stops the circular-shift-register animation. Port of `_slcd_sync_stop_animation`.
fn stop_animation() {
    sync();
    slcd().ctrld().modify(|_, w| w.csren().clear_bit());
}

/// SLCD frame frequency (Hz), computed from the reference config:
/// 32768 / (((PRESC+1)*16) * (CKDIV+1) * (COM_NUM+1))
/// = 32768 / ((3*16) * 5 * 3) = 32768 / 720 = 45.5 Hz
const FRAME_FREQUENCY: u32 = 45;

/// Frame counter constants (ms), from the reference config.
const FC_MAX_MS: u32 = ((0x1F + 1) * 8) * (1000 / FRAME_FREQUENCY);
const FC_MIN_MS: u32 = 1000 / FRAME_FREQUENCY;
const FC_BYPASS_MAX_MS: u32 = (0x1F + 1) * (1000 / FRAME_FREQUENCY);
