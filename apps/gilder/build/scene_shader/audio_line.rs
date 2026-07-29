//! Authored audio-line effect compiled through the shared Slang cold path.

pub(super) fn audio_line_fragment_source(texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x1);
    include_str!("../../shaders/scene/effects/audioline.slang").to_owned()
}
