//! NTP time synchronization.
//!
//! A simplified port of the master-clock NTP client: select a server, query it,
//! and get the current UTC time. Used by the dashboard for time tracking and by
//! the auto-calibration flow.

use std::net::UdpSocket;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// NTP port.
const NTP_PORT: u16 = 123;
/// Seconds between the NTP epoch (1900) and the Unix epoch (1970).
const NTP_TIMESTAMP_DELTA: u64 = 2_208_988_800;

/// A curated list of NTP servers. Cloudflare is first (trusted, stratum 1,
/// no leap smearing) and is the default manual-fetch selection.
pub const SERVERS: [(&str, &str); 8] = [
    ("Cloudflare", "time.cloudflare.com"),
    ("Google", "time.google.com"),
    ("Microsoft", "time.windows.com"),
    ("Apple", "time.apple.com"),
    ("NIST (USA)", "time.nist.gov"),
    ("Global Pool", "pool.ntp.org"),
    ("Europe Pool", "europe.pool.ntp.org"),
    ("Asia Pool", "asia.pool.ntp.org"),
];

/// Returns the weekday for a Unix timestamp using the firmware convention
/// `0=Sunday..6=Saturday`. Unix day zero was Thursday, so the offset is four.
pub fn weekday_from_unix_seconds(unix_seconds: u64) -> u32 {
    (((unix_seconds / 86_400) + 4) % 7) as u32
}

/// Formats the exact shell command for setting a UTC Unix timestamp.
///
/// The shell accepts `settime YYMMDDHHMMSS`; callers should send it at the
/// displayed boundary through the UART jig/debug pads.
pub fn settime_command(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64;
    let rem = unix_seconds % 86_400;
    let (year, month, day) = crate::watch_sim::civil_from_days(days);
    format!(
        "settime {:02}{:02}{:02}{:02}{:02}{:02}",
        (year.rem_euclid(100)) as u32,
        month,
        day,
        rem / 3_600,
        (rem / 60) % 60,
        rem % 60
    )
}

/// The result of an NTP query.
pub struct NtpResult {
    /// The server's reported UTC time (seconds since the Unix epoch).
    pub unix_seconds: u64,
    /// Round-trip time in milliseconds.
    pub ping_ms: f64,
    /// Estimated clock offset in seconds (server vs local).
    pub offset_secs: f64,
}

/// Build a 48-byte NTP request packet (version 4, client mode).
fn build_request_packet(transmit_time: f64) -> [u8; 48] {
    let mut packet = [0u8; 48];
    // LI=0 (2 bits), VN=4 (3 bits), Mode=3 (3 bits) => 0b001_000_11 = 0x23
    packet[0] = 0x23;
    write_ntp_timestamp(&mut packet[40..48], transmit_time);
    packet
}

fn write_ntp_timestamp(out: &mut [u8], timestamp: f64) {
    let seconds = timestamp.floor().max(0.0) as u64;
    let fraction =
        ((timestamp.fract().clamp(0.0, 1.0) * (1u64 << 32) as f64) as u64).min(u32::MAX as u64);
    out[..4].copy_from_slice(&(seconds as u32).to_be_bytes());
    out[4..].copy_from_slice(&(fraction as u32).to_be_bytes());
}

/// Parse a 64-bit NTP timestamp (seconds + fraction) at a given byte offset.
fn parse_ntp_timestamp(packet: &[u8], offset: usize) -> (u64, u32) {
    let seconds = u32::from_be_bytes([
        packet[offset],
        packet[offset + 1],
        packet[offset + 2],
        packet[offset + 3],
    ]);
    let fraction = u32::from_be_bytes([
        packet[offset + 4],
        packet[offset + 5],
        packet[offset + 6],
        packet[offset + 7],
    ]);
    (seconds as u64, fraction)
}

/// Convert an NTP (seconds, fraction) pair to fractional seconds since 1900.
fn ntp_fractional(seconds: u64, fraction: u32) -> f64 {
    seconds as f64 + fraction as f64 / (1u64 << 32) as f64
}

/// Convert fractional NTP seconds (since 1900) to fractional Unix seconds.
fn ntp_to_unix_fractional(ntp: f64) -> f64 {
    ntp - NTP_TIMESTAMP_DELTA as f64
}

fn validate_response_header(packet: &[u8]) -> Result<(), String> {
    // LI=3 means the server is unsynchronized. Stratum 0 is reserved for a
    // Kiss-o'-Death response, and strata above 15 are invalid.
    if (packet[0] >> 6) == 3 {
        return Err("NTP server is unsynchronized".to_string());
    }
    if !(1..=15).contains(&packet[1]) {
        return Err("NTP server returned an invalid stratum".to_string());
    }
    let version = (packet[0] >> 3) & 0x07;
    if !(3..=4).contains(&version) {
        return Err("Invalid NTP reply version".to_string());
    }
    if (packet[0] & 0x07) != 4 {
        return Err("Invalid NTP reply mode (expected server mode)".to_string());
    }
    Ok(())
}

/// Current local time as fractional NTP seconds (seconds since 1900).
fn local_to_ntp_seconds() -> f64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    now + NTP_TIMESTAMP_DELTA as f64
}

