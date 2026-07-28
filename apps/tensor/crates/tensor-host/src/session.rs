/// Host-session lifecycle transition delivered by the platform adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionEvent {
    Paused,
    Activated,
}
