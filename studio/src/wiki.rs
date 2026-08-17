//! A small built-in, Wikipedia-like reference browser for Sensor-Watch concepts.
//!
//! Pages are plain text bodies with `[[PageName]]` tokens that render as
//! clickable cross-links, plus a back stack for navigation.

/// A single reference page.
pub struct WikiPage {
    /// The page title (also the target of any `[[Title]]` links).
    pub title: String,
    /// The body text. `[[PageName]]` tokens become clickable links.
    pub body: String,
    /// Explicit links to other pages by title (kept for discoverability).
    pub links: Vec<String>,
}

/// The wiki state: the pages, the current page, and the back stack.
pub struct Wiki {
    pub pages: Vec<WikiPage>,
    /// The search box text used to filter the page list.
    pub search: String,
    /// The currently displayed page title.
    pub current: String,
    /// Titles visited before the current one (for the Back button).
    pub history: Vec<String>,
}

impl Wiki {
    /// Build the wiki with the curated set of pages.
    pub fn new() -> Self {
        let pages = curated_pages();
        let current = String::from("Wiki Home");
        Wiki {
            pages,
            search: String::new(),
            current,
            history: Vec::new(),
        }
    }

    /// Look up a page by title.
    pub fn page(&self, title: &str) -> Option<&WikiPage> {
        self.pages.iter().find(|p| p.title == title)
    }

    /// The currently displayed page, if any.
    pub fn current_page(&self) -> Option<&WikiPage> {
        self.page(&self.current)
    }

    /// Navigate to a page by title, pushing the current one onto the back stack.
    pub fn navigate(&mut self, title: &str) {
        if title == self.current || self.page(title).is_none() {
            return;
        }
        self.history.push(self.current.clone());
        self.current = title.to_string();
    }

    /// Go back one page.
    pub fn back(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.current = prev;
        }
    }

    /// Navigate to the previous page in the curated page order.
    pub fn previous_page(&mut self) {
        let Some(index) = self
            .pages
            .iter()
            .position(|page| page.title == self.current)
        else {
            return;
        };
        if index > 0 {
            let title = self.pages[index - 1].title.clone();
            self.navigate(&title);
        }
    }

    /// Navigate to the next page in the curated page order.
    pub fn next_page(&mut self) {
        let Some(index) = self
            .pages
            .iter()
            .position(|page| page.title == self.current)
        else {
            return;
        };
        if let Some(page) = self.pages.get(index + 1) {
            let title = page.title.clone();
            self.navigate(&title);
        }
    }
}

