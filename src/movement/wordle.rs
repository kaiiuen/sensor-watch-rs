//! Wordle watch face.
//!
//! Port of the C `wordle_face.c`. A Wordle-style word guessing game. It is a
//! pure state machine: it reacts to a single event and returns; it never keeps
//! the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;

const WORDLE_LENGTH: usize = 5;
const WORDLE_NUM_VALID_LETTERS: u8 = 12;
const WORDLE_MAX_ATTEMPTS: u8 = 6;

const VALID_LETTERS: [u8; 12] = [
    b'A', b'C', b'E', b'H', b'I', b'L', b'N', b'O', b'P', b'R', b'S', b'T',
];

const SCREEN_TITLE: u8 = 0;
const SCREEN_PLAYING: u8 = 1;
const SCREEN_RESULT: u8 = 2;
const SCREEN_LOSE: u8 = 3;
const SCREEN_WIN: u8 = 4;
const SCREEN_CONTINUE: u8 = 5;
const SCREEN_STREAK: u8 = 6;
const SCREEN_NO_DICT: u8 = 7;
const SCREEN_ALREADY_GUESSED: u8 = 8;

const WORDLE_LETTER_WRONG: u8 = 0;
const WORDLE_LETTER_WRONG_LOC: u8 = 1;
const WORDLE_LETTER_CORRECT: u8 = 2;

