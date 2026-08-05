//! Periodic table watch face.
//!
//! Port of the C `periodic_face.c`. Browsers the periodic table of elements,
//! showing each element's symbol, atomic mass, discovery year, electronegativity,
//! and full name. It is a pure state machine: it reacts to a single event and
//! returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd;

const MAX_ELEMENT: u8 = 118;

/// A single element.
struct Element {
    symbol: &'static str,
    name: &'static str,
    year_discovered: i16,
    atomic_mass: u16,
    electronegativity: u16,
    group: &'static str,
}

/// The periodic table (symbol, name, discovery year, mass x100, electronegativity x100, group).
const TABLE: [Element; 118] = [
    Element {
        symbol: "H",
        name: "Hydrogen",
        year_discovered: 1671,
        atomic_mass: 101,
        electronegativity: 220,
        group: "  ",
    },
    Element {
        symbol: "HE",
        name: "Helium",
        year_discovered: 1868,
        atomic_mass: 400,
        electronegativity: 0,
        group: "0",
    },
    Element {
        symbol: "LI",
        name: "Lithium",
        year_discovered: 1817,
        atomic_mass: 694,
        electronegativity: 98,
        group: "1",
    },
    Element {
        symbol: "BE",
        name: "Beryllium",
        year_discovered: 1798,
        atomic_mass: 901,
        electronegativity: 157,
        group: "2",
    },
    Element {
        symbol: "B",
        name: "Boron",
        year_discovered: 1787,
        atomic_mass: 1081,
        electronegativity: 204,
        group: "3",
    },
    Element {
        symbol: "C",
        name: "Carbon",
        year_discovered: -26000,
        atomic_mass: 1201,
        electronegativity: 255,
        group: "4",
    },
    Element {
        symbol: "N",
        name: "Nitrogen",
        year_discovered: 1772,
        atomic_mass: 1401,
        electronegativity: 304,
        group: "5",
    },
    Element {
        symbol: "O",
        name: "Oxygen",
        year_discovered: 1771,
        atomic_mass: 1600,
        electronegativity: 344,
        group: "6",
    },
    Element {
        symbol: "F",
        name: "Fluorine",
        year_discovered: 1771,
        atomic_mass: 1900,
        electronegativity: 398,
        group: "7",
    },
    Element {
        symbol: "NE",
        name: "Neon",
        year_discovered: 1898,
        atomic_mass: 2018,
        electronegativity: 0,
        group: "0",
    },
    Element {
        symbol: "NA",
        name: "Sodium",
        year_discovered: 1702,
        atomic_mass: 2299,
        electronegativity: 93,
        group: "1",
    },
    Element {
        symbol: "MG",
        name: "Magnesium",
        year_discovered: 1755,
        atomic_mass: 2431,
        electronegativity: 131,
        group: "2",
    },
    Element {
        symbol: "AL",
        name: "Aluminium",
        year_discovered: 1746,
        atomic_mass: 2698,
        electronegativity: 161,
        group: "3",
    },
    Element {
        symbol: "SI",
        name: "Silicon",
        year_discovered: 1739,
        atomic_mass: 2809,
        electronegativity: 190,
        group: "4",
    },
    Element {
        symbol: "P",
        name: "Phosphorus",
        year_discovered: 1669,
        atomic_mass: 3097,
        electronegativity: 219,
        group: "5",
    },
    Element {
        symbol: "S",
        name: "Sulfur",
        year_discovered: -2000,
        atomic_mass: 3206,
        electronegativity: 258,
        group: "6",
    },
    Element {
        symbol: "CL",
        name: "Chlorine",
        year_discovered: 1774,
        atomic_mass: 3545,
        electronegativity: 316,
        group: "7",
    },
    Element {
        symbol: "AR",
        name: "Argon",
        year_discovered: 1894,
        atomic_mass: 3995,
        electronegativity: 0,
        group: "0",
    },
    Element {
        symbol: "K",
        name: "Potassium",
        year_discovered: 1702,
        atomic_mass: 3910,
        electronegativity: 82,
        group: "1",
    },
    Element {
        symbol: "CA",
        name: "Calcium",
        year_discovered: 1739,
        atomic_mass: 4008,
        electronegativity: 100,
        group: "2",
    },
    Element {
        symbol: "SC",
        name: "Scandium",
        year_discovered: 1879,
        atomic_mass: 4496,
        electronegativity: 136,
        group: " T",
    },
    Element {
        symbol: "TI",
        name: "Titanium",
        year_discovered: 1791,
        atomic_mass: 4787,
        electronegativity: 154,
        group: " T",
    },
    Element {
        symbol: "V",
        name: "Vanadium",
        year_discovered: 1801,
        atomic_mass: 5094,
        electronegativity: 163,
        group: " T",
    },
    Element {
        symbol: "CR",
        name: "Chromium",
        year_discovered: 1797,
        atomic_mass: 5200,
        electronegativity: 166,
        group: " T",
    },
    Element {
        symbol: "MN",
        name: "Manganese",
        year_discovered: 1774,
        atomic_mass: 5494,
        electronegativity: 155,
        group: " T",
    },
    Element {
        symbol: "FE",
        name: "Iron",
        year_discovered: -5000,
        atomic_mass: 5585,
        electronegativity: 183,
        group: " T",
    },
    Element {
        symbol: "CO",
        name: "Cobalt",
        year_discovered: 1735,
        atomic_mass: 5893,
        electronegativity: 188,
        group: " T",
    },
    Element {
        symbol: "NI",
        name: "Nickel",
        year_discovered: 1751,
        atomic_mass: 5869,
        electronegativity: 191,
        group: " T",
    },
    Element {
        symbol: "CU",
        name: "Copper",
        year_discovered: -9000,
        atomic_mass: 6355,
        electronegativity: 190,
        group: " T",
    },
    Element {
        symbol: "ZN",
        name: "Zinc",
        year_discovered: -1000,
        atomic_mass: 6538,
        electronegativity: 165,
        group: " T",
    },
    Element {
        symbol: "GA",
        name: "Gallium",
        year_discovered: 1875,
        atomic_mass: 6972,
        electronegativity: 181,
        group: "3",
    },
    Element {
        symbol: "GE",
        name: "Germanium",
        year_discovered: 1886,
        atomic_mass: 7263,
        electronegativity: 201,
        group: "4",
    },
    Element {
        symbol: "AS",
        name: "Arsenic",
        year_discovered: 300,
        atomic_mass: 7492,
        electronegativity: 218,
        group: "5",
    },
    Element {
        symbol: "SE",
        name: "Selenium",
        year_discovered: 1817,
        atomic_mass: 7897,
        electronegativity: 255,
        group: "6",
    },
    Element {
        symbol: "BR",
        name: "Bromine",
        year_discovered: 1825,
        atomic_mass: 7990,
        electronegativity: 296,
        group: "7",
    },
    Element {
        symbol: "KR",
        name: "Krypton",
        year_discovered: 1898,
        atomic_mass: 8380,
        electronegativity: 300,
        group: "0",
    },
    Element {
        symbol: "RB",
        name: "Rubidium",
        year_discovered: 1861,
        atomic_mass: 8547,
        electronegativity: 82,
        group: "1",
    },
    Element {
        symbol: "SR",
        name: "Strontium",
        year_discovered: 1787,
        atomic_mass: 8762,
        electronegativity: 95,
        group: "2",
    },
    Element {
        symbol: "Y",
        name: "Yttrium",
        year_discovered: 1794,
        atomic_mass: 8891,
        electronegativity: 122,
        group: " T",
    },
    Element {
        symbol: "ZR",
        name: "Zirconium",
        year_discovered: 1789,
        atomic_mass: 9122,
        electronegativity: 133,
        group: " T",
    },
    Element {
        symbol: "NB",
        name: "Niobium",
        year_discovered: 1801,
        atomic_mass: 9291,
        electronegativity: 160,
        group: " T",
    },
    Element {
        symbol: "MO",
        name: "Molybdenum",
        year_discovered: 1778,
        atomic_mass: 9595,
        electronegativity: 216,
        group: " T",
    },
    Element {
        symbol: "TC",
        name: "Technetium",
        year_discovered: 1937,
        atomic_mass: 9700,
        electronegativity: 190,
        group: " T",
    },
    Element {
        symbol: "RU",
        name: "Ruthenium",
        year_discovered: 1844,
        atomic_mass: 10107,
        electronegativity: 220,
        group: " T",
    },
    Element {
        symbol: "RH",
        name: "Rhodium",
        year_discovered: 1804,
        atomic_mass: 10291,
        electronegativity: 228,
        group: " T",
    },
    Element {
        symbol: "PD",
        name: "Palladium",
        year_discovered: 1802,
        atomic_mass: 10642,
        electronegativity: 220,
        group: " T",
    },
    Element {
        symbol: "AG",
        name: "Silver",
        year_discovered: -5000,
        atomic_mass: 10787,
        electronegativity: 193,
        group: " T",
    },
    Element {
        symbol: "CD",
        name: "Cadmium",
        year_discovered: 1817,
        atomic_mass: 11241,
        electronegativity: 169,
        group: " T",
    },
    Element {
        symbol: "IN",
        name: "Indium",
        year_discovered: 1863,
        atomic_mass: 11482,
        electronegativity: 178,
        group: "3",
    },
    Element {
        symbol: "SN",
        name: "Tin",
        year_discovered: -3500,
        atomic_mass: 11871,
        electronegativity: 196,
        group: "4",
    },
    Element {
        symbol: "SB",
        name: "Antimony",
        year_discovered: -3000,
        atomic_mass: 12176,
        electronegativity: 205,
        group: "5",
    },
    Element {
        symbol: "TE",
        name: "Tellurium",
        year_discovered: 1782,
        atomic_mass: 12760,
        electronegativity: 210,
        group: "6",
    },
    Element {
        symbol: "I",
        name: "Iodine",
        year_discovered: 1811,
        atomic_mass: 12690,
        electronegativity: 266,
        group: "7",
    },
    Element {
        symbol: "XE",
        name: "Xenon",
        year_discovered: 1898,
        atomic_mass: 13129,
        electronegativity: 260,
        group: "0",
    },
    Element {
        symbol: "CS",
        name: "Caesium",
        year_discovered: 1860,
        atomic_mass: 13291,
        electronegativity: 79,
        group: "1",
    },
    Element {
        symbol: "BA",
        name: "Barium",
        year_discovered: 1772,
        atomic_mass: 13733,
        electronegativity: 89,
        group: "2",
    },
    Element {
        symbol: "LA",
        name: "Lanthanum",
        year_discovered: 1838,
        atomic_mass: 13891,
        electronegativity: 110,
        group: "1a",
    },
    Element {
        symbol: "CE",
        name: "Cerium",
        year_discovered: 1803,
        atomic_mass: 14012,
        electronegativity: 112,
        group: "1a",
    },
    Element {
        symbol: "PR",
        name: "Praseodymium",
        year_discovered: 1885,
        atomic_mass: 14091,
        electronegativity: 113,
        group: "1a",
    },
    Element {
        symbol: "ND",
        name: "Neodymium",
        year_discovered: 1841,
        atomic_mass: 14424,
        electronegativity: 114,
        group: "1a",
    },
    Element {
        symbol: "PM",
        name: "Promethium",
        year_discovered: 1945,
        atomic_mass: 14500,
        electronegativity: 113,
        group: "1a",
    },
    Element {
        symbol: "SM",
        name: "Samarium",
        year_discovered: 1879,
        atomic_mass: 15036,
        electronegativity: 117,
        group: "1a",
    },
    Element {
        symbol: "EU",
        name: "Europium",
        year_discovered: 1896,
        atomic_mass: 15196,
        electronegativity: 120,
        group: "1a",
    },
    Element {
        symbol: "GD",
        name: "Gadolinium",
        year_discovered: 1880,
        atomic_mass: 15725,
        electronegativity: 120,
        group: "1a",
    },
    Element {
        symbol: "TB",
        name: "Terbium",
        year_discovered: 1843,
        atomic_mass: 15893,
        electronegativity: 120,
        group: "1a",
    },
    Element {
        symbol: "DY",
        name: "Dysprosium",
        year_discovered: 1886,
        atomic_mass: 16250,
        electronegativity: 122,
        group: "1a",
    },
    Element {
        symbol: "HO",
        name: "Holmium",
        year_discovered: 1878,
        atomic_mass: 16493,
        electronegativity: 123,
        group: "1a",
    },
    Element {
        symbol: "ER",
        name: "Erbium",
        year_discovered: 1843,
        atomic_mass: 16726,
        electronegativity: 124,
        group: "1a",
    },
    Element {
        symbol: "TM",
        name: "Thulium",
        year_discovered: 1879,
        atomic_mass: 16893,
        electronegativity: 125,
        group: "1a",
    },
    Element {
        symbol: "YB",
        name: "Ytterbium",
        year_discovered: 1878,
        atomic_mass: 17305,
        electronegativity: 110,
        group: "1a",
    },
    Element {
        symbol: "LU",
        name: "Lutetium",
        year_discovered: 1906,
        atomic_mass: 17497,
        electronegativity: 127,
        group: "1a",
    },
    Element {
        symbol: "HF",
        name: "Hafnium",
        year_discovered: 1922,
        atomic_mass: 17849,
        electronegativity: 130,
        group: " T",
    },
    Element {
        symbol: "TA",
        name: "Tantalum",
        year_discovered: 1802,
        atomic_mass: 18095,
        electronegativity: 150,
        group: " T",
    },
    Element {
        symbol: "W",
        name: "Tungsten",
        year_discovered: 1781,
        atomic_mass: 18384,
        electronegativity: 236,
        group: " T",
    },
    Element {
        symbol: "RE",
        name: "Rhenium",
        year_discovered: 1908,
        atomic_mass: 18621,
        electronegativity: 190,
        group: " T",
    },
    Element {
        symbol: "OS",
        name: "Osmium",
        year_discovered: 1803,
        atomic_mass: 19023,
        electronegativity: 220,
        group: " T",
    },
    Element {
        symbol: "IR",
        name: "Iridium",
        year_discovered: 1803,
        atomic_mass: 19222,
        electronegativity: 220,
        group: " T",
    },
    Element {
        symbol: "PT",
        name: "Platinum",
        year_discovered: -600,
        atomic_mass: 19508,
        electronegativity: 228,
        group: " T",
    },
    Element {
        symbol: "AU",
        name: "Gold",
        year_discovered: -6000,
        atomic_mass: 19697,
        electronegativity: 254,
        group: " T",
    },
    Element {
        symbol: "HG",
        name: "Mercury",
        year_discovered: -1500,
        atomic_mass: 20059,
        electronegativity: 200,
        group: " T",
    },
    Element {
        symbol: "TL",
        name: "Thallium",
        year_discovered: 1861,
        atomic_mass: 20438,
        electronegativity: 162,
        group: "3",
    },
    Element {
        symbol: "PB",
        name: "Lead",
        year_discovered: -7000,
        atomic_mass: 20720,
        electronegativity: 187,
        group: "4",
    },
    Element {
        symbol: "BI",
        name: "Bismuth",
        year_discovered: 1500,
        atomic_mass: 20898,
        electronegativity: 202,
        group: "5",
    },
    Element {
        symbol: "PO",
        name: "Polonium",
        year_discovered: 1898,
        atomic_mass: 20900,
        electronegativity: 200,
        group: "6",
    },
    Element {
        symbol: "AT",
        name: "Astatine",
        year_discovered: 1940,
        atomic_mass: 21000,
        electronegativity: 220,
        group: "7",
    },
    Element {
        symbol: "RN",
        name: "Radon",
        year_discovered: 1899,
        atomic_mass: 22200,
        electronegativity: 220,
        group: "0",
    },
    Element {
        symbol: "FR",
        name: "Francium",
        year_discovered: 1939,
        atomic_mass: 22300,
        electronegativity: 79,
        group: "1",
    },
    Element {
        symbol: "RA",
        name: "Radium",
        year_discovered: 1898,
        atomic_mass: 22600,
        electronegativity: 90,
        group: "2",
    },
    Element {
        symbol: "AC",
        name: "Actinium",
        year_discovered: 1902,
        atomic_mass: 22700,
        electronegativity: 110,
        group: "Ac",
    },
    Element {
        symbol: "TH",
        name: "Thorium",
        year_discovered: 1829,
        atomic_mass: 23204,
        electronegativity: 130,
        group: "Ac",
    },
    Element {
        symbol: "PA",
        name: "Protactinium",
        year_discovered: 1913,
        atomic_mass: 23104,
        electronegativity: 150,
        group: "Ac",
    },
    Element {
        symbol: "U",
        name: "Uranium",
        year_discovered: 1789,
        atomic_mass: 23803,
        electronegativity: 138,
        group: "Ac",
    },
    Element {
        symbol: "NP",
        name: "Neptunium",
        year_discovered: 1940,
        atomic_mass: 23700,
        electronegativity: 136,
        group: "Ac",
    },
    Element {
        symbol: "PU",
        name: "Plutonium",
        year_discovered: 1941,
        atomic_mass: 24400,
        electronegativity: 128,
        group: "Ac",
    },
    Element {
        symbol: "AM",
        name: "Americium",
        year_discovered: 1944,
        atomic_mass: 24300,
        electronegativity: 113,
        group: "Ac",
    },
    Element {
        symbol: "CM",
        name: "Curium",
        year_discovered: 1944,
        atomic_mass: 24700,
        electronegativity: 128,
        group: "Ac",
    },
    Element {
        symbol: "BK",
        name: "Berkelium",
        year_discovered: 1949,
        atomic_mass: 24700,
        electronegativity: 130,
        group: "Ac",
    },
    Element {
        symbol: "CF",
        name: "Californium",
        year_discovered: 1950,
        atomic_mass: 25100,
        electronegativity: 130,
        group: "Ac",
    },
    Element {
        symbol: "ES",
        name: "Einsteinium",
        year_discovered: 1952,
        atomic_mass: 25200,
        electronegativity: 130,
        group: "Ac",
    },
    Element {
        symbol: "FM",
        name: "Fermium",
        year_discovered: 1953,
        atomic_mass: 25700,
        electronegativity: 130,
        group: "Ac",
    },
    Element {
        symbol: "MD",
        name: "Mendelevium",
        year_discovered: 1955,
        atomic_mass: 25800,
        electronegativity: 130,
        group: "Ac",
    },
    Element {
        symbol: "NO",
        name: "Nobelium",
        year_discovered: 1965,
        atomic_mass: 25900,
        electronegativity: 130,
        group: "Ac",
    },
    Element {
        symbol: "LR",
        name: "Lawrencium",
        year_discovered: 1961,
        atomic_mass: 26600,
        electronegativity: 130,
        group: "Ac",
    },
    Element {
        symbol: "RF",
        name: "Rutherfordium",
        year_discovered: 1969,
        atomic_mass: 26700,
        electronegativity: 0,
        group: " T",
    },
    Element {
        symbol: "DB",
        name: "Dubnium",
        year_discovered: 1970,
        atomic_mass: 26800,
        electronegativity: 0,
        group: " T",
    },
    Element {
        symbol: "SG",
        name: "Seaborgium",
        year_discovered: 1974,
        atomic_mass: 26700,
        electronegativity: 0,
        group: " T",
    },
    Element {
        symbol: "BH",
        name: "Bohrium",
        year_discovered: 1981,
        atomic_mass: 27000,
        electronegativity: 0,
        group: " T",
    },
    Element {
        symbol: "HS",
        name: "Hassium",
        year_discovered: 1984,
        atomic_mass: 27100,
        electronegativity: 0,
        group: " T",
    },
    Element {
        symbol: "MT",
        name: "Meitnerium",
        year_discovered: 1982,
        atomic_mass: 27800,
        electronegativity: 0,
        group: " T",
    },
    Element {
        symbol: "DS",
        name: "Darmstadtium",
        year_discovered: 1994,
        atomic_mass: 28100,
        electronegativity: 0,
        group: " T",
    },
    Element {
        symbol: "RG",
        name: "Roentgenium",
        year_discovered: 1994,
        atomic_mass: 28200,
        electronegativity: 0,
        group: " T",
    },
    Element {
        symbol: "CN",
        name: "Copernicium",
        year_discovered: 1996,
        atomic_mass: 28500,
        electronegativity: 0,
        group: " T",
    },
    Element {
        symbol: "NH",
        name: "Nihonium",
        year_discovered: 2004,
        atomic_mass: 28600,
        electronegativity: 0,
        group: "3",
    },
    Element {
        symbol: "FL",
        name: "Flerovium",
        year_discovered: 1999,
        atomic_mass: 28900,
        electronegativity: 0,
        group: "4",
    },
    Element {
        symbol: "MC",
        name: "Moscovium",
        year_discovered: 2003,
        atomic_mass: 29000,
        electronegativity: 0,
        group: "5",
    },
    Element {
        symbol: "LV",
        name: "Livermorium",
        year_discovered: 2000,
        atomic_mass: 29300,
        electronegativity: 0,
        group: "6",
    },
    Element {
        symbol: "TS",
        name: "Tennessine",
        year_discovered: 2009,
        atomic_mass: 29400,
        electronegativity: 0,
        group: "7",
    },
    Element {
        symbol: "OG",
        name: "Oganesson",
        year_discovered: 2002,
        atomic_mass: 29400,
        electronegativity: 0,
        group: "0",
    },
];

