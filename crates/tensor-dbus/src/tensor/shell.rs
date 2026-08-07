//! Versioned Tensor Shell control interface.

use crate::{Connection, Result, freedesktop::mpris::MprisAction};

pub const DESTINATION: &str = "org.tensor.Shell1";
pub const MEDIA_PATH: &str = "/org/tensor/Shell1/Media";
pub const MEDIA_INTERFACE: &str = "org.tensor.Shell1.Media";

pub async fn perform_media_action(connection: &mut Connection, action: MprisAction) -> Result<()> {
    let (): () = connection
        .call(
            Some(DESTINATION),
            MEDIA_PATH,
            Some(MEDIA_INTERFACE),
            media_member(action),
            &(),
        )
        .await?;
    Ok(())
}

pub const fn media_member(action: MprisAction) -> &'static str {
    match action {
        MprisAction::Previous => "Previous",
        MprisAction::PlayPause => "PlayPause",
        MprisAction::Next => "Next",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_actions_have_stable_dbus_members() {
        assert_eq!(media_member(MprisAction::Previous), "Previous");
        assert_eq!(media_member(MprisAction::PlayPause), "PlayPause");
        assert_eq!(media_member(MprisAction::Next), "Next");
    }
}