/// A subset of the valid answer words.
static VALID_WORDS: &[&str] = &[
    "SLATE", "STARE", "SNARE", "SANER", "CRANE", "STALE", "CRATE", "RAISE", "TRACE", "SHARE",
    "ARISE", "SCARE", "SPARE", "CHAOS", "TAPIR", "CAIRN", "TENOR", "CLEAN", "HEART", "SCOPE",
    "SNARL", "SLEPT", "SINCE", "EPOCH", "SPACE", "RELIC", "SPOIL", "LITER", "LEAPT", "LANCE",
    "RANCH", "HORSE", "LEACH", "LATER", "STEAL", "CHEAP", "SHORT", "ETHIC", "CHANT", "ACTOR",
    "REACH", "SEPIA", "ONSET", "SPLAT", "LEANT", "REACT", "OCTAL", "SPORE", "IRATE", "CORAL",
    "NICER", "SPILT", "SCENT", "PANIC", "SHIRT", "PECAN", "SLAIN", "SPLIT", "ROACH", "ASCOT",
    "PHONE", "LITHE", "STOIC", "STRIP", "RENAL", "POISE", "ENACT", "CHEAT", "PITCH", "NOISE",
    "INLET", "PEARL", "POLAR", "PEACH", "STOLE", "CASTE", "CREST", "CRONE", "ETHOS", "THEIR",
    "STONE", "SHIRE", "LATCH", "HASTE", "CLOSE", "SPINE", "SLANT", "SPEAR", "SCALE", "CAPER",
    "RETCH", "PESTO", "CHIRP", "SPORT", "OPTIC", "SNAIL", "PRICE", "PLANE", "TORCH", "PASTE",
    "RECAP", "SOLAR", "CRASH", "LINER", "OPINE", "ASHEN", "PALER", "ECLAT", "SPELT", "TRIAL",
    "PERIL", "SLICE", "SCANT", "SAINT", "POSIT", "ATONE", "SPIRE", "COAST", "INEPT", "SHOAL",
    "CLASH", "THORN", "PHASE", "SCORE", "TRICE", "PERCH", "PORCH", "SHEAR", "CHOIR", "RHINO",
    "PLANT", "SHONE", "CHORE", "LEARN", "ALTER", "CHAIN", "PANEL", "PLIER", "STEIN", "COPSE",
    "SONIC", "ALIEN", "CHOSE", "ACORN", "ANTIC", "CHEST", "OTHER", "CHINA", "TALON", "SCORN",
    "PLAIN", "PILOT", "RIPEN", "PATCH", "SPICE", "CLONE", "SCION", "SCONE", "STRAP", "PARSE",
    "SHALE", "RISEN", "CANOE", "INTER", "LEASH", "ISLET", "PRINT", "SHINE", "NORTH", "CLEAT",
    "PLAIT", "SCRAP", "CLEAR", "SLOTH", "LAPSE", "CHAIR", "SNORT", "SHARP", "OPERA", "STAIN",
    "TEACH", "TRAIL", "TRAIN", "LATHE", "PIANO", "PINCH", "PETAL", "STERN", "PRONE", "PROSE",
    "PLEAT", "TROPE", "PLACE", "POSER", "INERT", "CHASE", "CAROL", "STAIR", "SATIN", "SPITE",
    "LOATH", "ROAST", "ARSON", "SHAPE", "CLASP", "LOSER", "SALON", "CATER", "SHALT", "INTRO",
    "ALERT", "PENAL", "SHORE", "RINSE", "CREPT", "APRON", "SONAR", "AISLE", "AROSE", "POINT",
    "EARTH", "PINTO", "THOSE", "CLOTH", "NOTCH", "TOPIC", "RESIN", "SCALP", "HEIST", "HERON",
    "TRIPE", "TONAL", "TAPER", "SHORN", "TONIC", "HOIST", "SNORE", "STORE", "SLOPE", "OCEAN",
    "CHART", "PAINT", "SPENT", "SNIPE", "CRISP", "TRASH", "PATIO", "PLATE", "HOTEL", "LEAST",
    "ALONE", "SPIEL", "SIREN", "RATIO", "STOOP", "TROLL", "ATOOL", "SLASH", "RETRO", "CREEP",
    "STILT", "SPREE", "TASTE", "CACHE", "CANON", "EATEN", "TEPEE", "SHEET", "SNEER", "ERROR",
    "NATAL", "SLEEP", "STINT", "TROOP", "SHALL", "STALL", "PIPER", "TOAST", "NASAL", "CORER",
    "THERE", "POOCH", "SCREE", "ELITE", "ALTAR", "PENCE", "EATER", "ALPHA", "TENTH", "LINEN",
    "SHEER", "TAINT", "HEATH", "CRIER", "TENSE", "CARAT", "CANAL", "APNEA", "THESE", "HATCH",
    "SHELL", "CIRCA", "APART", "SPILL", "STEEL", "LOCAL", "STOOL", "SHEEN", "RESET", "STEEP",
    "ELATE", "PRESS", "SLEET", "CROSS", "TOTAL", "TREAT", "ONION", "STATE", "CINCH", "ASSET",
    "THREE", "TORSO", "SNOOP", "PENNE", "SPOON", "SHEEP", "PAPAL", "STILL", "CHILL", "THETA",
    "LEECH", "INNER", "HONOR", "LOOSE", "CONIC", "SCENE", "COACH", "CONCH", "LATTE", "ERASE",
    "ESTER", "PEACE", "PASTA", "INANE", "SPOOL", "TEASE", "HARSH", "PIECE", "STEER", "SCOOP",
    "NINTH", "OTTER", "OCTET", "EERIE", "RISER", "LAPEL", "HIPPO", "PREEN", "ETHER", "AORTA",
    "SENSE", "TRACT", "SHOOT", "SLOOP", "REPEL", "TITHE", "IONIC", "CELLO", "CHESS", "SOOTH",
    "COCOA", "TITAN", "TOOTH", "TIARA", "CRESS", "SLOSH", "RARER", "TERSE", "ERECT", "HELLO",
    "PARER", "RIPER", "NOOSE", "CREPE", "CACAO", "ILIAC", "POSSE", "CACTI", "EASEL", "LASSO",
    "ROOST", "ALLOT", "COLON", "LEPER", "TEETH", "TITLE", "HENCE", "NIECE", "PAPER", "TRITE",
    "SPELL", "RACER", "ATTIC", "CRASS", "HITCH", "LEASE", "CEASE", "ROTOR", "ELOPE", "APPLE",
    "CHILI", "START", "PHOTO", "SALSA", "STASH", "PRIOR", "TAROT", "COLOR", "CHEER", "CLASS",
    "ARENA", "ELECT", "ENTER", "CATCH", "TENET", "TACIT", "TRAIT", "TERRA", "LILAC",
];

const NUM_WORDS: u16 = 429;

/// The wordle face state.
pub struct WordleFace {
    curr_screen: u8,
    position: u8,
    attempt: u8,
    word_elements: [u8; WORDLE_LENGTH],
    word_elements_result: [u8; WORDLE_LENGTH],
    known_wrong_letters: [bool; WORDLE_NUM_VALID_LETTERS as usize],
    curr_answer: u8,
    skip_wrong_letter: bool,
    continuing: bool,
    streak: u8,
    using_random_guess: bool,
}