/// The periodic screens.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Title,
    Element,
    AtomicMass,
    DiscoverYear,
    Electronegativity,
    FullName,
}

const SCREEN_NAMES: [&str; 6] = ["  ", "  ", "am", " y", "EL", " n"];

/// The periodic face state.
pub struct PeriodicFace {
    atomic_num: u8,
    mode: Screen,
    text_pos: i8,
}

impl PeriodicFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        PeriodicFace {
            atomic_num: 0,
            mode: Screen::Title,
            text_pos: 0,
        }
    }

    pub fn new() -> Self {
        PeriodicFace::new_static()
    }

    fn display_element(&self) {
        let mut buf = [0u8; 9];
        let e = &TABLE[(self.atomic_num - 1) as usize];
        let g = e.group.as_bytes();
        buf[0] = g[0];
        buf[1] = g[1];
        buf[2] = b'0' + self.atomic_num / 100;
        buf[3] = b'0' + (self.atomic_num / 10) % 10;
        buf[4] = b'0' + self.atomic_num % 10;
        buf[5] = b' ';
        let s = e.symbol.as_bytes();
        buf[6] = s[0];
        buf[7] = if s.len() > 1 { s[1] } else { b' ' };
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 2);
    }

    fn display_atomic_mass(&self) {
        let mut buf = [0u8; 11];
        let e = &TABLE[(self.atomic_num - 1) as usize];
        let s = e.symbol.as_bytes();
        buf[0] = s[0];
        buf[1] = if s.len() > 1 { s[1] } else { b' ' };
        let n = SCREEN_NAMES[Screen::AtomicMass as usize].as_bytes();
        buf[2] = n[0];
        buf[3] = n[1];
        let integer = e.atomic_mass / 100;
        let decimal = e.atomic_mass % 100;
        if decimal == 0 {
            buf[6] = b'0' + (integer / 1000) as u8;
            buf[7] = b'0' + ((integer / 100) % 10) as u8;
            buf[8] = b'0' + ((integer / 10) % 10) as u8;
            buf[9] = b'0' + (integer % 10) as u8;
        } else {
            buf[6] = b'0' + (integer / 100) as u8;
            buf[7] = b'0' + ((integer / 10) % 10) as u8;
            buf[8] = b'0' + (integer % 10) as u8;
            buf[9] = b'_';
            buf[10] = b'0' + (decimal / 10) as u8;
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn display_year_discovered(&self) {
        let mut buf = [0u8; 11];
        let e = &TABLE[(self.atomic_num - 1) as usize];
        let s = e.symbol.as_bytes();
        buf[0] = s[0];
        buf[1] = if s.len() > 1 { s[1] } else { b' ' };
        let n = SCREEN_NAMES[Screen::DiscoverYear as usize].as_bytes();
        buf[2] = n[0];
        buf[3] = n[1];
        let year = e.year_discovered;
        if year.unsigned_abs() > 9999 {
            buf[4] = b'-';
            buf[5] = b'-';
            buf[6] = b'-';
            buf[7] = b'-';
        } else {
            let y = year.unsigned_abs();
            buf[4] = b'0' + ((y / 1000) % 10) as u8;
            buf[5] = b'0' + ((y / 100) % 10) as u8;
            buf[6] = b'0' + ((y / 10) % 10) as u8;
            buf[7] = b'0' + (y % 10) as u8;
        }
        if year < 0 {
            buf[8] = b'b';
            buf[9] = b'c';
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn display_electronegativity(&self) {
        let mut buf = [0u8; 11];
        let e = &TABLE[(self.atomic_num - 1) as usize];
        let s = e.symbol.as_bytes();
        buf[0] = s[0];
        buf[1] = if s.len() > 1 { s[1] } else { b' ' };
        let n = SCREEN_NAMES[Screen::Electronegativity as usize].as_bytes();
        buf[2] = n[0];
        buf[3] = n[1];
        let integer = e.electronegativity / 100;
        let decimal = e.electronegativity % 100;
        if decimal == 0 {
            buf[6] = b'0' + (integer / 10) as u8;
            buf[7] = b'0' + (integer % 10) as u8;
        } else {
            buf[6] = b'0' + (integer / 10) as u8;
            buf[7] = b'0' + (integer % 10) as u8;
            buf[8] = b'_';
            buf[9] = b'0' + (decimal / 10) as u8;
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn display_name(&self) {
        let mut buf = [0u8; 11];
        let e = &TABLE[(self.atomic_num - 1) as usize];
        let s = e.symbol.as_bytes();
        buf[0] = s[0];
        buf[1] = if s.len() > 1 { s[1] } else { b' ' };
        let n = SCREEN_NAMES[Screen::FullName as usize].as_bytes();
        buf[2] = n[0];
        buf[3] = n[1];
        let name = e.name.as_bytes();
        for (i, &c) in name.iter().take(6).enumerate() {
            buf[4 + i] = c;
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn display_title(&mut self) {
        self.atomic_num = 0;
        slcd::clear_colon();
        slcd::display_string("Pd   Table", 0);
    }

    fn display_screen(&mut self) {
        slcd::clear_display();
        match self.mode {
            Screen::Title => self.display_title(),
            Screen::Element => self.display_element(),
            Screen::AtomicMass => self.display_atomic_mass(),
            Screen::DiscoverYear => self.display_year_discovered(),
            Screen::Electronegativity => self.display_electronegativity(),
            Screen::FullName => self.display_name(),
        }
    }

    fn handle_forward(&mut self) {
        self.atomic_num = (self.atomic_num % MAX_ELEMENT) + 1;
        self.mode = Screen::Element;
        self.display_screen();
    }

    fn handle_backward(&mut self) {
        if self.atomic_num <= 1 {
            self.atomic_num = MAX_ELEMENT;
        } else {
            self.atomic_num -= 1;
        }
        self.mode = Screen::Element;
        self.display_screen();
    }
}

impl WatchFace for PeriodicFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.mode = Screen::Title;
        self.display_screen();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => {
                self.mode = Screen::Title;
                self.display_screen();
            }
            Event::Tick => {}
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.mode == Screen::Title || self.mode == Screen::Element {
                    self.handle_backward();
                } else {
                    self.mode = Screen::Element;
                    self.display_screen();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.mode == Screen::Title || self.mode == Screen::Element {
                    self.handle_forward();
                } else {
                    self.mode = Screen::Element;
                    self.display_screen();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.mode == Screen::Title || self.mode == Screen::Element {
                    self.handle_forward();
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.mode == Screen::Title || self.mode == Screen::Element {
                    self.handle_backward();
                } else {
                    movement::illuminate_led();
                }
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                if self.mode == Screen::Title {
                    movement::move_to_next_face();
                } else {
                    self.mode = match self.mode {
                        Screen::Element => Screen::AtomicMass,
                        Screen::AtomicMass => Screen::DiscoverYear,
                        Screen::DiscoverYear => Screen::Electronegativity,
                        Screen::Electronegativity => Screen::FullName,
                        _ => Screen::Element,
                    };
                    self.display_screen();
                }
            }
            Event::Button(Button::Mode, ButtonEvent::LongPress) => {
                if self.mode == Screen::Title {
                    movement::move_to_face(0);
                } else {
                    self.mode = Screen::Title;
                    self.display_screen();
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
