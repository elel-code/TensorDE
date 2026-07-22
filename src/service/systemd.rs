use std::io;

pub fn notify_ready() -> io::Result<()> {
    sd_notify::notify(&[
        sd_notify::NotifyState::Ready,
        sd_notify::NotifyState::Status("Tensor compositor initialized"),
    ])
}
