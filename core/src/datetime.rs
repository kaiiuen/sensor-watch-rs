//! The packed date/time type.
//!
//! Matches the RTC peripheral's CLOCK register bit layout.

/// Reference year for the 6-bit year field (2020 is a leap year, giving us
/// valid dates through 2083).
pub const WATCH_RTC_REFERENCE_YEAR: u16 = 2020;

/// A packed date/time value matching the RTC peripheral's CLOCK register.
///
/// The bit layout mirrors the hardware register:
/// - second: 6 bits (0-59)
/// - minute: 6 bits (0-59)
/// - hour:   5 bits (0-23)
/// - day:    5 bits (1-31)
/// - month:  4 bits (1-12)
/// - year:   6 bits (0-63, representing 2020-2083)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u8,
}

impl DateTime {
    /// Packs the fields into the 32-bit hardware register value.
    pub fn to_reg(self) -> u32 {
        (self.second as u32 & 0x3F)
            | ((self.minute as u32 & 0x3F) << 6)
            | ((self.hour as u32 & 0x1F) << 12)
            | ((self.day as u32 & 0x1F) << 17)
            | ((self.month as u32 & 0x0F) << 22)
            | ((self.year as u32 & 0x3F) << 26)
    }

    /// Unpacks a raw register value into a [`DateTime`].
    pub fn from_reg(reg: u32) -> Self {
        DateTime {
            second: (reg & 0x3F) as u8,
            minute: ((reg >> 6) & 0x3F) as u8,
            hour: ((reg >> 12) & 0x1F) as u8,
            day: ((reg >> 17) & 0x1F) as u8,
            month: ((reg >> 22) & 0x0F) as u8,
            year: ((reg >> 26) & 0x3F) as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_round_trip() {
        let dt = DateTime {
            second: 42,
            minute: 17,
            hour: 23,
            day: 31,
            month: 12,
            year: 3, // 2023
        };
        assert_eq!(DateTime::from_reg(dt.to_reg()), dt);
    }

    #[test]
    fn zero_round_trip() {
        let dt = DateTime {
            second: 0,
            minute: 0,
            hour: 0,
            day: 0,
            month: 0,
            year: 0,
        };
        assert_eq!(dt.to_reg(), 0);
        assert_eq!(DateTime::from_reg(0), dt);
    }

    #[test]
    fn max_values_round_trip() {
        let dt = DateTime {
            second: 59,
            minute: 59,
            hour: 23,
            day: 31,
            month: 12,
            year: 63,
        };
        assert_eq!(DateTime::from_reg(dt.to_reg()), dt);
    }
}
