//! TOTP (LFS) watch face.
//!
//! Port of the C `totp_face_lfs.c`. A variant of the TOTP face that reads its
//! credentials from a configurable list. It is a pure state machine: it reacts
//! to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd;
use crate::watch::utility;

const MAX_TOTP_SECRET_SIZE: usize = 128;

/// A TOTP record.
struct TotpRecord {
    label: &'static str,
    secret: &'static str,
    period: u8,
}

/// The configured records.
const TOTP_RECORDS: [TotpRecord; 2] = [
    TotpRecord {
        label: "AA",
        secret: "JBSWY3DPEHPK3PXP",
        period: 30,
    },
    TotpRecord {
        label: "BB",
        secret: "JBSWY3DPEHPK3PXP",
        period: 30,
    },
];

/// The TOTP LFS face state.
pub struct TotpFaceLfs {
    current_index: usize,
    timestamp: u32,
    steps: u32,
    current_code: u32,
    current_secret: [u8; MAX_TOTP_SECRET_SIZE],
    secret_size: usize,
}

impl TotpFaceLfs {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        TotpFaceLfs {
            current_index: 0,
            timestamp: 0,
            steps: 0,
            current_code: 0,
            current_secret: [0; MAX_TOTP_SECRET_SIZE],
            secret_size: 0,
        }
    }

    pub fn new() -> Self {
        TotpFaceLfs::new_static()
    }

    fn base32_decode(&self, input: &str, output: &mut [u8]) -> usize {
        let mut buffer = 0u32;
        let mut bits_left = 0u32;
        let mut out_len = 0usize;
        for &c in input.as_bytes() {
            let val = match c {
                b'A'..=b'Z' => c - b'A',
                b'2'..=b'7' => c - b'2' + 26,
                _ => continue,
            };
            buffer = (buffer << 5) | val as u32;
            bits_left += 5;
            if bits_left >= 8 {
                bits_left -= 8;
                if out_len < output.len() {
                    output[out_len] = ((buffer >> bits_left) & 0xFF) as u8;
                    out_len += 1;
                }
            }
        }
        out_len
    }

    fn sha1(&self, data: &[u8], out: &mut [u8; 20]) {
        let mut h0: u32 = 0x67452301;
        let mut h1: u32 = 0xEFCDAB89;
        let mut h2: u32 = 0x98BADCFE;
        let mut h3: u32 = 0x10325476;
        let mut h4: u32 = 0xC3D2E1F0;

        let mut msg = [0u8; 128];
        let mut msg_len = 0usize;
        msg[..data.len()].copy_from_slice(data);
        msg_len += data.len();
        let bit_len = (data.len() as u64) * 8;
        msg[msg_len] = 0x80;
        msg_len += 1;
        while msg_len % 64 != 56 {
            msg[msg_len] = 0;
            msg_len += 1;
        }
        msg[msg_len..msg_len + 8].copy_from_slice(&bit_len.to_be_bytes());
        msg_len += 8;

        let mut w = [0u32; 80];
        for chunk in msg[..msg_len].chunks(64) {
            for i in 0..16 {
                w[i] = u32::from_be_bytes([
                    chunk[i * 4],
                    chunk[i * 4 + 1],
                    chunk[i * 4 + 2],
                    chunk[i * 4 + 3],
                ]);
            }
            for i in 16..80 {
                w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
            }
            let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
            for i in 0..80 {
                let (f, k) = match i {
                    0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                    20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                    40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                    _ => (b ^ c ^ d, 0xCA62C1D6),
                };
                let temp = a
                    .rotate_left(5)
                    .wrapping_add(f)
                    .wrapping_add(e)
                    .wrapping_add(k)
                    .wrapping_add(w[i]);
                e = d;
                d = c;
                c = b.rotate_left(30);
                b = a;
                a = temp;
            }
            h0 = h0.wrapping_add(a);
            h1 = h1.wrapping_add(b);
            h2 = h2.wrapping_add(c);
            h3 = h3.wrapping_add(d);
            h4 = h4.wrapping_add(e);
        }
        out[0..4].copy_from_slice(&h0.to_be_bytes());
        out[4..8].copy_from_slice(&h1.to_be_bytes());
        out[8..12].copy_from_slice(&h2.to_be_bytes());
        out[12..16].copy_from_slice(&h3.to_be_bytes());
        out[16..20].copy_from_slice(&h4.to_be_bytes());
    }

    fn hmac_sha1(&self, key: &[u8], data: &[u8], out: &mut [u8; 20]) {
        let block_size = 64usize;
        let mut key_block = [0u8; 64];
        if key.len() > block_size {
            let mut digest = [0u8; 20];
            self.sha1(key, &mut digest);
            key_block[..20].copy_from_slice(&digest);
        } else {
            key_block[..key.len()].copy_from_slice(key);
        }
        let mut ipad = [0u8; 64];
        let mut opad = [0u8; 64];
        for i in 0..64 {
            ipad[i] = key_block[i] ^ 0x36;
            opad[i] = key_block[i] ^ 0x5C;
        }
        let mut inner = [0u8; 128];
        inner[..64].copy_from_slice(&ipad);
        inner[64..64 + data.len()].copy_from_slice(data);
        let mut inner_digest = [0u8; 20];
        self.sha1(&inner[..64 + data.len()], &mut inner_digest);
        let mut outer = [0u8; 84];
        outer[..64].copy_from_slice(&opad);
        outer[64..].copy_from_slice(&inner_digest);
        self.sha1(&outer, out);
    }

    fn set_record(&mut self, i: usize) {
        if TOTP_RECORDS.len() == 0 && i >= TOTP_RECORDS.len() {
            return;
        }
        self.current_index = i;
        let record = &TOTP_RECORDS[i];
        let mut secret = [0u8; MAX_TOTP_SECRET_SIZE];
        self.secret_size = self.base32_decode(record.secret, &mut secret);
        self.current_secret = secret;
        let counter = self.timestamp / record.period as u32;
        let mut counter_bytes = [0u8; 8];
        counter_bytes.copy_from_slice(&counter.to_be_bytes());
        let mut digest = [0u8; 20];
        self.hmac_sha1(&secret[..self.secret_size], &counter_bytes, &mut digest);
        let offset = (digest[19] & 0x0F) as usize;
        let code = ((digest[offset] as u32 & 0x7F) << 24)
            | ((digest[offset + 1] as u32) << 16)
            | ((digest[offset + 2] as u32) << 8)
            | (digest[offset + 3] as u32);
        self.current_code = code % 1_000_000;
        self.steps = self.timestamp / record.period as u32;
    }

    fn display(&mut self) {
        if TOTP_RECORDS.len() == 0 {
            slcd::display_string("No2F Codes", 0);
            return;
        }
        let index = self.current_index;
        let record = &TOTP_RECORDS[index];
        let result = self.timestamp / record.period as u32;
        if result != self.steps {
            self.set_record(index);
        }
        let valid_for = record.period as u32 - (self.timestamp % record.period as u32);
        let mut buf = [0u8; 11];
        let lb = record.label.as_bytes();
        buf[0] = lb[0];
        buf[1] = lb[1];
        buf[2] = b'0' + (valid_for / 10) as u8;
        buf[3] = b'0' + (valid_for % 10) as u8;
        let code = self.current_code;
        buf[4] = b'0' + ((code / 100000) % 10) as u8;
        buf[5] = b'0' + ((code / 10000) % 10) as u8;
        buf[6] = b'0' + ((code / 1000) % 10) as u8;
        buf[7] = b'0' + ((code / 100) % 10) as u8;
        buf[8] = b'0' + ((code / 10) % 10) as u8;
        buf[9] = b'0' + (code % 10) as u8;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

impl WatchFace for TotpFaceLfs {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, settings: &Settings) {
        let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60)
            as u32;
        self.timestamp = utility::date_time_to_unix_time(rtc::get_date_time(), tz);
        self.set_record(0);
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Tick => {
                self.timestamp += 1;
                self.display();
            }
            Event::Activate => self.display(),
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                let n = TOTP_RECORDS.len();
                if n > 0 {
                    self.set_record((self.current_index + 1) % n);
                }
                self.display();
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                let n = TOTP_RECORDS.len();
                if n > 0 {
                    self.set_record((self.current_index + n - 1) % n);
                }
                self.display();
            }
            Event::Button(Button::Alarm, ButtonEvent::Down)
            | Event::Button(Button::Alarm, ButtonEvent::LongPress)
            | Event::Button(Button::Light, ButtonEvent::Down) => {}
            Event::Button(Button::Light, ButtonEvent::LongPress) => movement::illuminate_led(),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
