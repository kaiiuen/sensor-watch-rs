//! Beginner-friendly block editor for the deliberately small, safe starter API.

use eframe::egui;
use serde::{Deserialize, Serialize};

const MAX_BLOCKS: usize = 256;
const MAX_TEXT_PARAMETER_CHARS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockKind {
    OnTick,
    OnLightPress,
    OnModePress,
    OnAlarmPress,
    DisplayText,
    DisplayTime,
    DisplayDate,
    ShowColon,
    ClearColon,
    IncrementCounter,
    ResetCounter,
}

impl BlockKind {
    pub const ALL: [Self; 11] = [
        Self::OnTick,
        Self::OnLightPress,
        Self::OnModePress,
        Self::OnAlarmPress,
        Self::DisplayText,
        Self::DisplayTime,
        Self::DisplayDate,
        Self::ShowColon,
        Self::ClearColon,
        Self::IncrementCounter,
        Self::ResetCounter,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::OnTick => "On Tick",
            Self::OnLightPress => "On Light press",
            Self::OnModePress => "On Mode press",
            Self::OnAlarmPress => "On Alarm press",
            Self::DisplayText => "Display text",
            Self::DisplayTime => "Display time",
            Self::DisplayDate => "Display date",
            Self::ShowColon => "Show colon",
            Self::ClearColon => "Clear colon",
            Self::IncrementCounter => "Increment counter",
            Self::ResetCounter => "Reset counter",
        }
    }

    fn default_parameter(self) -> String {
        match self {
            Self::DisplayText => "HELLO".into(),
            _ => String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub kind: BlockKind,
    #[serde(default)]
    pub parameter: String,
}

impl Block {
    pub fn new(kind: BlockKind) -> Self {
        Self {
            parameter: kind.default_parameter(),
            kind,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Blocks,
    Rust,
}

pub struct BlockEditor {
    blocks: Vec<Block>,
    selected_kind: BlockKind,
    mode: Mode,
    pub generated_source: String,
    notice: String,
}

impl Default for BlockEditor {
    fn default() -> Self {
        Self {
            blocks: vec![
                Block::new(BlockKind::OnTick),
                Block::new(BlockKind::DisplayTime),
            ],
            selected_kind: BlockKind::DisplayText,
            mode: Mode::Blocks,
            generated_source: String::new(),
            notice: String::new(),
        }
    }
}

impl BlockEditor {
    pub fn is_blocks_mode(&self) -> bool {
        self.mode == Mode::Blocks
    }

    pub fn set_blocks_mode(&mut self, blocks: bool) {
        self.mode = if blocks { Mode::Blocks } else { Mode::Rust };
    }

    pub fn show_blocks(&mut self, ui: &mut egui::Ui, rust_source: &mut String) {
        self.blocks_ui(ui, rust_source);
    }

    pub fn show(&mut self, ui: &mut egui::Ui, rust_source: &mut String) {
        ui.heading("Editor");
        ui.label("Blocks are a safe starter subset, not the complete firmware API.");
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.mode == Mode::Blocks, "Blocks")
                .clicked()
            {
                self.mode = Mode::Blocks;
            }
            if ui
                .selectable_label(self.mode == Mode::Rust, "Rust")
                .clicked()
            {
                self.mode = Mode::Rust;
            }
        });
        ui.separator();
        match self.mode {
            Mode::Blocks => self.blocks_ui(ui, rust_source),
            Mode::Rust => {
                ui.label("Advanced mode: edit the full Rust source directly.");
                ui.add(
                    egui::TextEdit::multiline(rust_source)
                        .code_editor()
                        .desired_rows(25)
                        .desired_width(f32::INFINITY),
                );
            }
        }
    }

    fn blocks_ui(&mut self, ui: &mut egui::Ui, rust_source: &mut String) {
        ui.weak("Use events to start a sequence, then add display or counter actions. Only these 11 blocks are generated.");
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_source("block-kind")
                .selected_text(self.selected_kind.label())
                .show_ui(ui, |ui| {
                    for kind in BlockKind::ALL {
                        ui.selectable_value(&mut self.selected_kind, kind, kind.label());
                    }
                });
            if ui.button("Add block").clicked() {
                if self.blocks.len() < MAX_BLOCKS {
                    self.blocks.push(Block::new(self.selected_kind));
                } else {
                    self.notice = format!("Block limit reached ({MAX_BLOCKS}).");
                }
            }
            if ui.button("Clear all").clicked() {
                self.blocks.clear();
            }
        });
        ui.add_space(6.0);
        let mut remove = None;
        let mut move_up = None;
        let mut move_down = None;
        let block_count = self.blocks.len();
        for index in 0..block_count {
            let can_move_down = index + 1 < block_count;
            let block = &mut self.blocks[index];
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.strong(format!("{}. {}", index + 1, block.kind.label()));
                    if ui.small_button("↑").clicked() && index > 0 {
                        move_up = Some(index);
                    }
                    if ui.small_button("↓").clicked() && can_move_down {
                        move_down = Some(index);
                    }
                    if ui.small_button("Delete").clicked() {
                        remove = Some(index);
                    }
                });
                if block.kind == BlockKind::DisplayText {
                    ui.horizontal(|ui| {
                        ui.label("Text:");
                        ui.add(
                            egui::TextEdit::singleline(&mut block.parameter)
                                .char_limit(MAX_TEXT_PARAMETER_CHARS),
                        );
                    });
                } else if matches!(
                    block.kind,
                    BlockKind::IncrementCounter | BlockKind::ResetCounter
                ) {
                    ui.label("Uses the starter counter value.");
                } else {
                    ui.weak("No parameters");
                }
            });
        }
        if let Some(index) = remove {
            self.blocks.remove(index);
        }
        if let Some(index) = move_up {
            self.blocks.swap(index, index - 1);
        }
        if let Some(index) = move_down {
            self.blocks.swap(index, index + 1);
        }

        ui.add_space(8.0);
        if ui.button("Generate Rust/source").clicked() {
            self.generated_source = generate_source(&self.blocks);
            self.notice = "Generated source is shown below. Rust text was not changed.".into();
        }
        if !self.notice.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_GREEN, &self.notice);
        }
        if !self.generated_source.is_empty() {
            ui.label("Preview (read-only):");
            ui.add(
                egui::TextEdit::multiline(&mut self.generated_source)
                    .code_editor()
                    .interactive(false)
                    .desired_rows(12)
                    .desired_width(f32::INFINITY),
            );
            let label = if rust_source.trim().is_empty() {
                "Load into Rust editor"
            } else {
                "Replace Rust editor with generated source"
            };
            if ui.button(label).clicked() {
                *rust_source = self.generated_source.clone();
                self.mode = Mode::Rust;
                self.notice = "Generated source loaded into Rust mode by explicit request.".into();
            }
        }
    }

    #[cfg(test)]
    fn blocks(&self) -> &[Block] {
        &self.blocks
    }
}

