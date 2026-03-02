use eframe::egui::{self, Color32};
use snake_domain::Direction;

use super::super::state::SnakeGuiApp;

fn show_modal_dialog(ctx: &egui::Context, open_id: &str, title: &str, add_contents: impl FnOnce(&mut egui::Ui, &mut bool)) {
    let id = egui::Id::new(open_id);
    let mut is_open = ctx.data(|d| d.get_temp(id).unwrap_or(false));
    if !is_open {
        return;
    }

    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| add_contents(ui, &mut is_open));

    ctx.data_mut(|d| d.insert_temp(id, is_open));
}

impl SnakeGuiApp {
    pub(super) fn draw_scenario_load_dialog(&mut self, ctx: &egui::Context) {
        show_modal_dialog(ctx, "show_load_dialog", "Load Scenario or SFEN", |ui, is_open| {
            ui.horizontal(|ui| {
                ui.label("Input:");
                ui.add(egui::TextEdit::singleline(&mut self.scenario_load_path).desired_width(300.0));
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Load").clicked() {
                    self.load_scenario_from_path();
                    if self.load_error.is_none() {
                        *is_open = false;
                    }
                }
                if ui.button("Cancel").clicked() {
                    *is_open = false;
                    self.load_error = None;
                }
            });
            if let Some(err) = &self.load_error {
                ui.add_space(4.0);
                ui.colored_label(Color32::from_rgb(255, 123, 114), format!("Error: {}", err));
            }
        });
    }

    pub(super) fn draw_scenario_save_dialog(&mut self, ctx: &egui::Context) {
        show_modal_dialog(ctx, "show_save_dialog", "Save as Test Scenario", |ui, is_open| {
            ui.horizontal(|ui| {
                ui.label("Scenario Name:");
                ui.add(egui::TextEdit::singleline(&mut self.scenario_save_name).desired_width(200.0));
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Expected AI Move:");
                egui::ComboBox::from_id_salt("expected_move_cb")
                    .selected_text(self.scenario_expected_move.as_upper())
                    .show_ui(ui, |ui| {
                        for dir in Direction::ALL {
                            ui.selectable_value(&mut self.scenario_expected_move, dir, dir.as_upper());
                        }
                    });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save to disk").clicked() {
                    self.save_scenario_to_disk();
                }
                if ui.button("Close").clicked() {
                    *is_open = false;
                    self.save_error = None;
                    self.save_success = None;
                }
            });
            if let Some(err) = &self.save_error {
                ui.add_space(4.0);
                ui.colored_label(Color32::from_rgb(255, 123, 114), format!("Error: {}", err));
            }
            if let Some(msg) = &self.save_success {
                ui.add_space(4.0);
                ui.colored_label(Color32::from_rgb(126, 231, 135), msg);
            }
        });
    }
}