/// Build the curated reference pages.
fn curated_pages() -> Vec<WikiPage> {
    let mut pages = Vec::new();

    pages.push(WikiPage {
        title: String::from("Wiki Home"),
        body: String::from(
            "Welcome to the Firmware Studio wiki. This is a small built-in
reference for the concepts you will meet while working with the
Sensor-Watch. Click any link below to jump to that page, or use the
search box on the left to filter the page list.

Start with these pages:

[[UF2]], [[Bootloader]], [[Firmware]], [[Watch Face]]

New to the ecosystem? Read [[Watch Face]] and [[Preset]] first, then
[[Flash]] to learn how code gets onto the watch.

Want to know how the watch stays accurate? See [[PPM (drift)]] and
[[NTP]]. For timezone handling, see [[Timezone]] and [[DST]].

For the display and controls, start with [[LCD & Indicators]],
[[Buttons]], and [[Character Set]]. For configuration vocabulary, see
[[Settings Terms]]. For Studio and firmware vocabulary, see
[[Firmware & Studio Terms]].

For the deeper plumbing, read [[HAL]], [[Serial Shell]], and
[[Settings Register]]. To try faces without hardware, see
[[Simulator]]. The always-on [[Accelerometer]] page explains motion
sensing.

Repos to browse are listed at the top under \"Browse repos\".",
        ),
        links: vec![
            String::from("UF2"),
            String::from("Bootloader"),
            String::from("Firmware"),
            String::from("Watch Face"),
            String::from("LCD & Indicators"),
            String::from("Buttons"),
            String::from("Character Set"),
            String::from("Settings Terms"),
            String::from("Firmware & Studio Terms"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("UF2"),
        body: String::from(
            "UF2 (USB Flashing Format) is the file format the firmware is
delivered in. When you build the project you get a `.uf2` file.

The watch's bootloader understands UF2. You copy (or drag-drop) the
.uf2 file onto the watch when it appears as a USB drive, and the
bootloader writes it into flash. The .uf2 is essentially the
[[Firmware]] plus a small header that tells the [[Bootloader]] where
to put each block.

Because it is a plain file-on-a-drive, flashing is simple: no special
programmer or cable is required beyond USB. See [[Flash]] for the
full workflow.",
        ),
        links: vec![
            String::from("Firmware"),
            String::from("Bootloader"),
            String::from("Flash"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("Bootloader"),
        body: String::from(
            "The bootloader is a small program that runs first when the watch
powers on. Its job is to receive a new [[UF2]] file and write it into
[[Flash]], replacing the previous [[Firmware]].

On the Sensor-Watch, the bootloader is intentionally easy to enter:
you just double-press the reset button (or similar) and the watch
mounts as a USB drive. Dropping a .uf2 file onto it triggers the
update.

The bootloader is separate from the main firmware, so even a broken
firmware image can usually still be recovered by re-entering the
bootloader and flashing again.",
        ),
        links: vec![
            String::from("UF2"),
            String::from("Flash"),
            String::from("Firmware"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("Watch Face"),
        body: String::from(
            "A watch face is a single screen of logic and drawing shown on the
watch. The Sensor-Watch is firmware-driven, so each face is a small
program with its own UI, buttons, and quiz-like options.

Faces are the building blocks of the watch. You pick which faces you
want through a [[Preset]], and the [[Firmware]] contains the code for
only those faces. Each face runs in a loop, redrawing the display and
reacting to button presses.

Many faces are available in the catalog here in Firmware Studio. The
[[Simulator]] lets you try a face without flashing it to hardware.",
        ),
        links: vec![
            String::from("Preset"),
            String::from("Firmware"),
            String::from("Simulator"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("Preset"),
        body: String::from(
            "A preset is a saved selection of [[Watch Face]]s (and
their settings) that make up the watch's firmware. Instead of building
a one-off, you create a preset that lists the faces you want.

When you build, the [[Firmware]] is compiled to include exactly the
faces in your active preset. Fewer faces means a smaller binary that
fits more easily in [[Flash]].

Presets are managed in the Faces tab and can be saved, loaded, and
exported. They are the main way you decide what your watch can do.",
        ),
        links: vec![
            String::from("Watch Face"),
            String::from("Firmware"),
            String::from("Flash"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("PPM (drift)"),
        body: String::from(
            "PPM stands for parts-per-million and is a unit used to describe
clock drift. A real clock is never perfectly accurate; it gains or
loses a little time each second.

A drift of 1 PPM means the watch deviates by one microsecond per
second, or about 0.0864 seconds per day. A typical quartz watch might
drift tens of PPM, which is why it can lose or gain seconds every day.

The Calibration tab measures the watch's drift and can apply a
correction. Time-based faces use this to keep the clock accurate, and
[[NTP]] can periodically re-sync the watch to an external time source
to correct for drift.",
        ),
        links: vec![String::from("NTP")],
    });

    pages.push(WikiPage {
        title: String::from("NTP"),
        body: String::from(
            "NTP (Network Time Protocol) is a way to fetch the current time from
a server over the network. The watch has no network of its own, so
NTP here is used by Firmware Studio (or a connected companion) to get
an accurate time reference.

The NTP tab queries a server such as Cloudflare and reports the
current time, the round-trip ping, and the offset between the server
time and your clock. This is useful for calibrating the watch and for
correcting [[PPM (drift)]] with a known-good time source.

You can add and edit NTP servers in the NTP tab.",
        ),
        links: vec![String::from("PPM (drift)")],
    });

    pages.push(WikiPage {
        title: String::from("HAL"),
        body: String::from(
            "HAL stands for Hardware Abstraction Layer. It is a thin layer of
code that lets the [[Firmware]] talk to the hardware (buttons, display,
battery, sensors) without caring about the exact chip underneath.

Because the code is written against a HAL, the same faces can run on
the real watch and, via a host implementation, inside the
[[Simulator]]. The HAL provides the buttons, the display drawing
primitives, and access to peripherals like the [[Accelerometer]].

In this project, the host HAL is what lets the Simulator render real
faces on your computer.",
        ),
        links: vec![
            String::from("Firmware"),
            String::from("Simulator"),
            String::from("Accelerometer"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("Serial Shell"),
        body: String::from(
            "The serial shell is a text-based command interface exposed over a
serial (USB UART) connection. It lets you type commands and read
diagnostics from the watch without a GUI.

The Shell tab here provides a terminal that talks to the watch over
the serial link. You can send commands, read the watch's response, and
see hardware log messages. This is invaluable for debugging a face
that behaves oddly on real hardware.

The shell complements the [[Firmware]]'s normal button-driven UI and is
commonly enabled for development builds.",
        ),
        links: vec![String::from("Firmware")],
    });

    pages.push(WikiPage {
        title: String::from("Firmware"),
        body: String::from(
            "Firmware is the software that runs on the watch's microcontroller.
It is the compiled result of all the source code: the boot logic, the
[[Watch Face]]s, the [[HAL]], and the settings handling.

In Firmware Studio you build the firmware from your [[Preset]] of
selected faces. The build produces a [[UF2]] file that the
[[Bootloader]] writes into [[Flash]].

The firmware is what makes the hardware come alive: it draws to the
display, reads the buttons, keeps time, and runs the faces you
selected.",
        ),
        links: vec![
            String::from("Watch Face"),
            String::from("HAL"),
            String::from("Preset"),
            String::from("UF2"),
            String::from("Bootloader"),
            String::from("Flash"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("Flash"),
        body: String::from(
            "Flash is the non-volatile memory where the [[Firmware]] is stored.
Unlike RAM, flash keeps its contents when power is removed, so the
watch remembers its program and settings.

Updating the firmware means writing a new image into flash. This is
done via the [[Bootloader]], which accepts a [[UF2]] file over USB and
programs it into flash.

Flash has a limited size and a limited number of write cycles. The
Build & Flash tab shows the estimated flash usage of your [[Preset]] so
you know you are not overflowing the chip.",
        ),
        links: vec![
            String::from("Firmware"),
            String::from("Bootloader"),
            String::from("UF2"),
            String::from("Preset"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("Simulator"),
        body: String::from(
            "The Simulator is a virtual version of the watch that runs on your
computer. It renders the display and emulates the buttons, so you can
try faces without flashing any hardware.

Because the faces are written against the [[HAL]], the simulator can
run the same code that would run on the real watch. You press the
L, C, and A buttons in the UI and watch the face react.

The simulator also shows the current time, which you can set to test
how a face behaves at different times or on different days. It is a
fast way to iterate on a [[Watch Face]] before committing to a
[[Flash]].",
        ),
        links: vec![
            String::from("HAL"),
            String::from("Watch Face"),
            String::from("Flash"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("Settings Register"),
        body: String::from(
            "The settings register is a small block of memory where the watch
keeps its persistent configuration: the current timezone, whether
[[DST]] is active, the selected sign mode, and other user options.

Faces and the [[Firmware]] read and write these settings so that your
choices survive a power cycle. Because it lives in [[Flash]] (or a
dedicated settings area), it is not lost when the watch turns off.

The settings are exposed in the Settings tab of Firmware Studio, where
you configure defaults that get baked into the [[Firmware]] when you
build.",
        ),
        links: vec![
            String::from("Firmware"),
            String::from("Flash"),
            String::from("DST"),
            String::from("Timezone"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("Timezone"),
        body: String::from(
            "The timezone is the local time offset from UTC that the watch uses
to display the time. The watch normally keeps time in UTC internally
and applies the timezone to show your local time.

You configure the timezone in Settings. The watch uses it along with
[[DST]] rules to derive the correct displayed time.

Getting the timezone right matters for accuracy: a wrong offset makes
every [[Watch Face]] that shows the time appear wrong even though the
underlying clock is fine. [[NTP]] can help set the correct reference
time, but the timezone is a local choice.",
        ),
        links: vec![
            String::from("DST"),
            String::from("Watch Face"),
            String::from("NTP"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("DST"),
        body: String::from(
            "DST (Daylight Saving Time) is the seasonal one-hour shift applied
to local time in many regions. The watch offers a DST setting so it
can add the extra hour when daylight saving is in effect.

You enable or disable DST based on where you are and the season. The
watch combines this with the [[Timezone]] offset to compute the local
time shown on the display.

Because DST rules vary by country and change over the years, the watch
keeps it as a simple on/off setting rather than a full rule database.
Check the [[Settings Register]] page for how these values are stored.",
        ),
        links: vec![String::from("Timezone"), String::from("Settings Register")],
    });

    pages.push(WikiPage {
        title: String::from("Accelerometer"),
        body: String::from(
            "The accelerometer is a motion sensor that measures acceleration
along one or more axes. On the Sensor-Watch it is used by faces that
detect movement, orientation, or gestures.

For example, a face can tell that the watch was lifted, tilted, or
shaken, and react accordingly. The sensor is read through the [[HAL]],
so the same face code works on the real watch and in the
[[Simulator]].

The accelerometer is a peripheral, separate from the timekeeping core,
so it is only used by faces that opt in to motion sensing.",
        ),
        links: vec![String::from("HAL"), String::from("Simulator")],
    });

    pages.push(WikiPage {
        title: String::from("LCD & Indicators"),
        body: String::from(
            "The LCD is the watch's segmented display. A segment is either
lit or unlit; it is not a general-purpose pixel screen. Faces choose
which segments to light, and the available segments are a hardware
constraint (see [[Character Set]] and [[Firmware & Studio Terms]]).

Time faces commonly use six character positions in HHMMSS order:
two for hours, two for minutes, and two for seconds. A colon between
HH and MM is an indicator, not one of those six positions. Other
indicators can show PM, 24H, or LAP. Their exact visibility depends on
the face and the display hardware.

The display's alphabetic glyphs are approximations made from segments.
A numeric 7 and an alphabetic T or t are different glyph requests. In
some character tables, the alphabetic glyph is drawn as a backward-7-
style shape, so it may look like a 7 even though it is not the numeric
7. See [[Watch Face]] for how a face uses the display.",
        ),
        links: vec![
            String::from("Character Set"),
            String::from("Firmware & Studio Terms"),
            String::from("Watch Face"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("Buttons"),
        body: String::from(
            "The physical controls are conventionally named L, C, and A:
Left, Center, and (usually) Alarm/right-side control. The labels are
button identities, not directions on the LCD. A face can assign its
own action to each button.

The input vocabulary distinguishes a press from a release and from a
held press. Down means the button became pressed; Up means it was
released. LongPress means a press was held long enough to count as a
long action, and LongUp means the release after that long press. The
precise threshold is supplied by the firmware/input layer, so a face
should handle the events it supports rather than assume every face
uses every event.

The [[Simulator]] exposes L, C, and A controls for trying these actions
without hardware. The [[HAL]] carries the same input idea to a real
watch, where switch bounce, timing, and the physical case can affect
what is practical.",
        ),
        links: vec![String::from("Simulator"), String::from("HAL")],
    });

    pages.push(WikiPage {
        title: String::from("Character Set"),
        body: String::from(
            "A character set is the table that maps a requested symbol to
segments on the LCD. Numeric glyphs are intended for digits such as
0 through 9. Alphabetic glyphs are a separate, limited set because a
segmented LCD cannot draw every letter clearly.

Do not infer a glyph from how it looks: numeric 7 is the digit used in
numbers, while alphabetic T or t may use a backward-7-style segment
pattern. The result can look similar on a small LCD, but the character
meaning and table entry are different. Unsupported letters may be
blank, substituted, or only approximate.

The [[LCD & Indicators]] page describes positions and indicators. A
face should use the character definitions supported by its target
hardware rather than expect a full computer font.",
        ),
        links: vec![String::from("LCD & Indicators")],
    });

    pages.push(WikiPage {
        title: String::from("Settings Terms"),
        body: String::from(
            "Watch settings are values the firmware reads while the watch
runs, such as time format, timezone, DST, or face choices. They are
part of the watch's runtime configuration and may be stored in the
watch's settings area; see [[Settings Register]].

Studio settings are controls for the desktop application: for example,
which repository or tool path Studio uses, what it displays, or which
build options it offers. A Studio setting is not automatically a watch
setting. Likewise, choosing a value in Studio does not prove that a
connected watch has received it.

A build can use selected defaults from Studio to produce [[Firmware]],
but the resulting behavior still needs to be checked in the appropriate
host simulation or on compatible hardware. See [[Timezone]], [[DST]],
and [[Simulator]] for examples.",
        ),
        links: vec![
            String::from("Settings Register"),
            String::from("Firmware"),
            String::from("Timezone"),
            String::from("DST"),
            String::from("Simulator"),
        ],
    });

    pages.push(WikiPage {
        title: String::from("Firmware & Studio Terms"),
        body: String::from(
            "Firmware is the program compiled for the watch. Studio is the
desktop tool that helps select faces, build firmware, inspect files,
and work with a device. A [[UF2]] is a packaged firmware file; the
[[Bootloader]] accepts it and writes it to [[Flash]]. A host-copy or
drag-and-drop step only copies a file to the bootloader's USB drive. It
is not, by itself, confirmation that the hardware rebooted and is
running the new firmware.

UART is a serial communication interface used for logs or a shell;
[[Serial Shell]] is one user-facing example. SWD (Serial Wire Debug)
is a separate hardware debug/programming connection. Neither term
means that Studio has successfully connected or validated a device.

A real-face is face code intended for the watch target. `face_sim` is
the host/simulator variant or entry point used to exercise a face on
the computer. [[Simulator]] is useful for iteration, but it cannot
prove LCD segment appearance, button feel, power behavior, sensor
accuracy, UART/SWD wiring, or other hardware-only properties.

Hardware has finite flash, a fixed LCD character/indicator layout, and
specific buttons and peripherals. A configured Studio build or a
successful host run does not remove those limitations. Check the
actual target and use [[Flash]] and [[Watch Face]] as workflow references.",
        ),
        links: vec![
            String::from("UF2"),
            String::from("Bootloader"),
            String::from("Flash"),
            String::from("Serial Shell"),
            String::from("Simulator"),
            String::from("Watch Face"),
        ],
    });

    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_declared_and_inline_link_targets_a_page() {
        let wiki = Wiki::new();
        for page in &wiki.pages {
            for link in &page.links {
                assert!(
                    wiki.page(link).is_some(),
                    "{} links to missing page {link}",
                    page.title
                );
            }
            let mut body = page.body.as_str();
            while let Some(start) = body.find("[[") {
                let target = &body[start + 2..];
                let Some(end) = target.find("]]") else {
                    panic!("unterminated link in {}", page.title)
                };
                let title = &target[..end];
                assert!(
                    wiki.page(title).is_some(),
                    "{} links to missing page {title}",
                    page.title
                );
                body = &target[end + 2..];
            }
        }
    }

    #[test]
    fn navigation_ignores_unknown_pages_and_backtracks_in_order() {
        let mut wiki = Wiki::new();
        wiki.navigate("not a page");
        assert_eq!(wiki.current, "Wiki Home");
        assert!(wiki.history.is_empty());

        wiki.navigate("UF2");
        wiki.navigate("Bootloader");
        assert_eq!(wiki.history, vec!["Wiki Home", "UF2"]);
        wiki.back();
        assert_eq!(wiki.current, "UF2");
        wiki.back();
        assert_eq!(wiki.current, "Wiki Home");
        assert!(wiki.history.is_empty());
    }
}
