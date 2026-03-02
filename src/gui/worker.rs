use std::{
    sync::mpsc::{self, Sender, TryRecvError},
    thread,
};

use eframe::egui;

use super::state::{SnakeGuiApp, WorkerMessage};

impl SnakeGuiApp {
    pub(super) fn start_worker<F>(&mut self, label: &str, job: F)
    where
        F: FnOnce(Sender<WorkerMessage>) + Send + 'static,
    {
        if self.worker_running {
            self.log_line(format!("A task is already running: {}", self.worker_label));
            return;
        }
        let (tx, rx) = mpsc::channel::<WorkerMessage>();
        self.worker_rx = Some(rx);
        self.worker_running = true;
        self.worker_label = label.to_owned();
        self.log_line(format!("Started {label}..."));
        thread::spawn(move || {
            job(tx);
        });
    }

    pub(super) fn run_async_job<T, F>(fut: F) -> Result<T, String>
    where
        F: std::future::Future<Output = anyhow::Result<T>>,
    {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        rt.block_on(fut).map_err(|e| e.to_string())
    }

    pub(super) fn poll_worker(&mut self, ctx: &egui::Context) {
        let mut messages = Vec::new();
        let mut worker_finished = false;
        let Some(rx) = self.worker_rx.as_ref() else {
            return;
        };

        loop {
            match rx.try_recv() {
                Ok(msg) => messages.push(msg),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    worker_finished = true;
                    break;
                }
            }
        }

        for msg in messages {
            let is_terminal = match msg {
                WorkerMessage::ArenaProgress(progress) => {
                    self.arena_progress = Some(progress);
                    false
                }
                WorkerMessage::Regression(result) => {
                    match result {
                        Ok(summary) => self.log_line(format!(
                            "Regression done: pass={} fail={} skipped={} scenarios={} total={}",
                            summary.passed,
                            summary.failed,
                            summary.skipped,
                            summary.scenarios,
                            summary.duration_display()
                        )),
                        Err(err) => self.log_line(format!("Regression failed: {err}")),
                    }
                    true
                }
                WorkerMessage::Arena(result) => {
                    match *result {
                        Ok(summary) => {
                            self.log_line(format!(
                                "Arena done: local={} opponent={} draws={} total={} duration={}ms",
                                summary.wins_local, summary.wins_opponent, summary.draws, summary.total_games, summary.duration_ms
                            ));
                            self.arena_summary = Some(summary);
                        }
                        Err(err) => self.log_line(format!("Arena failed: {err}")),
                    }
                    true
                }
                WorkerMessage::Trainer(result) => {
                    match *result {
                        Ok(summary) => self.log_line(format!(
                            "Trainer done: best_fitness={:.3} generation={}",
                            summary.best_fitness, summary.best_generation
                        )),
                        Err(err) => self.log_line(format!("Trainer failed: {err}")),
                    }
                    true
                }
            };
            if is_terminal {
                worker_finished = true;
                break;
            }
        }

        if worker_finished {
            self.worker_running = false;
            self.worker_label.clear();
            self.worker_rx = None;
        }

        if self.worker_running {
            ctx.request_repaint_after(self.worker_poll_interval);
        }
    }
}
