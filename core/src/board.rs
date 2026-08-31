//! Evidence-backed board and revision mappings shared by firmware and Studio.
//!
//! This is intentionally a hardware description, not a sensor inventory. A
//! mapping may be returned for review while `validate()` rejects ownership
//! conflicts before it can be used to generate or apply hardware settings.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardId {
    Green,
    RedLite,
    Blue,
    Pro,
}

impl BoardId {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Green => "Green",
            Self::RedLite => "Red / Lite",
            Self::Blue => "Blue",
            Self::Pro => "Pro",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevisionId {
    SwatA1_02,
    SwatA1_05,
    FealA1_00,
    SwatC1_00,
}

impl RevisionId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SwatA1_02 => "OSO-SWAT-A1-02",
            Self::SwatA1_05 => "OSO-SWAT-A1-05",
            Self::FealA1_00 => "OSO-FEAL-A1-00",
            Self::SwatC1_00 => "OSO-SWAT-C1-00",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "OSO-SWAT-A1-02" => Some(Self::SwatA1_02),
            "OSO-SWAT-A1-05" => Some(Self::SwatA1_05),
            "OSO-FEAL-A1-00" => Some(Self::FealA1_00),
            "OSO-SWAT-C1-00" => Some(Self::SwatC1_00),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinId {
    pub port: u8,
    pub number: u8,
}

impl PinId {
    pub const fn new(port: u8, number: u8) -> Self {
        Self { port, number }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinAssignment {
    pub owner: &'static str,
    pub pin: PinId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerAssignment {
    pub rail: &'static str,
    pub owner: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinMap {
    pub vbus: Option<PinId>,
    pub power: &'static [PowerAssignment],
    pub buttons: [PinId; 3],
    pub buzzer: PinId,
    pub red_led: PinId,
    pub green_led: Option<PinId>,
    pub blue_led: Option<PinId>,
    pub connector: [PinId; 5],
    pub i2c: Option<[PinId; 2]>,
    pub interrupt_channels: [u8; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LcdMap {
    pub segments: [PinId; 27],
    pub contrast_adjust: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Verified,
    Documented,
    Unknown,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilitySet {
    pub lcd: Capability,
    pub buzzer: Capability,
    pub thermistor: Capability,
    pub accelerometer: Capability,
    pub light_sensor: Capability,
    pub converter: Capability,
    pub i2c: Capability,
    pub spi: Capability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardMapping {
    pub board: BoardId,
    pub revision: RevisionId,
    pub pins: PinMap,
    pub lcd: LcdMap,
    pub capabilities: CapabilitySet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MappingError {
    UnsupportedTuple,
    PinConflict {
        pin: PinId,
        first: &'static str,
        second: &'static str,
    },
    InterruptConflict {
        channel: u8,
        first: &'static str,
        second: &'static str,
    },
    PowerConflict {
        rail: &'static str,
        first: &'static str,
        second: &'static str,
    },
}

impl BoardMapping {
    /// Detects duplicate physical ownership in the selected mapping.
    pub fn conflicts(&self) -> MappingConflicts {
        let mut result = MappingConflicts {
            pin: None,
            interrupt: None,
            power: None,
        };
        let mut assignments = [PinAssignment {
            owner: "",
            pin: PinId::new(0, 0),
        }; 70];
        let mut count = 0;
        let mut add = |owner: &'static str, pin: PinId| {
            if result.pin.is_some() {
                return;
            }
            for assignment in assignments.iter().take(count) {
                if assignment.pin == pin && assignment.owner != owner {
                    result.pin = Some(MappingError::PinConflict {
                        pin,
                        first: assignment.owner,
                        second: owner,
                    });
                    return;
                }
            }
            if count < assignments.len() {
                assignments[count] = PinAssignment { owner, pin };
                count += 1;
            }
        };
        if let Some(pin) = self.pins.vbus {
            add("vbus", pin);
        }
        for (name, pin) in ["btn_alarm", "btn_light", "btn_mode"]
            .into_iter()
            .zip(self.pins.buttons)
        {
            add(name, pin);
        }
        add("buzzer", self.pins.buzzer);
        add("red_led", self.pins.red_led);
        if let Some(pin) = self.pins.green_led {
            add("green_led", pin);
        }
        if let Some(pin) = self.pins.blue_led {
            add("blue_led", pin);
        }
        for (index, pin) in self.pins.connector.into_iter().enumerate() {
            add(["a0", "a1", "a2", "a3", "a4"][index], pin);
        }
        if let Some(bus) = self.pins.i2c {
            add("i2c_sda", bus[0]);
            add("i2c_scl", bus[1]);
        }
        for (index, pin) in self.lcd.segments.into_iter().enumerate() {
            add(
                [
                    "lcd0", "lcd1", "lcd2", "lcd3", "lcd4", "lcd5", "lcd6", "lcd7", "lcd8", "lcd9",
                    "lcd10", "lcd11", "lcd12", "lcd13", "lcd14", "lcd15", "lcd16", "lcd17",
                    "lcd18", "lcd19", "lcd20", "lcd21", "lcd22", "lcd23", "lcd24", "lcd25",
                    "lcd26",
                ][index],
                pin,
            );
        }
        let mut power = [("", ""); 8];
        for assignment in self.pins.power.iter().copied() {
            for (rail, owner) in power.iter().copied() {
                if rail == assignment.rail && !owner.is_empty() && owner != assignment.owner {
                    result.power = Some(MappingError::PowerConflict {
                        rail: assignment.rail,
                        first: owner,
                        second: assignment.owner,
                    });
                }
            }
            if let Some(slot) = power.iter_mut().find(|slot| slot.0.is_empty()) {
                *slot = (assignment.rail, assignment.owner);
            }
        }
        let mut channels = [(0u8, ""); 8];
        for (name, channel) in ["btn_alarm", "btn_light", "btn_mode"]
            .into_iter()
            .zip(self.pins.interrupt_channels)
        {
            if result.interrupt.is_some() {
                break;
            }
            for (used, owner) in channels.iter().copied().take(8) {
                if used == channel && !owner.is_empty() && owner != name {
                    result.interrupt = Some(MappingError::InterruptConflict {
                        channel,
                        first: owner,
                        second: name,
                    });
                }
            }
            if channel < 8 {
                channels[channel as usize] = (channel, name);
            }
        }
        result
    }

    pub fn validate(&self) -> Result<(), MappingError> {
        let conflicts = self.conflicts();
        conflicts
            .pin
            .or(conflicts.interrupt)
            .or(conflicts.power)
            .map_or(Ok(()), Err)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappingConflicts {
    pub pin: Option<MappingError>,
    pub interrupt: Option<MappingError>,
    pub power: Option<MappingError>,
}

const fn p(port: u8, number: u8) -> PinId {
    PinId::new(port, number)
}

const SWAT_LCD: [PinId; 27] = [
    p(1, 6),
    p(1, 7),
    p(1, 8),
    p(1, 9),
    p(0, 4),
    p(0, 5),
    p(0, 6),
    p(0, 7),
    p(0, 8),
    p(0, 9),
    p(0, 10),
    p(0, 11),
    p(1, 11),
    p(1, 12),
    p(1, 13),
    p(1, 14),
    p(1, 15),
    p(0, 12),
    p(0, 13),
    p(0, 14),
    p(0, 15),
    p(0, 16),
    p(0, 17),
    p(0, 18),
    p(0, 19),
    p(1, 16),
    p(1, 17),
];
const LITE_LCD: [PinId; 27] = [
    p(1, 6),
    p(1, 7),
    p(1, 8),
    p(1, 9),
    p(0, 5),
    p(0, 6),
    p(0, 8),
    p(0, 9),
    p(0, 10),
    p(0, 11),
    p(1, 11),
    p(1, 12),
    p(1, 13),
    p(1, 14),
    p(1, 15),
    p(0, 14),
    p(0, 15),
    p(0, 16),
    p(0, 17),
    p(0, 18),
    p(0, 19),
    p(1, 16),
    p(1, 17),
    p(0, 20),
    p(0, 21),
    p(0, 22),
    p(0, 23),
];

const fn swat_pins(buttons: [PinId; 3], channels: [u8; 3], red: PinId, green: PinId) -> PinMap {
    PinMap {
        vbus: Some(p(1, 5)),
        power: &[],
        buttons,
        buzzer: p(0, 27),
        red_led: red,
        green_led: Some(green),
        blue_led: None,
        connector: [p(1, 4), p(1, 1), p(1, 2), p(1, 3), p(1, 0)],
        i2c: Some([p(1, 30), p(1, 31)]),
        interrupt_channels: channels,
    }
}

pub const GREEN: BoardMapping = BoardMapping {
    board: BoardId::Green,
    revision: RevisionId::SwatA1_05,
    pins: swat_pins([p(0, 2), p(0, 22), p(0, 23)], [2, 6, 7], p(0, 20), p(0, 21)),
    lcd: LcdMap {
        segments: SWAT_LCD,
        contrast_adjust: None,
    },
    capabilities: CapabilitySet {
        lcd: Capability::Verified,
        buzzer: Capability::Verified,
        thermistor: Capability::Unknown,
        accelerometer: Capability::Unknown,
        light_sensor: Capability::Unknown,
        converter: Capability::Unknown,
        i2c: Capability::Documented,
        spi: Capability::Documented,
    },
};

pub const BLUE: BoardMapping = BoardMapping {
    board: BoardId::Blue,
    revision: RevisionId::SwatA1_05,
    pins: swat_pins([p(0, 2), p(0, 22), p(0, 23)], [2, 6, 7], p(0, 21), p(0, 20)),
    lcd: LcdMap {
        segments: SWAT_LCD,
        contrast_adjust: None,
    },
    capabilities: GREEN.capabilities,
};

pub const PRO_FEAL: BoardMapping = BoardMapping {
    board: BoardId::Pro,
    revision: RevisionId::FealA1_00,
    pins: PinMap {
        vbus: Some(p(1, 5)),
        power: &[],
        buttons: [p(0, 2), p(0, 30), p(0, 31)],
        buzzer: p(0, 27),
        red_led: p(0, 12),
        green_led: Some(p(0, 22)),
        blue_led: Some(p(0, 13)),
        connector: [p(1, 4), p(1, 1), p(1, 2), p(1, 3), p(1, 0)],
        i2c: Some([p(1, 30), p(1, 31)]),
        interrupt_channels: [2, 10, 11],
    },
    lcd: LcdMap {
        segments: LITE_LCD,
        contrast_adjust: Some(7),
    },
    capabilities: CapabilitySet {
        lcd: Capability::Documented,
        buzzer: Capability::Documented,
        thermistor: Capability::Unknown,
        accelerometer: Capability::Unknown,
        light_sensor: Capability::Unknown,
        converter: Capability::Unknown,
        i2c: Capability::Documented,
        spi: Capability::Unknown,
    },
};

/// Hardware-only mapping for the headless Lite profile. This is intentionally
/// not a `BoardMapping`: it cannot accidentally expose LCD or production pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiteTestMapping {
    pub board: BoardId,
    pub revision: RevisionId,
    pub red_led: PinId,
    pub green_led: PinId,
    pub leds_active_low: bool,
}

pub const LITE_TEST_RED_A1_02: LiteTestMapping = LiteTestMapping {
    board: BoardId::RedLite,
    revision: RevisionId::SwatA1_02,
    red_led: p(0, 20),
    green_led: p(0, 21),
    leds_active_low: true,
};

pub fn lite_test_lookup(board: BoardId, revision: RevisionId) -> Option<&'static LiteTestMapping> {
    match (board, revision) {
        (BoardId::RedLite, RevisionId::SwatA1_02) => Some(&LITE_TEST_RED_A1_02),
        _ => None,
    }
}

pub const RED_LITE: BoardMapping = BoardMapping {
    board: BoardId::RedLite,
    revision: RevisionId::SwatA1_02,
    pins: PinMap {
        vbus: Some(p(0, 3)),
        power: &[],
        buttons: [p(0, 2), p(1, 5), p(0, 7)],
        buzzer: p(0, 27),
        red_led: p(0, 4),
        green_led: Some(p(1, 23)),
        blue_led: None,
        connector: [p(1, 4), p(1, 1), p(1, 2), p(1, 3), p(1, 0)],
        i2c: None,
        interrupt_channels: [2, 5, 7],
    },
    lcd: LcdMap {
        segments: LITE_LCD,
        contrast_adjust: Some(7),
    },
    capabilities: CapabilitySet {
        lcd: Capability::Verified,
        buzzer: Capability::Documented,
        thermistor: Capability::Documented,
        accelerometer: Capability::Unsupported,
        light_sensor: Capability::Unsupported,
        converter: Capability::Unknown,
        i2c: Capability::Unsupported,
        spi: Capability::Unsupported,
    },
};

pub trait RevisionSelector {
    fn select(self) -> Option<RevisionId>;
}

impl RevisionSelector for RevisionId {
    fn select(self) -> Option<RevisionId> {
        Some(self)
    }
}

impl RevisionSelector for &str {
    fn select(self) -> Option<RevisionId> {
        RevisionId::parse(self)
    }
}

pub fn lookup<R: RevisionSelector>(
    board: BoardId,
    revision: R,
) -> Result<&'static BoardMapping, MappingError> {
    let revision = revision.select().ok_or(MappingError::UnsupportedTuple)?;
    match (board, revision) {
        (BoardId::Green, RevisionId::SwatA1_05) => Ok(&GREEN),
        (BoardId::Blue, RevisionId::SwatA1_05) => Ok(&BLUE),
        (BoardId::RedLite, RevisionId::SwatA1_02) => Ok(&RED_LITE),
        (BoardId::Pro, RevisionId::FealA1_00) => Ok(&PRO_FEAL),
        // C1 is represented by typed revisions but deliberately not
        // selected until a product-level evidence record is added.
        _ => Err(MappingError::UnsupportedTuple),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lite_test_is_red_a1_02_only_and_has_active_low_leds() {
        let mapping = lite_test_lookup(BoardId::RedLite, RevisionId::SwatA1_02).unwrap();
        assert_eq!(mapping.red_led, PinId::new(0, 20));
        assert_eq!(mapping.green_led, PinId::new(0, 21));
        assert!(mapping.leds_active_low);
        assert!(lite_test_lookup(BoardId::Green, RevisionId::SwatA1_05).is_none());
        assert!(lite_test_lookup(BoardId::RedLite, RevisionId::SwatA1_05).is_none());
    }

    #[test]
    fn evidenced_tuples_resolve_and_unknowns_fail_closed() {
        assert!(lookup(BoardId::Green, RevisionId::SwatA1_05).is_ok());
        assert!(lookup(BoardId::Green, "OSO-SWAT-A1-05").is_ok());
        assert!(lookup(BoardId::RedLite, "OSO-SWAT-A1-02").is_ok());
        assert!(lookup(BoardId::Blue, "OSO-SWAT-A1-05").is_ok());
        assert!(lookup(BoardId::Pro, "OSO-FEAL-A1-00").is_ok());
        assert!(lookup(BoardId::Green, "OSO-SWAT-C1-00").is_err());
        assert!(lookup(BoardId::Green, "unknown").is_err());
    }

    #[test]
    fn ownership_conflicts_fail_closed() {
        let mut mapping = *lookup(BoardId::RedLite, "OSO-SWAT-A1-02").unwrap();
        mapping.pins.vbus = Some(mapping.pins.buttons[1]);
        let conflicts = mapping.conflicts();
        assert!(matches!(
            conflicts.pin,
            Some(MappingError::PinConflict { .. })
        ));
        assert!(mapping.validate().is_err());
        assert!(
            lookup(BoardId::RedLite, "OSO-SWAT-A1-02")
                .unwrap()
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn standard_mapping_has_no_pin_or_interrupt_conflicts() {
        assert!(
            lookup(BoardId::Green, "OSO-SWAT-A1-05")
                .unwrap()
                .validate()
                .is_ok()
        );
        assert!(
            lookup(BoardId::Blue, "OSO-SWAT-A1-05")
                .unwrap()
                .validate()
                .is_ok()
        );
    }
}
