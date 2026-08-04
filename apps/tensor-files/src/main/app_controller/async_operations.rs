fn receive_clipboard_reply<T>(
    reply_rx: std::sync::mpsc::Receiver<std::io::Result<T>>,
) -> Result<T, String> {
    reply_rx
        .recv()
        .map_err(|_| "clipboard worker stopped before replying".to_string())?
        .map_err(|error| error.to_string())
}

include!("async_operations/lifecycle.rs");
include!("async_operations/submit.rs");
