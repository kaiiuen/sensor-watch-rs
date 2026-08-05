//! Micro timezone library (port of the C `utz` library).
//!
//! Provides DST-aware timezone offset calculation. Each zone has a base offset
//! and an optional set of daylight-saving rules; `get_current_offset` returns
//! the effective offset for a given date/time, applying DST when it is in
//! effect. This is what makes the watch's timezone handling correct across
//! daylight-saving transitions.

/// Offset increment in minutes (15-minute granularity).
pub const OFFSET_INCREMENT: i32 = 15;
/// Maximum number of rules cached for a zone/year.
const MAX_CURRENT_RULES: usize = 5;
/// Sentinel meaning "this zone does not observe DST".
pub const TIMEZONE_DOES_NOT_OBSERVE: i8 = 127;

/// A time-of-day (hour, minute, second).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UTime {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// A date (year since 2000, month, day-of-month, day-of-week).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UDate {
    pub year: u8,
    pub month: u8,
    pub dayofmonth: u8,
    pub dayofweek: u8,
}

/// A date + time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UDateTime {
    pub date: UDate,
    pub time: UTime,
}

/// A timezone offset (hours + minutes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UOffset {
    pub hours: i8,
    pub minutes: u8,
}

/// A packed timezone definition.
#[derive(Clone, Copy, Debug)]
pub struct UZonePacked {
    pub offset_inc_minutes: i8,
    pub rules_idx: u8,
    pub rules_len: u8,
    pub abrev_formatter: u16,
}

/// A packed DST rule.
#[derive(Clone, Copy, Debug)]
pub struct URulePacked {
    pub from_year: u8,
    pub to_year: u8,
    pub on_dayofweek: u8,
    pub on_dayofmonth: u8,
    pub at_is_local_time: u8,
    pub at_hours: u8,
    pub at_inc_minutes: u8,
    pub letter: u8,
    pub in_month: u8,
    pub offset_hours: u8,
}

/// An unpacked DST rule.
#[derive(Clone, Copy, Debug, Default)]
pub struct URule {
    pub datetime: UDateTime,
    pub is_local_time: u8,
    pub letter: char,
    pub offset_hours: u8,
}

impl URule {
    fn is_valid(&self) -> bool {
        self.letter != '\0'
    }
}

/// An unpacked timezone.
pub struct UZone<'a> {
    pub name: &'a str,
    pub offset: UOffset,
    pub rules: &'a [URulePacked],
    pub abrev_formatter: &'a str,
    pub src: *const UZonePacked,
}