pub fn generate_source(blocks: &[Block]) -> String {
    let mut out = String::from(
        "// Generated by Studio Blocks. Safe starter subset only.\n// Review this source before saving or building.\n\n"
    );
    out.push_str(
        "use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};\n",
    );
    out.push_str("use crate::watch;\n\n");
    out.push_str("pub struct BlocksFace {\n    counter: u32,\n}\n\n");
    out.push_str("impl BlocksFace {\n    pub const fn new_static() -> Self {\n        Self { counter: 0 }\n    }\n\n    pub fn new() -> Self {\n        Self::new_static()\n    }\n}\n\n");
    out.push_str("impl WatchFace for BlocksFace {\n");
    out.push_str("    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}\n");
    out.push_str("    fn activate(&mut self, _settings: &Settings) {}\n");
    out.push_str("    fn loop_(&mut self, event: Event, _settings: &mut Settings) {\n        match event {\n");

    let mut arms: Vec<(String, Vec<&Block>)> = Vec::new();
    let mut actions_without_event = Vec::new();
    let mut current_arm: Option<usize> = None;
    for block in blocks {
        if let Some(pattern) = event_pattern(block.kind) {
            if let Some(index) = arms.iter().position(|(candidate, _)| candidate == pattern) {
                current_arm = Some(index);
            } else {
                arms.push((pattern.to_string(), Vec::new()));
                current_arm = Some(arms.len() - 1);
            }
        } else if let Some(index) = current_arm {
            arms[index].1.push(block);
        } else {
            actions_without_event.push(block);
        }
    }

    let has_event_arms = !arms.is_empty();
    for (pattern, actions) in arms {
        out.push_str("            ");
        out.push_str(&pattern);
        out.push_str(" => {\n");
        for block in actions {
            out.push_str(&action_source(block));
        }
        out.push_str("            }\n");
    }
    if !actions_without_event.is_empty() || !has_event_arms {
        out.push_str("            _ => {\n");
        for block in actions_without_event {
            out.push_str(&action_source(block));
        }
        out.push_str("            }\n");
    } else {
        out.push_str("            _ => {}\n");
    }
    out.push_str("        }\n    }\n");
    out.push_str("    fn resign(&mut self, _settings: &mut Settings) {}\n}\n");
    out
}

fn event_pattern(kind: BlockKind) -> Option<&'static str> {
    Some(match kind {
        BlockKind::OnTick => "Event::Tick",
        BlockKind::OnLightPress => "Event::Button(Button::Light, ButtonEvent::Up)",
        BlockKind::OnModePress => "Event::Button(Button::Mode, ButtonEvent::Up)",
        BlockKind::OnAlarmPress => "Event::Button(Button::Alarm, ButtonEvent::Up)",
        _ => return None,
    })
}

