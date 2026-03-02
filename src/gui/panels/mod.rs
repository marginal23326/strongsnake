use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke};

use super::state::{SnakeGuiApp, Tab};

mod dialogs;
mod tabs;

/// Helper to draw a modern "Card" container with a background, border, and padding.
pub(super) fn ui_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    Frame::new()
        .fill(ui.visuals().window_fill)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(12))
        .show(ui, add_contents)
}

impl SnakeGuiApp {
    pub(super) fn draw_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top")
            .frame(
                Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(Margin::symmetric(16, 8))
                    .stroke(Stroke::new(1.0, ctx.style().visuals.widgets.noninteractive.bg_stroke.color)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // --- 1. File Menu for Scenario Loading & Saving ---
                    ui.menu_button("File", |ui| {
                        if ui.button("Load Scenario...").clicked() {
                            ui.data_mut(|d| d.insert_temp(egui::Id::new("show_load_dialog"), true));
                            ui.close();
                        }
                        if ui.button("Save Scenario...").clicked() {
                            ui.data_mut(|d| d.insert_temp(egui::Id::new("show_save_dialog"), true));
                            self.save_error = None;
                            self.save_success = None;
                            ui.close();
                        }
                    });

                    ui.add_space(8.0);

                    // --- 2. Tab Navigation ---
                    egui::Frame::new()
                        .fill(Color32::from_rgb(1, 4, 9))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(4, 4))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(48, 54, 61)))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                for (tab, label) in [
                                    (Tab::Playground, "Playground"),
                                    (Tab::Regression, "Regression"),
                                    (Tab::Arena, "Arena"),
                                    (Tab::Trainer, "Trainer"),
                                    (Tab::Server, "Server"),
                                ] {
                                    let selected = self.tab == tab;
                                    if ui.selectable_label(selected, label).clicked() {
                                        self.tab = tab;
                                    }
                                }
                            });
                        });

                    // --- 3. Right-Aligned Stats & Toggles ---
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut show_logs = ui.data(|d| d.get_temp(egui::Id::new("show_logs")).unwrap_or(false));
                        if ui.toggle_value(&mut show_logs, "Logs").clicked() {
                            ui.data_mut(|d| d.insert_temp(egui::Id::new("show_logs"), show_logs));
                        }

                        ui.separator();

                        if self.tab == Tab::Playground {
                            let snake_stats = |id: &str| {
                                self.sim_state
                                    .board
                                    .snakes
                                    .iter()
                                    .find(|s| s.id.0 == id)
                                    .map(|s| (s.health, s.body.len()))
                                    .unwrap_or((0, 0))
                            };
                            let (s1_hp, s1_len) = snake_stats("s1");
                            let (s2_hp, s2_len) = snake_stats("s2");

                            ui.label(format!("{:.1}ms", self.last_move_ms)).on_hover_text("Last AI move time");
                            ui.separator();

                            ui.label(RichText::new(format!("Turn {}", self.sim_state.turn)).strong());
                            ui.separator();

                            ui.label(format!("HP:{} Len:{}", s2_hp, s2_len));
                            ui.label(RichText::new("AI").strong().color(Color32::from_rgb(255, 123, 114)));
                            ui.separator();

                            ui.label(format!("HP:{} Len:{}", s1_hp, s1_len));
                            ui.label(RichText::new("P1").strong().color(Color32::from_rgb(88, 166, 255)));
                        } else if self.worker_running {
                            ui.label(RichText::new(format!("Running: {}", self.worker_label)).color(Color32::from_rgb(88, 166, 255)));
                        } else {
                            ui.label(RichText::new("Idle").weak());
                        }
                    });
                });
            });
    }

    pub(super) fn draw_logs_panel(&mut self, ctx: &egui::Context) {
        let show_logs = ctx.data(|d| d.get_temp(egui::Id::new("show_logs")).unwrap_or(false));
        if !show_logs {
            return;
        }

        egui::SidePanel::right("logs")
            .resizable(true)
            .default_width(340.0)
            .min_width(280.0)
            .frame(
                Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(Margin::same(12))
                    .stroke(Stroke::new(1.0, ctx.style().visuals.widgets.noninteractive.bg_stroke.color)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("System Logs").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Clear").clicked() {
                            self.logs.clear();
                        }
                    });
                });

                ui.add_space(8.0);

                ui_card(ui, |ui| {
                    ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::multiline(&mut self.logs)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .frame(false)
                            .lock_focus(true),
                    );
                });
            });
    }

    pub(super) fn draw_central_panel(&mut self, ctx: &egui::Context) {
        self.draw_scenario_load_dialog(ctx);
        self.draw_scenario_save_dialog(ctx);

        let frame = Frame::new().fill(ctx.style().visuals.panel_fill).inner_margin(Margin::same(16));

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            if self.tab != Tab::Playground {
                egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                    Tab::Regression => self.show_regression_tab(ui),
                    Tab::Arena => self.show_arena_tab(ui),
                    Tab::Trainer => self.show_trainer_tab(ui),
                    Tab::Server => self.show_server_tab(ui),
                    _ => {}
                });
            } else {
                self.show_playground_tab(ui);
            }
        });
    }
}
