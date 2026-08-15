# Test coverage and integration contract audit

**Audit date:** 2026-08-14  
**Scope:** workspace crates under `sensor-watch-rs`  
**Change policy:** this audit adds documentation only. No private files, reference
repositories, firmware implementation files, or generated artifacts were changed.

## Reproducible baseline

The following host commands were run from the repository root:

```text
cargo test --workspace --all-targets --no-fail-fast
cargo test -p sensor-watch --lib --features hostmock,std --no-fail-fast
cargo test -p sensor-watch --lib --features usb-cdc --no-fail-fast
cargo test -p sensor-watch --lib --features hostmock,std,optical,shell-auth,usb-cdc --no-fail-fast
```

The latest validated standard workspace test run reported:

| Package/path | Tests | Boundary covered |
|---|---:|---|
| `sensor-watch` host seam (`hostmock,std`) | 121 | Real firmware-face and host HAL seam behavior |
| `sensor-watch-core` | 69 | Pure date/time, settings, UF2, ECC, transfer, optical, and safety logic |
| `sensor-watch-studio` default features | 145 | Studio default path, including the default `real-faces` bridge |
| `sensor-watch-studio` `--no-default-features` | 121 | Covered fallback/simulator path; passing |
| `sensor-watch-tools` | 30 | Tools library and CLI behavior |
| `sensor-watch` (`usb-cdc` only) | 1 | USB CDC descriptor-contract coverage |
| `sensor-watch` (`hostmock,std,optical,shell-auth,usb-cdc`) | 122 | Combined firmware host-contract coverage |
| **Workspace aggregate** | **365** | Host/software coverage only |

The separate command-scoped firmware totals are 1 test for `usb-cdc` only
and 122 tests for `hostmock,std,optical,shell-auth,usb-cdc`; these are not
additional tests to add to the 365-test workspace aggregate.

No command in this audit validates real watch silicon, USB enumeration, UART
wiring, SWD probe behavior, power draw, or physical display/peripheral behavior.

## Concrete gaps found

### 1. Studio `real-faces` disabled mode is covered

`studio/Cargo.toml` documents `real-faces` as optional, and
`studio/src/real_face.rs` documents a fallback implementation that returns
`None` so the simulator can use `face_sim`. The current reproducible command:

```text
cargo test -p sensor-watch-studio --no-default-features
```

passes, and CI now enforces this no-default-features path immediately after
the default Studio test.

Historical context: an earlier audit revision recorded this command as failing
before tests ran because the fallback methods returned `RealFaceSnapshot` while
that type was only defined under `#[cfg(feature = "real-faces")]`. That stale
failure report no longer describes the current status.

### 2. Feature-contract coverage status

CI now makes the documented firmware host contracts explicit. The
`firmware-host` job checks the `hostmock` compile path without `std`, runs the
`hostmock,std` seam tests, runs the `hostmock,std,optical` tests, exercises
shell authorization behavior with `hostmock,std,shell-auth`, and runs the
combined `hostmock,std,optical,shell-auth,usb-cdc` contract tests.

The `shell-auth` feature remains a marker for integrations that provide an
explicit physical-presence/auth hook: no source `cfg(feature = "shell-auth")`
branch was found, so the CI check covers host shell behavior rather than a
feature-specific implementation. The existing USB CDC contract remains
metadata-only and does not validate controller transfers, enumeration,
suspend/resume, or power behavior, consistent with `docs/USB_CDC.md`.

ARM compile coverage for `defmt-log` is kept as a separate ARM check. The
feature is explicitly ARM-only; host use is rejected by a compile error. This
ARM check still does not validate SWD/RTT probe behavior.

### 3. Claimed behaviors with no host substitute

The following status claims remain software-only or unexecuted, rather than
being covered by the passing test counts:

- real silicon boot, button/LCD/LED/buzzer/accelerometer/I2C behavior;
- current draw, RTC drift, brown-out, watchdog reset, and clock-failure recovery;
- USB CDC device enumeration and endpoint transfers;
- UART jig wiring and SWD/RTT probe behavior;
- Studio configured UF2 generation with user selections. The path is correctly
  fail-closed until selections become firmware build inputs;
- optical receiver integration. The protocol framing and authentication logic
  are host-testable, but no receiver integration is claimed.

## Stale documentation corrected by this audit

- The README baseline of `106 + 67 + 90 + 16 = 279` and the later audit
  snapshot of `109 + 67 + 94 + 14 = 284` are historical totals, not the current
  validation contract. The latest standard workspace run is `121 + 69 + 145 +
  30 = 365`; the separate Studio no-default-features run reports 121 tests.
  The command-scoped firmware runs report 1 test for `usb-cdc` only and 122
  tests for `hostmock,std,optical,shell-auth,usb-cdc`.
- The current Studio mapping uses 97 real firmware faces and 14 `face_sim`
  fallback faces out of 111.
- `PROJECT_LOG.md` contains historical snapshots that are intentionally not
  part of the public validation baseline. Those entries are retained as dated
  history, not treated as the current validation contract.

## Recommended follow-up gates

1. Keep the CI Studio `--no-default-features` gate and add a deterministic
   fallback test asserting `RealFace::new("SIMPLE_CLOCK") == None` if that
   behavior needs an explicit regression contract.
2. Keep the CI feature-contract checks for `hostmock`, `std`, `optical`,
   `shell-auth`, and `usb-cdc`, distinguishing host contract tests from the
   separate ARM `defmt-log` compile check.
3. Keep physical validation results in `docs/TESTING.md` separate from host
   test counts and record each as PASS, FAIL, NOT AVAILABLE, or NOT TESTED.