impl WordleFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        WordleFace {
            curr_screen: SCREEN_TITLE,
            position: 0,
            attempt: 0,
            word_elements: [WORDLE_NUM_VALID_LETTERS; WORDLE_LENGTH],
            word_elements_result: [WORDLE_LETTER_WRONG; WORDLE_LENGTH],
            known_wrong_letters: [false; WORDLE_NUM_VALID_LETTERS as usize],
            curr_answer: 0,
            skip_wrong_letter: false,
            continuing: false,
            streak: 0,
            using_random_guess: false,
        }
    }

    pub fn new() -> Self {
        WordleFace::new_static()
    }

    fn get_random(&self, max: u32) -> u32 {
        let now = crate::watch::rtc::get_date_time();
        let mut x = now.to_reg();
        if x == 0 {
            x = 0x1234_5678;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x % max
    }

    fn get_first_pos(&self) -> u8 {
        for i in 0..WORDLE_LENGTH {
            if self.word_elements_result[i] != WORDLE_LETTER_CORRECT {
                return i as u8;
            }
        }
        0
    }

    fn get_next_pos(&self, curr_pos: u8) -> u8 {
        let mut pos = curr_pos as usize;
        while pos < WORDLE_LENGTH {
            pos += 1;
            if pos >= WORDLE_LENGTH {
                break;
            }
            if self.word_elements_result[pos] != WORDLE_LETTER_CORRECT {
                return pos as u8;
            }
        }
        WORDLE_LENGTH as u8
    }

    fn get_prev_pos(&self, curr_pos: u8) -> u8 {
        if curr_pos == 0 {
            return 0;
        }
        let mut pos = curr_pos as i8;
        while pos >= 0 {
            pos -= 1;
            if pos < 0 {
                break;
            }
            if self.word_elements_result[pos as usize] != WORDLE_LETTER_CORRECT {
                return pos as u8;
            }
        }
        curr_pos
    }

    fn get_next_letter(&mut self) {
        loop {
            if self.word_elements[self.position as usize] >= WORDLE_NUM_VALID_LETTERS {
                self.word_elements[self.position as usize] = 0;
            } else {
                self.word_elements[self.position as usize] =
                    (self.word_elements[self.position as usize] + 1) % WORDLE_NUM_VALID_LETTERS;
            }
            if !self.skip_wrong_letter
                || !self.known_wrong_letters[self.word_elements[self.position as usize] as usize]
            {
                break;
            }
        }
    }

    fn get_prev_letter(&mut self) {
        loop {
            if self.word_elements[self.position as usize] >= WORDLE_NUM_VALID_LETTERS {
                self.word_elements[self.position as usize] = WORDLE_NUM_VALID_LETTERS - 1;
            } else {
                self.word_elements[self.position as usize] =
                    (self.word_elements[self.position as usize] + WORDLE_NUM_VALID_LETTERS - 1)
                        % WORDLE_NUM_VALID_LETTERS;
            }
            if !self.skip_wrong_letter
                || !self.known_wrong_letters[self.word_elements[self.position as usize] as usize]
            {
                break;
            }
        }
    }

    fn display_letter(&self, display_dash: bool) {
        let mut buf = [b' '; 1];
        if self.word_elements[self.position as usize] >= WORDLE_NUM_VALID_LETTERS {
            buf[0] = if display_dash { b'-' } else { b' ' };
        } else {
            buf[0] = VALID_LETTERS[self.word_elements[self.position as usize] as usize];
        }
        watch::slcd::display_string(
            core::str::from_utf8(&buf[..]).unwrap_or(" "),
            self.position + 5,
        );
    }

    fn display_all_letters(&mut self) {
        let prev_pos = self.position;
        watch::slcd::display_string(" ", 4);
        for i in 0..WORDLE_LENGTH {
            self.position = i as u8;
            self.display_letter(false);
        }
        self.position = prev_pos;
    }

    fn display_attempt(&self) {
        let mut buf = [0u8; 2];
        buf[0] = b'0' + (self.attempt + 1);
        watch::slcd::display_string(core::str::from_utf8(&buf[..1]).unwrap_or(" "), 3);
    }

    fn display_playing(&mut self) {
        self.curr_screen = SCREEN_PLAYING;
        self.display_attempt();
        self.display_all_letters();
    }

    fn reset_all_elements(&mut self) {
        for i in 0..WORDLE_LENGTH {
            self.word_elements[i] = WORDLE_NUM_VALID_LETTERS;
            self.word_elements_result[i] = WORDLE_LETTER_WRONG;
        }
        for i in 0..WORDLE_NUM_VALID_LETTERS as usize {
            self.known_wrong_letters[i] = false;
        }
        self.using_random_guess = false;
        self.attempt = 0;
    }

    fn reset_incorrect_elements(&mut self) {
        for i in 0..WORDLE_LENGTH {
            if self.word_elements_result[i] != WORDLE_LETTER_CORRECT {
                self.word_elements[i] = WORDLE_NUM_VALID_LETTERS;
            }
        }
    }

    fn reset_board(&mut self) {
        self.reset_all_elements();
        self.curr_answer = self.get_random(NUM_WORDS as u32) as u8;
        watch::slcd::clear_colon();
        self.position = self.get_first_pos();
        self.display_playing();
        watch::slcd::display_string(" -", 4);
    }

    fn display_title(&mut self) {
        self.curr_screen = SCREEN_TITLE;
        watch::slcd::display_string("WO  WordLE", 0);
        if self.skip_wrong_letter {
            watch::slcd::display_string("H", 3);
        } else {
            watch::slcd::display_string(" ", 3);
        }
    }

    fn display_continue_result(&self, continuing: bool) {
        watch::slcd::display_string(if continuing { "y" } else { "n" }, 9);
    }

    fn display_continue(&mut self) {
        self.curr_screen = SCREEN_CONTINUE;
        watch::slcd::display_string("Cont ", 4);
        if self.skip_wrong_letter {
            watch::slcd::display_string("H", 3);
        } else {
            watch::slcd::display_string(" ", 3);
        }
        self.display_continue_result(self.continuing);
    }

    fn display_streak(&mut self) {
        self.curr_screen = SCREEN_STREAK;
        let mut buf = [0u8; 11];
        buf[0] = b'W';
        buf[1] = b'O';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b'S';
        buf[5] = b't';
        buf[6] = b'0' + (self.streak / 100) % 10;
        buf[7] = b'0' + (self.streak / 10) % 10;
        buf[8] = b'0' + self.streak % 10;
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        watch::slcd::set_colon();
        if self.skip_wrong_letter {
            watch::slcd::display_string("H", 3);
        } else {
            watch::slcd::display_string(" ", 3);
        }
    }

    fn display_lose(&self, subsecond: u8) {
        let mut buf = [0u8; 11];
        buf[0] = b' ';
        buf[1] = b'L';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b' ';
        if subsecond % 2 == 1 {
            let w = VALID_WORDS[self.curr_answer as usize].as_bytes();
            for (i, &c) in w.iter().take(5).enumerate() {
                buf[5 + i] = c;
            }
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn display_win(&self, subsecond: u8) {
        let mut buf = [0u8; 11];
        buf[0] = b' ';
        buf[1] = b'W';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b' ';
        let word = if subsecond % 2 == 1 { "NICE" } else { "JOb " };
        let w = word.as_bytes();
        for (i, &c) in w.iter().take(4).enumerate() {
            buf[5 + i] = c;
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn is_playing(&self) -> bool {
        if self.attempt > 0 {
            return true;
        }
        for i in 0..WORDLE_LENGTH {
            if self.word_elements[i] != WORDLE_NUM_VALID_LETTERS {
                return true;
            }
        }
        false
    }

    fn display_result(&self, subsecond: u8) {
        let mut buf = [b' '; 6];
        for i in 0..WORDLE_LENGTH {
            match self.word_elements_result[i] {
                WORDLE_LETTER_WRONG => buf[i] = b'-',
                WORDLE_LETTER_CORRECT => buf[i] = VALID_LETTERS[self.word_elements[i] as usize],
                _ => {
                    if subsecond % 2 == 1 {
                        buf[i] = b' ';
                    } else {
                        buf[i] = VALID_LETTERS[self.word_elements[i] as usize];
                    }
                }
            }
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("     "), 5);
    }

    fn check_word(&mut self) -> bool {
        let mut is_exact_match = true;
        let mut answer_accounted = [false; WORDLE_LENGTH];
        let answer = VALID_WORDS[self.curr_answer as usize].as_bytes();
        for i in 0..WORDLE_LENGTH {
            if VALID_LETTERS[self.word_elements[i] as usize] == answer[i] {
                self.word_elements_result[i] = WORDLE_LETTER_CORRECT;
                answer_accounted[i] = true;
            } else {
                self.word_elements_result[i] = WORDLE_LETTER_WRONG;
                is_exact_match = false;
            }
        }
        if is_exact_match {
            return true;
        }
        for i in 0..WORDLE_LENGTH {
            if self.word_elements_result[i] != WORDLE_LETTER_WRONG {
                continue;
            }
            for j in 0..WORDLE_LENGTH {
                if answer_accounted[j] {
                    continue;
                }
                if VALID_LETTERS[self.word_elements[i] as usize] == answer[j] {
                    self.word_elements_result[i] = WORDLE_LETTER_WRONG_LOC;
                    answer_accounted[j] = true;
                    break;
                }
            }
        }
        false
    }

    fn update_known_wrong_letters(&mut self) {
        let mut wrong_loc = [false; WORDLE_NUM_VALID_LETTERS as usize];
        for i in 0..WORDLE_LENGTH {
            if self.word_elements_result[i] == WORDLE_LETTER_WRONG_LOC {
                wrong_loc[self.word_elements[i] as usize] = true;
            }
        }
        for i in 0..WORDLE_LENGTH {
            if self.word_elements_result[i] == WORDLE_LETTER_WRONG {
                let j = self.word_elements[i] as usize;
                if !wrong_loc[j] {
                    self.known_wrong_letters[j] = true;
                }
            }
        }
    }

    fn act_on_btn(&mut self, is_alarm: bool) -> bool {
        match self.curr_screen {
            SCREEN_RESULT => {
                self.reset_incorrect_elements();
                self.position = self.get_first_pos();
                self.display_playing();
                true
            }
            SCREEN_TITLE => {
                if self.is_playing() {
                    self.continuing = true;
                    self.display_continue();
                } else {
                    self.display_streak();
                }
                true
            }
            SCREEN_STREAK => {
                self.reset_board();
                true
            }
            SCREEN_WIN | SCREEN_LOSE => {
                self.display_title();
                true
            }
            SCREEN_NO_DICT | SCREEN_ALREADY_GUESSED => {
                self.position = self.get_first_pos();
                self.display_playing();
                true
            }
            SCREEN_CONTINUE => {
                if is_alarm {
                    if self.continuing {
                        self.display_playing();
                    } else {
                        self.reset_board();
                        self.streak = 0;
                        self.display_streak();
                    }
                } else {
                    self.continuing = !self.continuing;
                    self.display_continue_result(self.continuing);
                }
                true
            }
            _ => false,
        }
    }

    fn get_result(&mut self) {
        let exact_match = self.check_word();
        if exact_match {
            self.reset_all_elements();
            self.curr_screen = SCREEN_WIN;
            if self.streak < 0x7F {
                self.streak += 1;
            }
            return;
        }
        self.attempt += 1;
        if self.attempt >= WORDLE_MAX_ATTEMPTS {
            self.reset_all_elements();
            self.curr_screen = SCREEN_LOSE;
            self.streak = 0;
            return;
        }
        self.update_known_wrong_letters();
        self.curr_screen = SCREEN_RESULT;
    }
}

impl WatchFace for WordleFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.using_random_guess = false;
        if self.is_playing() && self.curr_screen >= SCREEN_RESULT {
            self.reset_incorrect_elements();
            self.position = self.get_first_pos();
        }
        self.display_title();
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Tick => match self.curr_screen {
                SCREEN_PLAYING => {
                    if 0 % 2 == 1 {
                        self.display_letter(true);
                    } else {
                        watch::slcd::display_string(" ", self.position + 5);
                    }
                }
                SCREEN_RESULT => self.display_result(0),
                SCREEN_LOSE => self.display_lose(0),
                SCREEN_WIN => self.display_win(0),
                _ => {}
            },
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.act_on_btn(false) {
                    return;
                }
                self.get_next_letter();
                self.display_letter(true);
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.curr_screen < SCREEN_PLAYING {
                    self.skip_wrong_letter = !self.skip_wrong_letter;
                    if self.skip_wrong_letter {
                        watch::slcd::display_string("H", 3);
                    } else {
                        watch::slcd::display_string(" ", 3);
                    }
                    return;
                }
                if self.curr_screen != SCREEN_PLAYING {
                    return;
                }
                self.get_prev_letter();
                self.display_letter(true);
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.act_on_btn(true) {
                    return;
                }
                self.display_letter(true);
                if self.word_elements[self.position as usize] == WORDLE_NUM_VALID_LETTERS {
                    return;
                }
                self.position = self.get_next_pos(self.position);
                if self.position >= WORDLE_LENGTH as u8 {
                    self.get_result();
                    self.using_random_guess = false;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.curr_screen != SCREEN_PLAYING {
                    return;
                }
                self.display_letter(true);
                self.position = self.get_prev_pos(self.position);
            }
            Event::Button(Button::Light, ButtonEvent::Down) | Event::Activate => {}
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
