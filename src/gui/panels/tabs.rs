use eframe::egui::{self, Color32, RichText};
use snake_api::ApiFlavor;
use snake_domain::Direction;

use crate::RegressionOutput;
use crate::services::{
    ArenaOptions, ArenaSummary, RegressionOptions, TrainerOptions, parse_arena_find_modes, parse_depths, run_arena_with_progress,
    run_regression_suite, run_trainer,
};

use super::super::state::{EditMode, SnakeGuiApp, WorkerMessage};
use super::ui_card;

impl SnakeGuiApp {
    pub(super) fn show_playground_tab(&mut self, ui: &mut egui::Ui) {
        // --- CONTROLS TOOLBAR ---
        ui.horizontal(|ui| {
            // Actions Card
            ui_card(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Evaluate").clicked() {
                        self.evaluate_ai();
                    }
                    if ui.button("Step").clicked() {
                        self.step_playground();
                    }

                    let play_btn = if self.auto_run {
                        egui::Button::new(RichText::new("Pause").color(Color32::from_rgb(255, 123, 114)))
                    } else {
                        egui::Button::new("Play")
                    };
                    if ui.add_enabled(self.auto_run || self.playground_can_run(), play_btn).clicked() {
                        self.set_playground_running(!self.auto_run);
                    }
                    if ui.button("Reset").clicked() {
                        self.reset_playground();
                    }

                    ui.separator();
                    ui.menu_button("Copy", |ui| {
                        if ui.button("SFEN").clicked() {
                            self.copy_state_to_clipboard(ui);
                            ui.close();
                        }
                        if ui.button("JSON").clicked() {
                            self.copy_json_state_to_clipboard(ui);
                            ui.close();
                        }
                    });

                    ui.separator();
                    ui.label("AI Depth:");
                    ui.add(egui::DragValue::new(&mut self.playground_depth).range(1..=64));

                    ui.separator();
                    ui.label("P1 Move:");
                    egui::ComboBox::from_id_salt("player_dir")
                        .selected_text(self.player_dir.as_upper())
                        .width(60.0)
                        .show_ui(ui, |ui| {
                            for dir in Direction::ALL {
                                if ui.selectable_label(self.player_dir == dir, dir.as_upper()).clicked() {
                                    if self.auto_run {
                                        self.queue_player_input(dir);
                                    } else {
                                        self.set_player_dir(dir);
                                    }
                                }
                            }
                        });
                });
            });

            // Editor Tools Card
            ui_card(ui, |ui| {
                ui.horizontal(|ui| {
                    for (label, mode) in [
                        ("Paint P1", EditMode::PaintP1),
                        ("Paint AI", EditMode::PaintAi),
                        ("Food", EditMode::Food),
                        ("Erase", EditMode::Erase),
                    ] {
                        if ui.selectable_label(self.edit_mode == mode, label).clicked() {
                            self.edit_mode = mode;
                        }
                    }
                });
            });
        });

        // --- PV LINE ---
        if !self.pv_line.is_empty() {
            ui.add_space(8.0);
            ui_card(ui, |ui| {
                let max_turns = self.pv_line.len() / 2;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Principal Variation:").strong());

                    if ui.button("<<").clicked() {
                        self.pv_index = 0;
                    }
                    if ui.button("<").clicked() {
                        self.pv_index = self.pv_index.saturating_sub(1);
                    }
                    ui.add(egui::DragValue::new(&mut self.pv_index).range(0..=max_turns));
                    if ui.button(">").clicked() {
                        self.pv_index = (self.pv_index + 1).min(max_turns);
                    }
                    if ui.button(">>").clicked() {
                        self.pv_index = max_turns;
                    }

                    ui.separator();

                    // Wrapping the PV list in a horizontal scroll area
                    egui::ScrollArea::horizontal().show(ui, |ui| {
                        let mut moves_str = String::new();
                        for (i, chunk) in self.pv_line.chunks(2).enumerate() {
                            if chunk.len() == 2 {
                                if i > 0 {
                                    moves_str.push_str(" | ");
                                }
                                moves_str.push_str(&format!("{} vs {}", chunk[0].as_upper(), chunk[1].as_upper()));
                            }
                        }
                        ui.label(RichText::new(moves_str).monospace());
                    });
                });
            });
        }

        ui.add_space(12.0);

        // --- BOARD ---
        self.draw_playground_board(ui);
    }

    pub(super) fn show_regression_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("Regression Suite").strong());
        ui.add_space(8.0);

        ui_card(ui, |ui| {
            egui::Grid::new("regression_grid").spacing([16.0, 12.0]).show(ui, |ui| {
                ui.label(RichText::new("Scenario Directory:").strong());
                ui.add(egui::TextEdit::singleline(&mut self.scenario_dir).min_size(egui::vec2(400.0, 0.0)));
                ui.end_row();

                ui.label(RichText::new("Depths to Test:").strong());
                ui.add(egui::TextEdit::singleline(&mut self.depths).min_size(egui::vec2(400.0, 0.0)));
                ui.end_row();

                ui.label(RichText::new("Repeats (Averaging):").strong());
                ui.add(egui::DragValue::new(&mut self.regression_repeat).range(1..=1000));
                ui.end_row();
            });

            ui.add_space(16.0);
            let run_btn = ui.add_enabled(!self.worker_running, egui::Button::new("Run Regression Suite"));
            if run_btn.clicked() {
                let depths = parse_depths(&self.depths);
                let cfg = self.cfg.clone();
                let scenario_dir = self.scenario_dir.clone();
                let repeat = self.regression_repeat;

                self.start_worker("regression", move |tx| {
                    let result = run_regression_suite(
                        cfg,
                        RegressionOptions {
                            scenario_dir: scenario_dir.into(),
                            depths,
                            output: RegressionOutput::FailuresOnly,
                            repeat,
                        },
                    )
                    .map_err(|e| e.to_string());
                    let _ = tx.send(WorkerMessage::Regression(result));
                });
            }
        });
    }

    pub(super) fn show_arena_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("Arena & Tournaments").strong());
        ui.add_space(8.0);

        ui_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Match Setup").strong());
                    ui.add_space(4.0);
                    egui::Grid::new("arena_grid_1").spacing([16.0, 8.0]).show(ui, |ui| {
                        ui.label("Total Games:");
                        ui.add(egui::DragValue::new(&mut self.arena_games).range(1..=1000));
                        ui.end_row();

                        ui.label("Initial Seed:");
                        ui.add(egui::DragValue::new(&mut self.arena_seed));
                        ui.end_row();

                        ui.label("Opponent Name/URL:");
                        ui.add(egui::TextEdit::singleline(&mut self.arena_opponent).min_size(egui::vec2(300.0, 0.0)));
                        ui.end_row();

                        ui.label("Self Play:");
                        ui.checkbox(&mut self.arena_self_play, "");
                        ui.end_row();
                    });
                });

                ui.separator();

                ui.vertical(|ui| {
                    ui.label(RichText::new("Advanced Options").strong());
                    ui.add_space(4.0);
                    egui::Grid::new("arena_grid_2").spacing([16.0, 8.0]).show(ui, |ui| {
                        ui.label("Find Modes:");
                        ui.add(egui::TextEdit::singleline(&mut self.arena_find_modes).min_size(egui::vec2(300.0, 0.0)));
                        ui.end_row();

                        ui.label("Snapshot File:");
                        ui.add(egui::TextEdit::singleline(&mut self.arena_snapshot_file).min_size(egui::vec2(300.0, 0.0)));
                        ui.end_row();

                        ui.label("Snapshot Ticks:");
                        ui.add(egui::DragValue::new(&mut self.arena_snapshot_ticks).range(1..=500));
                        ui.end_row();

                        ui.label("Modifiers:");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.arena_only_loss, "Only Loss");
                            ui.add_space(8.0);
                            ui.checkbox(&mut self.arena_resume, "Resume");
                        });
                        ui.end_row();
                    });
                });
            });

            ui.add_space(16.0);
            let run_btn = ui.add_enabled(!self.worker_running, egui::Button::new("Start Arena Tournament"));
            if run_btn.clicked() {
                let cfg = self.cfg.clone();
                self.arena_progress = None;
                self.arena_summary = None;
                let find_tokens = if self.arena_find_modes.trim().is_empty() {
                    Vec::new()
                } else {
                    vec![self.arena_find_modes.clone()]
                };
                let (find_modes, invalid_find_modes) = parse_arena_find_modes(&find_tokens);
                let opts = ArenaOptions {
                    games: self.arena_games,
                    seed: self.arena_seed,
                    width: 16,
                    height: 9,
                    max_turns: 2000,
                    opponent: self.arena_opponent.clone(),
                    self_play: self.arena_self_play,
                    api_flavor: ApiFlavor::Auto,
                    request_timeout_ms: 700,
                    payload_timeout_ms: 100,
                    find_modes,
                    invalid_find_modes,
                    only_loss: self.arena_only_loss,
                    resume: self.arena_resume,
                    snapshot_file: self.arena_snapshot_file.clone().into(),
                    snapshot_ticks: self.arena_snapshot_ticks.max(1),
                };
                self.start_worker("arena", move |tx| {
                    let tx_progress = tx.clone();
                    let result = Self::run_async_job(run_arena_with_progress(cfg, opts, move |progress| {
                        let _ = tx_progress.send(WorkerMessage::ArenaProgress(progress));
                    }));
                    let _ = tx.send(WorkerMessage::Arena(Box::new(result)));
                });
            }
        });

        if let Some(progress) = &self.arena_progress {
            ui.add_space(12.0);
            ui_card(ui, |ui| {
                ui.heading(RichText::new("Tournament Progress").strong().color(Color32::from_rgb(88, 166, 255)));
                ui.add_space(4.0);

                let pct = if progress.total_games > 0 {
                    (progress.completed_games as f32 / progress.total_games as f32) * 100.0
                } else {
                    0.0
                };

                ui.horizontal(|ui| {
                    ui.add(
                        egui::ProgressBar::new(pct / 100.0)
                            .desired_width(300.0)
                            .text(format!("{:.1}%", pct)),
                    );
                    ui.label(format!("Games: {} / {}", progress.completed_games, progress.total_games));
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(format!("Wins: {}", progress.wins_local)).on_hover_text("Local AI");
                    ui.separator();
                    ui.label(format!("Losses: {}", progress.wins_opponent)).on_hover_text("Opponent");
                    ui.separator();
                    ui.label(format!("Draws: {}", progress.draws));
                    ui.separator();
                    ui.label(RichText::new(format!("Win Rate: {:.2}%", progress.local_win_rate)).strong());
                });

                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "Last Game: Seed {} | {} Turns | Winner: {} | {:.0}ms",
                        progress.last_seed, progress.last_turns, progress.last_winner, progress.elapsed_ms
                    ))
                    .weak(),
                );
            });
        } else if self.worker_running && self.worker_label == "arena" {
            ui.add_space(12.0);
            ui.label(RichText::new("Arena starting...").color(Color32::from_rgb(88, 166, 255)));
        }

        if let Some(summary) = &self.arena_summary {
            ui.add_space(12.0);
            let snapshot = summary.clone();
            self.draw_arena_summary(ui, &snapshot);
        }
    }

    pub(super) fn show_trainer_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("Genetic Trainer").strong());
        ui.add_space(8.0);

        ui_card(ui, |ui| {
            egui::Grid::new("trainer_grid").spacing([16.0, 12.0]).show(ui, |ui| {
                ui.label(RichText::new("Population Size:").strong());
                ui.add(egui::DragValue::new(&mut self.trainer_pop).range(2..=300));
                ui.end_row();

                ui.label(RichText::new("Generations:").strong());
                ui.add(egui::DragValue::new(&mut self.trainer_gens).range(1..=1000));
                ui.end_row();

                ui.label(RichText::new("Games per Eval:").strong());
                ui.add(egui::DragValue::new(&mut self.trainer_games).range(1..=100));
                ui.end_row();

                ui.label(RichText::new("Seed:").strong());
                ui.add(egui::DragValue::new(&mut self.trainer_seed));
                ui.end_row();
            });

            ui.add_space(16.0);
            let run_btn = ui.add_enabled(!self.worker_running, egui::Button::new("Start Trainer"));
            if run_btn.clicked() {
                let cfg = self.cfg.clone();
                let opts = TrainerOptions::for_gui(
                    self.trainer_pop,
                    self.trainer_gens,
                    self.trainer_games,
                    self.trainer_seed,
                    self.cfg.max_depth,
                );
                self.start_worker("trainer", move |tx| {
                    let result = Self::run_async_job(run_trainer(cfg, opts));
                    let _ = tx.send(WorkerMessage::Trainer(Box::new(result)));
                });
            }
        });
    }

    pub(super) fn show_server_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("Battle Server API").strong());
        ui.add_space(8.0);

        ui_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Bind Address:").strong());
                ui.add(egui::TextEdit::singleline(&mut self.server_addr).desired_width(200.0));
            });

            ui.add_space(16.0);

            let running = self.server_handle.is_some();
            ui.horizontal(|ui| {
                if !running {
                    if ui.button("Start Server").clicked() {
                        self.start_server();
                    }
                    ui.label(RichText::new("Stopped").color(Color32::from_rgb(139, 148, 158)));
                } else {
                    if ui.button("Stop Server").clicked() {
                        self.stop_server();
                    }
                    ui.label(RichText::new("Running").color(Color32::from_rgb(88, 166, 255)));
                }
            });
        });
    }

    fn draw_arena_summary(&self, ui: &mut egui::Ui, summary: &ArenaSummary) {
        ui_card(ui, |ui| {
            ui.heading(RichText::new("Arena Results Summary").strong());
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Total Games: {}", summary.total_games)).strong());
                ui.separator();
                ui.label(format!("Local Wins: {} ({:.2}%)", summary.wins_local, summary.local_win_rate));
                ui.separator();
                ui.label(format!(
                    "Opponent Wins: {} ({:.2}%)",
                    summary.wins_opponent, summary.opponent_win_rate
                ));
                ui.separator();
                ui.label(format!("Draws: {} ({:.2}%)", summary.draws, summary.draw_rate));
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Averages:").strong());
                ui.label(format!("Turns: {:.1}", summary.avg_turns));
                ui.separator();
                ui.label(format!("Local Len: {:.1}", summary.avg_local_length));
                ui.separator();
                ui.label(format!("Opponent Len: {:.1}", summary.avg_opponent_length));
                ui.separator();
                ui.label(format!("Total Duration: {}ms", summary.duration_ms));
            });

            ui.add_space(8.0);
            if let Some(shortest) = &summary.shortest_turn_game {
                ui.label(format!("Shortest game: {} turns (Seed: {})", shortest.turns, shortest.seed));
            }
            if let Some(longest) = &summary.longest_turn_game {
                ui.label(format!("Longest game: {} turns (Seed: {})", longest.turns, longest.seed));
            }

            ui.separator();
            ui.label(RichText::new("Death Analysis").strong());
            ui.add_space(4.0);
            egui::Grid::new("arena_death_grid")
                .num_columns(5)
                .spacing([24.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    let strong_lbl = |u: &mut egui::Ui, text: &str| u.label(RichText::new(text).strong());
                    strong_lbl(ui, "");
                    strong_lbl(ui, "Starvation");
                    strong_lbl(ui, "Wall");
                    strong_lbl(ui, "Body");
                    strong_lbl(ui, "Head Collision");
                    ui.end_row();

                    ui.label(RichText::new("Local").strong());
                    ui.label(summary.death_stats.local.starvation.to_string());
                    ui.label(summary.death_stats.local.wall.to_string());
                    ui.label(summary.death_stats.local.body.to_string());
                    ui.label(summary.death_stats.local.head.to_string());
                    ui.end_row();

                    ui.label(RichText::new("Opponent").strong());
                    ui.label(summary.death_stats.opponent.starvation.to_string());
                    ui.label(summary.death_stats.opponent.wall.to_string());
                    ui.label(summary.death_stats.opponent.body.to_string());
                    ui.label(summary.death_stats.opponent.head.to_string());
                    ui.end_row();
                });

            ui.separator();
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Turn Distribution").strong());
                    ui.add_space(4.0);
                    egui::Grid::new("arena_turn_dist")
                        .num_columns(3)
                        .spacing([16.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(RichText::new("Turns").strong());
                            ui.label(RichText::new("Count").strong());
                            ui.label(RichText::new("%").strong());
                            ui.end_row();
                            for bin in &summary.turn_distribution {
                                ui.label(&bin.label);
                                ui.label(bin.count.to_string());
                                ui.label(format!("{:.1}%", bin.percent));
                                ui.end_row();
                            }
                        });
                });

                ui.add_space(32.0);

                ui.vertical(|ui| {
                    ui.label(RichText::new("Length Distribution").strong());
                    ui.add_space(4.0);
                    egui::Grid::new("arena_len_dist")
                        .num_columns(5)
                        .spacing([16.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            let sl = |u: &mut egui::Ui, text: &str| u.label(RichText::new(text).strong());
                            sl(ui, "Length");
                            sl(ui, "Local");
                            sl(ui, "Local %");
                            sl(ui, "Opp");
                            sl(ui, "Opp %");
                            ui.end_row();
                            for (local, opponent) in summary
                                .local_length_distribution
                                .iter()
                                .zip(summary.opponent_length_distribution.iter())
                            {
                                ui.label(&local.label);
                                ui.label(local.count.to_string());
                                ui.label(format!("{:.1}%", local.percent));
                                ui.label(opponent.count.to_string());
                                ui.label(format!("{:.1}%", opponent.percent));
                                ui.end_row();
                            }
                        });
                });
            });

            if !summary.find_results.is_empty() {
                ui.separator();
                ui.label(RichText::new("Find Results").strong());
                ui.add_space(4.0);
                for found in &summary.find_results {
                    ui.label(
                        RichText::new(format!("{} - {}", found.mode_title, found.metric_label))
                            .strong()
                            .color(Color32::from_rgb(240, 246, 252)),
                    );
                    ui.label(format!("Winner: {}", found.winner));
                    ui.label(format!("Reproduce: {}", found.reproduce_hint));
                    if let Some(resume) = &found.resume_hint {
                        ui.label(format!("Resume: {}", resume));
                    }
                    ui.add_space(8.0);
                }
            }
        });
    }
}
