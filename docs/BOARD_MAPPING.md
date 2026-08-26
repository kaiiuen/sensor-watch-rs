# Board and revision mappings

The replaceable mapping model lives in `core/src/board.rs` and is re-exported
by firmware `movement::board`. It is intentionally separate from the persisted
`BoardConfig`: existing board selection and buzzer-voltage storage remain
compatible, while new code can use typed `BoardId`, `RevisionId`, `PinMap`,
`LcdMap`, and `CapabilitySet` values.

## Evidence-backed tuples

| Board identity | Revision | Mapping status |
|---|---|---|
| Green | `OSO-SWAT-A1-05` | Supported mapping |
| Red / Lite | `OSO-SWAT-A1-02` | Supported mapping |
| Blue reference | `OSO-SWAT-A1-05` | Supported mapping using the documented LED swap |
| Pro | `OSO-FEAL-A1-00` | Typed mapping retained for evidence review; not selected by the current firmware lookup policy |
| Any | `OSO-SWAT-C1-00` | Not selected; no product-level support claim |

Unknown revisions and board/revision mismatches return `UnsupportedTuple`.
`BoardMapping::validate()` must be called before a mapping is applied or emitted;
it reports duplicate pin or interrupt ownership instead of guessing. The
validator is also covered by a synthetic conflict test so future mapping edits
cannot silently introduce an overlap.

The capability set does not assert sensor population, converter presence, or
converter support. Those fields remain `Unknown` unless directly evidenced.
The current thermistor evidence used by software profiles is:

- Green: unknown for this board and revision.
- Red / Lite: documented onboard temperature sensor.
- Blue reference: unknown for this board and revision.
- Pro: documented onboard temperature sensor, with nine-pin ownership still
  subject to board validation.

These are documentation and configuration facts, not physical validation. A
thermistor profile does not prove that a sensor is fitted or that its ADC wiring
matches the selected mapping.
The mapping model also does not change USB, UART, optical, RTC/calibration,
package, launcher, or unrelated UI behavior.