fn action_source(block: &Block) -> String {
    let body = match block.kind {
        BlockKind::DisplayText => format!(
            "watch::slcd::display_string(\"{}\", 0);",
            escape_string(&block.parameter)
        ),
        BlockKind::DisplayTime => "let now = watch::rtc::get_date_time();\n                let mut time = [b'0'; 4];\n                time[0] = b'0' + now.hour / 10;\n                time[1] = b'0' + now.hour % 10;\n                time[2] = b'0' + now.minute / 10;\n                time[3] = b'0' + now.minute % 10;\n                watch::slcd::display_string(core::str::from_utf8(&time).unwrap_or(\"\"), 0);".into(),
        BlockKind::DisplayDate => "let now = watch::rtc::get_date_time();\n                let mut date = [b'0'; 6];\n                date[0] = b'0' + now.day / 10;\n                date[1] = b'0' + now.day % 10;\n                date[2] = b'0' + now.month / 10;\n                date[3] = b'0' + now.month % 10;\n                date[4] = b'2';\n                date[5] = b'0' + now.year % 10;\n                watch::slcd::display_string(core::str::from_utf8(&date).unwrap_or(\"\"), 0);".into(),
        BlockKind::ShowColon => "watch::slcd::set_colon();".into(),
        BlockKind::ClearColon => "watch::slcd::clear_colon();".into(),
        BlockKind::IncrementCounter => "self.counter = self.counter.wrapping_add(1);".into(),
        BlockKind::ResetCounter => "self.counter = 0;".into(),
        _ => String::new(),
    };
    format!(
        "                // {}\n                {}\n",
        block.kind.label(),
        body
    )
}

fn escape_string(value: &str) -> String {
    value
        .chars()
        .take(MAX_TEXT_PARAMETER_CHARS)
        .flat_map(|character| character.escape_default())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serialization_round_trip_preserves_order_and_parameters() {
        let blocks = vec![
            Block::new(BlockKind::OnModePress),
            Block {
                kind: BlockKind::DisplayText,
                parameter: "A\"B".into(),
            },
        ];
        let json = serde_json::to_string(&blocks).unwrap();
        assert_eq!(serde_json::from_str::<Vec<Block>>(&json).unwrap(), blocks);
    }
    #[test]
    fn generation_is_ordered_and_escapes_text() {
        let blocks = vec![
            Block {
                kind: BlockKind::DisplayText,
                parameter: "A\"B".into(),
            },
            Block::new(BlockKind::ResetCounter),
        ];
        let source = generate_source(&blocks);
        assert!(
            source.find("display_string(\"A\\\"B\", 0)").unwrap()
                < source.find("self.counter = 0").unwrap()
        );
    }

    fn assert_balanced_braces(source: &str) {
        let mut depth = 0i32;
        for character in source.chars() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    assert!(depth >= 0, "closing brace without an opener");
                }
                _ => {}
            }
        }
        assert_eq!(depth, 0, "generated source has unbalanced braces");
    }

    #[test]
    fn every_starter_block_generates_structurally_valid_face_source() {
        let blocks: Vec<_> = BlockKind::ALL.into_iter().map(Block::new).collect();
        let source = generate_source(&blocks);

        assert_balanced_braces(&source);
        assert!(source.contains("impl WatchFace for BlocksFace"));
        for method in ["fn setup(", "fn activate(", "fn loop_(", "fn resign("] {
            assert!(source.contains(method), "missing {method}");
        }
        for call in [
            "display_string(\"HELLO\", 0)",
            "watch::rtc::get_date_time()",
            "watch::slcd::set_colon()",
            "watch::slcd::clear_colon()",
            "self.counter = self.counter.wrapping_add(1)",
            "self.counter = 0",
        ] {
            assert!(source.contains(call), "missing starter action {call}");
        }
    }

    #[test]
    fn event_combinations_have_one_arm_each_and_balanced_structure() {
        let events = [
            BlockKind::OnTick,
            BlockKind::OnLightPress,
            BlockKind::OnModePress,
            BlockKind::OnAlarmPress,
        ];
        let blocks: Vec<_> = events
            .into_iter()
            .flat_map(|event| [Block::new(event), Block::new(BlockKind::DisplayText)])
            .collect();
        let source = generate_source(&blocks);

        assert_balanced_braces(&source);
        assert_eq!(source.matches("Event::Tick => {").count(), 1);
        assert_eq!(
            source
                .matches("Event::Button(Button::Light, ButtonEvent::Up) => {")
                .count(),
            1
        );
        assert_eq!(
            source
                .matches("Event::Button(Button::Mode, ButtonEvent::Up) => {")
                .count(),
            1
        );
        assert_eq!(
            source
                .matches("Event::Button(Button::Alarm, ButtonEvent::Up) => {")
                .count(),
            1
        );
        assert_eq!(source.matches("_ => {").count(), 1);
    }
}
