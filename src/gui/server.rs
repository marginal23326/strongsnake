use std::{net::SocketAddr, str::FromStr, thread};

use crate::server::run_server_with_shutdown;

use super::state::{GuiServerHandle, SnakeGuiApp};

impl SnakeGuiApp {
    pub(super) fn start_server(&mut self) {
        if self.server_handle.is_some() {
            return;
        }
        let Ok(addr) = SocketAddr::from_str(self.server_addr.trim()) else {
            self.log_line("Invalid server address.");
            return;
        };
        let cfg = self.cfg.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let join = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
            rt.block_on(async move {
                let _ = run_server_with_shutdown(addr, cfg, async {
                    let _ = rx.await;
                })
                .await;
            });
        });
        self.server_handle = Some(GuiServerHandle {
            stop_tx: Some(tx),
            join: Some(join),
        });
        self.log_line("Server started.");
    }

    pub(super) fn stop_server(&mut self) {
        if let Some(mut handle) = self.server_handle.take() {
            if let Some(tx) = handle.stop_tx.take() {
                let _ = tx.send(());
            }
            if let Some(join) = handle.join.take() {
                let _ = join.join();
            }
            self.log_line("Server stopped.");
        }
    }
}

impl Drop for SnakeGuiApp {
    fn drop(&mut self) {
        self.stop_server();
    }
}
