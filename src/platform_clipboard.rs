use std::io;
use std::sync::mpsc;

/// Clipboard access backed by the event loop's existing Wayland connection.
pub struct WaylandClipboard {
    runtime: Rc<RefCell<PlatformBackend>>,
}

impl WaylandClipboard {
    fn new(runtime: Rc<RefCell<PlatformBackend>>) -> Self {
        Self { runtime }
    }

    pub fn backend(&self) -> &'static str {
        "wayland-wl-data-device"
    }

    pub fn store_async(
        &self,
        content: TransferContent,
    ) -> Result<mpsc::Receiver<io::Result<()>>, String> {
        let result = {
            let mut backend = self.runtime.borrow_mut();
            let Some(runtime) = backend.sctk_runtime_mut() else {
                return Err(
                    "clipboard store requires SCTK backend (unset FIKA_WAYLAND_BACKEND=native)"
                        .into(),
                );
            };
            runtime
                .store_selection(content)
                .map_err(|error| io::Error::other(error.to_string()))
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        reply_tx
            .send(result)
            .map_err(|_| "clipboard result receiver stopped".to_string())?;
        Ok(reply_rx)
    }

    pub fn store_text_async(
        &self,
        text: impl AsRef<str>,
    ) -> Result<mpsc::Receiver<io::Result<()>>, String> {
        self.store_async(TransferContent::text(text))
    }

    pub fn load_async(
        &self,
        preferred_mimes: &[&str],
    ) -> Result<mpsc::Receiver<io::Result<String>>, String> {
        let pipe = {
            let backend = self.runtime.borrow();
            let Some(runtime) = backend.sctk_runtime() else {
                return Err(
                    "clipboard load requires SCTK backend (unset FIKA_WAYLAND_BACKEND=native)"
                        .into(),
                );
            };
            runtime
                .receive_selection(preferred_mimes)
                .map_err(|error| error.to_string())?
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        thread::Builder::new()
            .name("fika-wayland-clipboard-read".to_string())
            .spawn(move || {
                let _ = reply_tx.send(pipe.read_text());
            })
            .map_err(|error| error.to_string())?;
        Ok(reply_rx)
    }
}

impl ActiveEventLoop {
    pub fn clipboard(&self) -> WaylandClipboard {
        WaylandClipboard::new(self.runtime.clone())
    }
}
