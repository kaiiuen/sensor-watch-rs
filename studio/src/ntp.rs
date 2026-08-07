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

/// Parse the transmit timestamp (bytes 40..48) from an NTP response packet.
fn parse_transmit_timestamp(packet: &[u8]) -> (u64, u32) {
    let seconds = u32::from_be_bytes([packet[40], packet[41], packet[42], packet[43]]);
    let fraction = u32::from_be_bytes([packet[44], packet[45], packet[46], packet[47]]);
    (seconds as u64, fraction)
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

    let origin_ntp = local_to_ntp_seconds();
    let start = Instant::now();
    socket.send(&request).map_err(|e| e.to_string())?;

    let mut buf = [0u8; 1024];
    let len = socket.recv(&mut buf).map_err(|e| e.to_string())?;
    let ping_ms = start.elapsed().as_secs_f64() * 1000.0;
    let destination_ntp = local_to_ntp_seconds();

    if len < 48 {
        return Err(format!("Short NTP response: {len} bytes"));
    }

    let (seconds, fraction) = parse_transmit_timestamp(&buf[..48]);
    let unix = seconds.saturating_sub(NTP_TIMESTAMP_DELTA);
    let millis = (fraction as u64 * 1000) >> 32;

    // Simple offset estimate: server transmit + one-way delay - local receipt.
    let local_receipt = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let server_fractional = unix as f64 + millis as f64 / 1000.0;
    let offset_secs = server_fractional + (ping_ms / 1000.0) / 2.0 - local_receipt;

    let _ = (origin_ntp, destination_ntp);
    Ok(NtpResult {
        unix_seconds: unix,
        ping_ms,
        offset_secs,
    })
}
