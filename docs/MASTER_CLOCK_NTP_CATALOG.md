# Master Clock NTP catalog import

Studio has an optional **Import Master Clock server list** action beside the
Dashboard custom NTP server controls. It is an offline file import: it does not
launch Master Clock, inspect its runtime memory, resolve hosts, or query NTP.
Importing never selects a server and never fetches time. Use Studio's explicit
**Fetch time** action when a network query is wanted.

## Neutral JSON interchange format

Use this documented, neutral format when a Master Clock persistence/export
surface is not available. It is a template, not a copy of Master Clock source
or data:

```json
{
  "format": "sensor-watch-master-clock-ntp-v1",
  "servers": [
    {
      "name": "Example time service",
      "hostname": "time.example.com",
      "ip": "192.0.2.1"
    },
    {
      "name": "Example IPv6 service",
      "hostname": "time6.example.com"
    }
  ]
}
```

`format` and `servers` are required. Each server has a name, a DNS hostname,
and an optional literal IP address. Hostnames and IPs must contain no ports,
whitespace, or control characters; hostnames use DNS label syntax and IPs must
parse as IPv4 or IPv6. Unknown JSON properties are rejected. The document is
bounded to 64 KiB and the persisted custom-server list is bounded to 64
entries.

Entries with invalid fields are **rejected**. Hostname duplicates (including
case differences) and entries already in Studio are **skipped**. Valid new
entries are **imported** together in one transactional persistence operation;
if saving fails, the in-memory import is rolled back. Studio displays imported,
skipped, and rejected counts. An optional `ip` is validated for interchange
safety but Studio's current settings schema preserves only the name and
hostname, so it is not used for selection or transport.

NTP is a network operation using UDP port 123 and can disclose the selected
hostname and the device/network's normal resolver and routing metadata to the
network. Master Clock may have separate geolocation and time-setting behavior;
this import does not invoke or reproduce those behaviors. Review the source and
license of any catalog before importing it.