/// Query an NTP server for the current time.
///
/// Uses the classic four-timestamp NTP offset algorithm:
///   offset = ((T2 - T1) + (T3 - T4)) / 2
/// where T1 = our send time, T2 = server receive time, T3 = server transmit
/// time, T4 = our receive time. This is far more accurate than a half-RTT
/// estimate and is the standard NTP math.
pub fn query_ntp(server: &str) -> Result<NtpResult, String> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;

    let address = format!("{server}:{NTP_PORT}");
    socket
        .connect(address.as_str())
        .map_err(|e| e.to_string())?;

    // Address setup (bind/connect) is done above so that the T1/T4 timestamps
    // and the ping measurement cover only the actual send/receive exchange.
    let mut buf = [0u8; 1024];
    let t1 = local_to_ntp_seconds();
    let request = build_request_packet(t1);
    let start = Instant::now();
    socket.send(&request).map_err(|e| e.to_string())?;
    let len = socket.recv(&mut buf).map_err(|e| e.to_string())?;
    let ping_ms = start.elapsed().as_secs_f64() * 1000.0;
    let t4 = local_to_ntp_seconds();

    if len < 48 {
        return Err(format!("Short NTP response: {len} bytes"));
    }
    validate_response_header(&buf[..48])?;
    // A response must refer to this request. Without this check, a delayed or
    // unrelated UDP packet can become the clock reference.
    let (origin_seconds, origin_fraction) = parse_ntp_timestamp(&buf[..48], 24);
    let mut expected_origin = [0u8; 8];
    write_ntp_timestamp(&mut expected_origin, t1);
    if origin_seconds != u32::from_be_bytes(expected_origin[..4].try_into().unwrap()) as u64
        || origin_fraction != u32::from_be_bytes(expected_origin[4..].try_into().unwrap())
    {
        return Err("NTP response did not match the request".to_string());
    }

    // T2 = receive timestamp (bytes 32..40), T3 = transmit (bytes 40..48).
    let (r1, r2) = parse_ntp_timestamp(&buf[..48], 32);
    let (x1, x2) = parse_ntp_timestamp(&buf[..48], 40);
    let t2 = ntp_fractional(r1, r2);
    let t3 = ntp_fractional(x1, x2);

    // Require valid T2/T3: nonzero seconds (the mode/era bit check is handled
    // by the Mode=4 reply check above), so empty or garbage timestamps are
    // rejected rather than feeding the offset math.
    if r1 == 0 || x1 == 0 {
        return Err("NTP server returned invalid receive/transmit timestamp".to_string());
    }

    // Sanity: the server's transmit timestamp must be close to our local NTP
    // time. This rejects garbage (e.g. the 1900 epoch) without hardcoding a
    // year. Allow up to 10 years of slop (well beyond any sane offset).
    let deviation = (t3 - t4).abs();
    if deviation > 10.0 * 365.25 * 86400.0 {
        return Err("NTP server returned implausible timestamp".to_string());
    }

    // Server processing delay (T3 - T2) must be non-negative and tiny. Systems
    // usually reply in microseconds; a negative or >1s delay means bad/fake data.
    let server_delay = t3 - t2;
    if !(0.0..=1.0).contains(&server_delay) {
        return Err("NTP server returned implausible processing delay".to_string());
    }

    // Offset = ((T2 - T1) + (T3 - T4)) / 2 (all in NTP seconds since 1900).
    let offset_secs = ((t2 - t1) + (t3 - t4)) / 2.0;

    let unix_time = ntp_to_unix_fractional(t3);
    if !unix_time.is_finite() || unix_time < 0.0 {
        return Err("NTP server returned an invalid Unix timestamp".to_string());
    }
    let unix_seconds = unix_time.floor() as u64;
    Ok(NtpResult {
        unix_seconds,
        ping_ms,
        offset_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_settime_command_at_boundary() {
        assert_eq!(settime_command(1_672_531_200), "settime 230101000000");
    }

    #[test]
    fn formats_two_digit_year() {
        assert_eq!(settime_command(1_704_067_200), "settime 240101000000");
    }

    #[test]
    fn uses_unix_epoch_weekday_offset() {
        assert_eq!(weekday_from_unix_seconds(0), 4); // Thursday
        assert_eq!(weekday_from_unix_seconds(86_400 * 3), 0); // Sunday
    }

    #[test]
    fn rejects_unsynchronized_server_replies() {
        let mut packet = [0u8; 48];
        packet[0] = 0xc4; // LI=3, server mode
        packet[1] = 2;

        assert_eq!(
            validate_response_header(&packet),
            Err("NTP server is unsynchronized".to_string())
        );
    }

    #[test]
    fn rejects_kiss_of_death_stratum_zero_replies() {
        let mut packet = [0u8; 48];
        packet[0] = 0x24; // LI=0, version 4, server mode

        assert_eq!(
            validate_response_header(&packet),
            Err("NTP server returned an invalid stratum".to_string())
        );
    }

    #[test]
    fn rejects_invalid_versions_and_strata() {
        let mut packet = [0u8; 48];
        packet[0] = 0x04; // version 0, server mode
        packet[1] = 1;
        assert_eq!(
            validate_response_header(&packet),
            Err("Invalid NTP reply version".to_string())
        );

        packet[0] = 0x24; // version 4, server mode
        packet[1] = 16;
        assert_eq!(
            validate_response_header(&packet),
            Err("NTP server returned an invalid stratum".to_string())
        );
    }

    #[test]
    fn request_carries_a_matching_transmit_timestamp() {
        let packet = build_request_packet(2_208_988_800.25);
        assert_eq!(&packet[40..44], &2_208_988_800u32.to_be_bytes());
        assert_eq!(&packet[44..48], &1_073_741_824u32.to_be_bytes());
    }
}
