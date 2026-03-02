use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Vec2};
use snake_ai::AiConfig;

use super::state::SnakeGuiApp;

impl eframe::App for SnakeGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        const AUTO_TICK_INTERVAL: Duration = Duration::from_millis(220);

        self.poll_worker(ctx);
        self.sync_playground_playback();
        self.process_playground_keys(ctx);

        if self.auto_run {
            if self.last_auto_tick.elapsed() >= AUTO_TICK_INTERVAL {
                self.step_playground();
                self.last_auto_tick = Instant::now();
            }
            let until_next_tick = AUTO_TICK_INTERVAL.saturating_sub(self.last_auto_tick.elapsed());
            ctx.request_repaint_after(until_next_tick);
        }

        self.draw_top_panel(ctx);
        self.draw_logs_panel(ctx);
        self.draw_central_panel(ctx);
    }
}

pub fn run_gui(cfg: AiConfig) -> Result<()> {
    // Fits neatly into a 1366x768 laptop screen with breathing room
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(Vec2::new(1200.0, 700.0))
            .with_min_inner_size(Vec2::new(800.0, 600.0))
            .with_title("Snake AI Lab"),
        ..Default::default()
    };

    eframe::run_native(
        "Snake Lab Rust",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.style_mut(|style| {
                style.visuals = egui::Visuals::dark();

                // --- Modern Dark Palette ---
                // App background
                style.visuals.panel_fill = Color32::from_rgb(13, 17, 23);
                // Card/Window background
                style.visuals.window_fill = Color32::from_rgb(22, 27, 34);
                // Text input, scroll bg
                style.visuals.extreme_bg_color = Color32::from_rgb(1, 4, 9);
                // Default Text color (Crisp light gray-blue)
                style.visuals.override_text_color = Some(Color32::from_rgb(201, 209, 217));

                // --- Widget Styles (Card-style & hierarchy) ---
                let corner_radius = CornerRadius::same(6);

                // Non-interactive (Frames, Panels, basic containers)
                style.visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(22, 27, 34);
                style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(48, 54, 61));
                style.visuals.widgets.noninteractive.corner_radius = corner_radius;

                // Inactive (Resting Buttons, Checkboxes, etc.)
                style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(33, 38, 45);
                style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(48, 54, 61));
                style.visuals.widgets.inactive.corner_radius = corner_radius;
                style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(201, 209, 217)); // Text

                // Hovered
                style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 54, 61);
                style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(139, 148, 158));
                style.visuals.widgets.hovered.corner_radius = corner_radius;
                style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::from_rgb(240, 246, 252));

                // Active / Clicked
                style.visuals.widgets.active.bg_fill = Color32::from_rgb(31, 111, 235);
                style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(88, 166, 255));
                style.visuals.widgets.active.corner_radius = corner_radius;
                style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

                // Open (Dropdowns, Menus)
                style.visuals.widgets.open.bg_fill = Color32::from_rgb(33, 38, 45);
                style.visuals.widgets.open.bg_stroke = Stroke::new(1.0, Color32::from_rgb(139, 148, 158));
                style.visuals.widgets.open.corner_radius = corner_radius;

                // Selection (Text selection, Active tabs)
                style.visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(31, 111, 235, 80);
                style.visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(88, 166, 255));

                // --- Spacing & Layout ---
                style.spacing.item_spacing = Vec2::new(10.0, 10.0); // Better breathing room
                style.spacing.button_padding = Vec2::new(12.0, 6.0); // Plump, modern buttons
                style.spacing.window_margin = Margin::same(12);
                style.spacing.menu_margin = Margin::same(8);
                style.spacing.interact_size = Vec2::new(40.0, 24.0); // Slightly taller interaction targets
            });

            Ok(Box::new(SnakeGuiApp::new(cfg.clone())))
        }),
    )
    .map_err(|e| anyhow!(e.to_string()))?;
    Ok(())
}
