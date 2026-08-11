//! Beginner-friendly block editor for the deliberately small, safe starter API.

use eframe::egui;
use serde::{Deserialize, Serialize};

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
                self.blocks.push(Block::new(self.selected_kind));
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
                        ui.text_edit_singleline(&mut block.parameter);
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
    let mut out = String::from("// Generated by Studio Blocks. Safe starter subset only.\n// Review this source before saving or building.\n\nfn loop_(&mut self, event: Event) {\n");
    for block in blocks {
        let line = match block.kind {
            BlockKind::OnTick => "    // On Tick\n    Event::Tick => {\n",
            BlockKind::OnLightPress => {
                "    // On Light press\n    Event::Button(Button::Light, ButtonEvent::Press) => {\n"
            }
            BlockKind::OnModePress => {
                "    // On Mode press\n    Event::Button(Button::Mode, ButtonEvent::Press) => {\n"
            }
            BlockKind::OnAlarmPress => {
                "    // On Alarm press\n    Event::Button(Button::Alarm, ButtonEvent::Press) => {\n"
            }
            BlockKind::DisplayText => "    // Display text\n    display_text(\"PARAM\");\n",
            BlockKind::DisplayTime => "    // Display time\n    display_time();\n",
            BlockKind::DisplayDate => "    // Display date\n    display_date();\n",
            BlockKind::ShowColon => "    // Show colon\n    show_colon();\n",
            BlockKind::ClearColon => "    // Clear colon\n    clear_colon();\n",
            BlockKind::IncrementCounter => "    // Increment counter\n    counter += 1;\n",
            BlockKind::ResetCounter => "    // Reset counter\n    counter = 0;\n",
        };
        out.push_str(&if block.kind == BlockKind::DisplayText {
            line.replace("PARAM", &escape_string(&block.parameter))
        } else {
            line.into()
        });
    }
    out.push_str("    }\n}\n");
    out
}

fn escape_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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
            source.find("display_text(\"A\\\"B\")").unwrap() < source.find("counter = 0").unwrap()
        );
    }
}
