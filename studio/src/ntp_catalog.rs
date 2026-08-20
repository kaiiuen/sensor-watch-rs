//! Offline Master Clock NTP catalog interchange.
//!
//! This is a neutral, bounded JSON format. It deliberately does not discover,
//! launch, or query Master Clock; Studio only reads a user-selected file.

use serde::Deserialize;
use std::collections::HashSet;
use std::net::IpAddr;

pub const MAX_ENTRIES: usize = 64;
pub const MAX_JSON_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogDocument {
    format: String,
    servers: Vec<CatalogServer>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogServer {
    name: String,
    hostname: String,
    #[serde(default)]
    ip: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub imported: usize,
    pub skipped: usize,
    pub rejected: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImportedServer {
    pub name: String,
    pub hostname: String,
}

pub fn parse(
    bytes: &[u8],
    existing: &[(String, String)],
) -> Result<(Vec<ImportedServer>, ImportReport), String> {
    if bytes.len() > MAX_JSON_BYTES {
        return Err(format!("catalog exceeds {MAX_JSON_BYTES} bytes"));
    }
    let document: CatalogDocument = serde_json::from_slice(bytes)
        .map_err(|error| format!("invalid Master Clock catalog JSON: {error}"))?;
    if document.format != "sensor-watch-master-clock-ntp-v1" {
        return Err("unsupported Master Clock catalog format".to_string());
    }

    let mut report = ImportReport::default();
    let mut imported = Vec::new();
    let mut seen = existing
        .iter()
        .map(|(_, hostname)| hostname.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    for server in document.servers {
        if existing.len() + imported.len() >= MAX_ENTRIES {
            report.rejected += 1;
            continue;
        }
        if !valid_name(&server.name)
            || !valid_hostname(&server.hostname)
            || !valid_optional_ip(server.ip.as_deref())
        {
            report.rejected += 1;
            continue;
        }
        let key = server.hostname.to_ascii_lowercase();
        if !seen.insert(key) {
            report.skipped += 1;
            continue;
        }
        imported.push(ImportedServer {
            name: server.name,
            hostname: server.hostname,
        });
        report.imported += 1;
    }
    Ok((imported, report))
}

fn valid_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}

/// Accept DNS hostnames only: no URI syntax, ports, whitespace, or control chars.
fn valid_hostname(value: &str) -> bool {
    if value.is_empty() || value.len() > 253 || value.parse::<IpAddr>().is_ok() {
        return false;
    }
    if value
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || !c.is_ascii())
    {
        return false;
    }
    value.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

fn valid_optional_ip(value: Option<&str>) -> bool {
    value.is_none_or(|ip| {
        !ip.is_empty()
            && !ip.chars().any(|c| c.is_whitespace() || c.is_control())
            && ip.parse::<IpAddr>().is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(servers: &str) -> Vec<u8> {
        format!(r#"{{"format":"sensor-watch-master-clock-ntp-v1","servers":[{servers}]}}"#)
            .into_bytes()
    }

    #[test]
    fn imports_valid_entries_and_skips_case_insensitive_duplicates() {
        let bytes = catalog(
            r#"{"name":"Lab","hostname":"time.example.com","ip":"192.0.2.1"},
               {"name":"Duplicate","hostname":"TIME.EXAMPLE.COM"}"#,
        );
        let (servers, report) = parse(&bytes, &[]).unwrap();
        assert_eq!(servers[0].hostname, "time.example.com");
        assert_eq!(
            report,
            ImportReport {
                imported: 1,
                skipped: 1,
                rejected: 0
            }
        );
    }

    #[test]
    fn rejects_ports_whitespace_bad_hosts_and_bad_ips() {
        let bytes = catalog(
            r#"{"name":"port","hostname":"time.example.com:123"},
               {"name":"space","hostname":"time.example.com "},
               {"name":"ip","hostname":"time.example.com","ip":"192.0.2.1:123"},
               {"name":"control\u0001","hostname":"valid.example.com"}"#,
        );
        let (_, report) = parse(&bytes, &[]).unwrap();
        assert_eq!(report.rejected, 4);
    }

    #[test]
    fn enforces_entry_bound_without_partial_error() {
        let entries = (0..65)
            .map(|i| format!(r#"{{"name":"n{i}","hostname":"host{i}.example.com"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let (_, report) = parse(&catalog(&entries), &[]).unwrap();
        assert_eq!(report.imported, 64);
        assert_eq!(report.rejected, 1);
    }
}