/// Returns the day of the week (Monday = 1, Sunday = 7) for a year/month/day.
pub fn dayofweek(y: u8, m: u8, d: u8) -> u8 {
    const TABLE: [u8; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut y = y as i32;
    let m = m as i32;
    let d = d as i32;
    y -= (m < 3) as i32;
    let year = y + 2000;
    let mut dow =
        (year + year / 4 - year / 100 + 2000 / 400 + TABLE[(m - 1) as usize] as i32 + d) % 7;
    if dow == 0 {
        dow = 7;
    }
    dow as u8
}

/// Returns true if the year (since 2000) is a leap year.
pub fn is_leap_year(y: u8) -> bool {
    let year = y as i32 + 2000;
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Returns the number of days in a month for a given year.
pub fn days_in_month(y: u8, m: u8) -> u8 {
    const DAYS: [u8; 13] = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if m == 2 && is_leap_year(y) {
        DAYS[m as usize] + 1
    } else {
        DAYS[m as usize]
    }
}

/// Returns the offset (0-6) from the current day-of-week to the desired one.
pub fn next_dayofweek_offset(cur: u8, target: u8) -> u8 {
    (7 + target - cur) % 7
}

/// Compares two datetimes: negative if dt1 < dt2, 0 if equal, positive if >.
pub fn udatetime_cmp(dt1: &UDateTime, dt2: &UDateTime) -> i16 {
    let mut ret = dt1.date.year as i16 - dt2.date.year as i16;
    if ret != 0 {
        return ret;
    }
    ret = dt1.date.month as i16 - dt2.date.month as i16;
    if ret != 0 {
        return ret;
    }
    ret = dt1.date.dayofmonth as i16 - dt2.date.dayofmonth as i16;
    if ret != 0 {
        return ret;
    }
    ret = dt1.time.hour as i16 - dt2.time.hour as i16;
    if ret != 0 {
        return ret;
    }
    ret = dt1.time.minute as i16 - dt2.time.minute as i16;
    if ret != 0 {
        return ret;
    }
    dt1.time.second as i16 - dt2.time.second as i16
}

/// Unpacks a packed rule for a given year.
pub fn unpack_rule(rule_in: &URulePacked, cur_year: u8, rule_out: &mut URule) {
    const LETTER_LUT: [char; 3] = ['-', 'S', 'D'];

    rule_out.datetime.date.year = cur_year;
    rule_out.datetime.date.month = rule_in.in_month;

    if rule_in.on_dayofweek == 0 {
        // Format is day-of-month (e.g. 22).
        rule_out.datetime.date.dayofmonth = rule_in.on_dayofmonth;
    } else if rule_in.on_dayofmonth == 0 {
        // Format is last-day-of-week (e.g. lastSun).
        let dow_first = dayofweek(cur_year, rule_in.in_month, 1);
        let first = next_dayofweek_offset(dow_first, rule_in.on_dayofweek);
        rule_out.datetime.date.dayofmonth = 1 + 7 * 3 + first;
        if rule_out.datetime.date.dayofmonth + 7 <= days_in_month(cur_year, rule_in.in_month) {
            rule_out.datetime.date.dayofmonth += 7;
        }
    } else {
        // Format is day-of-week >= day-of-month (e.g. Sun>=22).
        let dow_dom = dayofweek(cur_year, rule_in.in_month, rule_in.on_dayofmonth);
        rule_out.datetime.date.dayofmonth =
            rule_in.on_dayofmonth + next_dayofweek_offset(dow_dom, rule_in.on_dayofweek);
    }

    rule_out.datetime.time.hour = rule_in.at_hours;
    rule_out.datetime.time.minute = rule_in.at_inc_minutes * OFFSET_INCREMENT as u8;
    rule_out.is_local_time = rule_in.at_is_local_time;
    rule_out.letter = LETTER_LUT[(rule_in.letter as usize).min(2)];
    rule_out.offset_hours = rule_in.offset_hours;
}

/// Unpacks the rules active in the current year into `rules_out`.
///
/// `rules_out` must have room for `MAX_CURRENT_RULES` entries. The first entry
/// is the "last rule of the previous year" (start-of-year baseline).
pub fn unpack_rules(
    rules_in: &[URulePacked],
    cur_year: u8,
    rules_out: &mut [URule; MAX_CURRENT_RULES],
) {
    let mut l = 0usize;
    let mut current_rule_count = 1usize;

    for (i, r) in rules_in.iter().enumerate() {
        if current_rule_count >= MAX_CURRENT_RULES {
            break;
        }
        if cur_year >= r.from_year && cur_year <= r.to_year {
            if r.in_month > rules_in[l].in_month {
                l = i;
            }
            unpack_rule(r, cur_year, &mut rules_out[current_rule_count]);
            current_rule_count += 1;
        }
    }

    // Baseline: the "last" rule of the previous year, overridden to start of year.
    unpack_rule(&rules_in[l], cur_year, &mut rules_out[0]);
    rules_out[0].datetime.date.year = cur_year;
    rules_out[0].datetime.date.month = 1;
    rules_out[0].datetime.date.dayofmonth = 1;
    rules_out[0].datetime.time = UTime::default();
}

/// Returns the active rule for a datetime from the cached rules.
pub fn get_active_rule<'a>(
    rules: &'a [URule; MAX_CURRENT_RULES],
    datetime: &UDateTime,
) -> &'a URule {
    for i in 1..MAX_CURRENT_RULES {
        if !rules[i].is_valid() || udatetime_cmp(datetime, &rules[i].datetime) < 0 {
            return &rules[i - 1];
        }
    }
    &rules[MAX_CURRENT_RULES - 1]
}

/// Returns the effective offset for a zone at a datetime, applying DST rules.
///
/// Returns the abbreviation letter ('S' standard, 'D' daylight, '-' none).
pub fn get_current_offset(zone: &UZone, datetime: &UDateTime, offset: &mut UOffset) -> char {
    let mut cached_rules = [URule::default(); MAX_CURRENT_RULES];
    unpack_rules(zone.rules, datetime.date.year, &mut cached_rules);

    offset.minutes = zone.offset.minutes;
    offset.hours = zone.offset.hours;

    if zone.rules.is_empty() {
        return 'S';
    }

    let rule = get_active_rule(&cached_rules, datetime);
    offset.hours += rule.offset_hours as i8;
    rule.letter
}

/// Unpacks a packed zone definition.
pub fn unpack_zone<'a>(
    zone_in: &'a UZonePacked,
    name: &'a str,
    zone_rules: &'a [URulePacked],
    zone_out: &mut UZone<'a>,
) {
    zone_out.src = zone_in;
    zone_out.name = name;
    let inc = zone_in.offset_inc_minutes as i32;
    zone_out.offset.minutes = ((inc % (60 / OFFSET_INCREMENT)) * OFFSET_INCREMENT) as u8;
    zone_out.offset.hours = (inc / (60 / OFFSET_INCREMENT)) as i8;
    zone_out.rules = &zone_rules
        [zone_in.rules_idx as usize..zone_in.rules_idx as usize + zone_in.rules_len as usize];
    zone_out.abrev_formatter = "";
}
