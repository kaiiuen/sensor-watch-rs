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
/// no leap smearing) and is the default auto-fetch target.
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
fn build_request_packet() -> [u8; 48] {
    let mut packet = [0u8; 48];
    // LI=0 (2 bits), VN=4 (3 bits), Mode=3 (3 bits) => 0b001_000_11 = 0x23
    packet[0] = 0x23;
    packet
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
    let request = build_request_packet();
    socket
        .connect(address.as_str())
        .map_err(|e| e.to_string())?;

    // Address setup (bind/connect) is done above so that the T1/T4 timestamps
    // and the ping measurement cover only the actual send/receive exchange.
    let mut buf = [0u8; 1024];
    let t1 = local_to_ntp_seconds();
    let start = Instant::now();
    socket.send(&request).map_err(|e| e.to_string())?;
    let len = socket.recv(&mut buf).map_err(|e| e.to_string())?;
    let ping_ms = start.elapsed().as_secs_f64() * 1000.0;
    let t4 = local_to_ntp_seconds();

    if len < 48 {
        return Err(format!("Short NTP response: {len} bytes"));
    }
    // LI/VN/Mode byte sanity check: a valid server reply has Mode=4 (server).
    if (buf[0] & 0x07) != 4 {
        return Err("Invalid NTP reply mode (expected server mode)".to_string());
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

    let unix_seconds = ntp_to_unix_fractional(t3) as u64;
    Ok(NtpResult {
        unix_seconds,
        ping_ms,
        offset_secs,
    })
}
