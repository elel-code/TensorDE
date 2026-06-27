//! Codec reference planning and streaming bootstrap helpers.
//!
//! This isolates the DPB/reference-plan state from the renderer loop, matching
//! the FFmpeg-style split between parsed access units, decoder state, and frame
//! presentation.

#![allow(dead_code)]

use super::super::*;
use vulkanalia::vk;

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanH264PictureFieldKind {
    Frame,
    TopField,
    BottomField,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanH264PictureFieldKind {
    pub(in crate::renderer::native_vulkan) fn from_flags(
        field_pic_flag: bool,
        bottom_field_flag: bool,
    ) -> Self {
        if !field_pic_flag {
            Self::Frame
        } else if bottom_field_flag {
            Self::BottomField
        } else {
            Self::TopField
        }
    }

    pub(in crate::renderer::native_vulkan) fn from_slice(
        slice: &NativeVulkanH264AccessUnitSliceSnapshot,
    ) -> Self {
        Self::from_flags(slice.field_pic_flag, slice.bottom_field_flag)
    }

    pub(in crate::renderer::native_vulkan) fn field_pic_flag(self) -> bool {
        !matches!(self, Self::Frame)
    }

    pub(in crate::renderer::native_vulkan) fn bottom_field_flag(self) -> bool {
        matches!(self, Self::BottomField)
    }

    pub(in crate::renderer::native_vulkan) fn opposite_field(self) -> Self {
        match self {
            Self::Frame => Self::Frame,
            Self::TopField => Self::BottomField,
            Self::BottomField => Self::TopField,
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH264ShortTermPictureKey {
    pub(in crate::renderer::native_vulkan) frame_num: u16,
    pub(in crate::renderer::native_vulkan) field_kind: NativeVulkanH264PictureFieldKind,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanH264ShortTermPictureKey {
    pub(in crate::renderer::native_vulkan) fn frame(frame_num: u16) -> Self {
        Self {
            frame_num,
            field_kind: NativeVulkanH264PictureFieldKind::Frame,
        }
    }

    pub(in crate::renderer::native_vulkan) fn from_slice(
        slice: &NativeVulkanH264AccessUnitSliceSnapshot,
    ) -> Self {
        Self {
            frame_num: slice.frame_num,
            field_kind: NativeVulkanH264PictureFieldKind::from_flags(
                slice.field_pic_flag,
                slice.bottom_field_flag,
            ),
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanH264ReferenceListEntry {
    ShortTerm(NativeVulkanH264ShortTermPictureKey),
    LongTerm(NativeVulkanH264LongTermPictureKey),
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanH264DpbSlotKey {
    ShortTerm(NativeVulkanH264ShortTermPictureKey),
    LongTerm(NativeVulkanH264LongTermPictureKey),
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH264LongTermPictureKey {
    pub(in crate::renderer::native_vulkan) frame_idx: u16,
    pub(in crate::renderer::native_vulkan) field_kind: NativeVulkanH264PictureFieldKind,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanH264LongTermPictureKey {
    pub(in crate::renderer::native_vulkan) fn from_short_term(
        key: NativeVulkanH264ShortTermPictureKey,
        long_term_frame_idx: u16,
    ) -> Self {
        Self {
            frame_idx: long_term_frame_idx,
            field_kind: key.field_kind,
        }
    }

    pub(in crate::renderer::native_vulkan) fn from_slice(
        slice: &NativeVulkanH264AccessUnitSliceSnapshot,
        long_term_frame_idx: u16,
    ) -> Self {
        Self {
            frame_idx: long_term_frame_idx,
            field_kind: NativeVulkanH264PictureFieldKind::from_slice(slice),
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
const NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES: usize = 64;

#[cfg(feature = "native-vulkan-video")]
const NATIVE_VULKAN_H264_EMPTY_SHORT_TERM_KEY: NativeVulkanH264ShortTermPictureKey =
    NativeVulkanH264ShortTermPictureKey {
        frame_num: 0,
        field_kind: NativeVulkanH264PictureFieldKind::Frame,
    };

#[cfg(feature = "native-vulkan-video")]
const NATIVE_VULKAN_H264_EMPTY_LONG_TERM_KEY: NativeVulkanH264LongTermPictureKey =
    NativeVulkanH264LongTermPictureKey {
        frame_idx: 0,
        field_kind: NativeVulkanH264PictureFieldKind::Frame,
    };

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanH264ReferenceListEntry {
    const EMPTY: Self = Self::ShortTerm(NATIVE_VULKAN_H264_EMPTY_SHORT_TERM_KEY);
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH264ReferenceListEntries {
    entries: [NativeVulkanH264ReferenceListEntry; NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES],
    len: u8,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanH264ReferenceListEntries {
    pub(in crate::renderer::native_vulkan) fn new() -> Self {
        Self {
            entries: [NativeVulkanH264ReferenceListEntry::EMPTY;
                NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES],
            len: 0,
        }
    }

    pub(in crate::renderer::native_vulkan) fn len(&self) -> usize {
        usize::from(self.len)
    }

    pub(in crate::renderer::native_vulkan) fn push(
        &mut self,
        entry: NativeVulkanH264ReferenceListEntry,
    ) -> Result<(), String> {
        let len = self.len();
        if len == NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES {
            return Err(format!(
                "H.264 reference list exceeds FFmpeg fixed capacity {NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES}"
            ));
        }
        self.entries[len] = entry;
        self.len += 1;
        Ok(())
    }

    pub(in crate::renderer::native_vulkan) fn insert(
        &mut self,
        index: usize,
        entry: NativeVulkanH264ReferenceListEntry,
    ) -> Result<(), String> {
        let len = self.len();
        if len == NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES {
            return Err(format!(
                "H.264 reference list modification exceeds FFmpeg fixed capacity {NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES}"
            ));
        }
        let index = index.min(len);
        for move_index in (index..len).rev() {
            self.entries[move_index + 1] = self.entries[move_index];
        }
        self.entries[index] = entry;
        self.len += 1;
        Ok(())
    }

    pub(in crate::renderer::native_vulkan) fn retain(
        &mut self,
        mut keep: impl FnMut(&NativeVulkanH264ReferenceListEntry) -> bool,
    ) {
        let len = self.len();
        let mut write_index = 0usize;
        for read_index in 0..len {
            let entry = self.entries[read_index];
            if keep(&entry) {
                self.entries[write_index] = entry;
                write_index += 1;
            }
        }
        self.len = u8::try_from(write_index).unwrap_or(u8::MAX);
    }

    pub(in crate::renderer::native_vulkan) fn truncate(&mut self, len: usize) {
        self.len = u8::try_from(self.len().min(len)).unwrap_or(u8::MAX);
    }

    pub(in crate::renderer::native_vulkan) fn extend_from_entries(
        &mut self,
        entries: &Self,
    ) -> Result<(), String> {
        for entry in entries.iter().copied() {
            self.push(entry)?;
        }
        Ok(())
    }

    pub(in crate::renderer::native_vulkan) fn contains(
        &self,
        entry: &NativeVulkanH264ReferenceListEntry,
    ) -> bool {
        self.as_slice().contains(entry)
    }

    pub(in crate::renderer::native_vulkan) fn iter(
        &self,
    ) -> std::slice::Iter<'_, NativeVulkanH264ReferenceListEntry> {
        self.as_slice().iter()
    }

    pub(in crate::renderer::native_vulkan) fn as_slice(
        &self,
    ) -> &[NativeVulkanH264ReferenceListEntry] {
        &self.entries[..self.len()]
    }

    pub(in crate::renderer::native_vulkan) fn as_mut_slice(
        &mut self,
    ) -> &mut [NativeVulkanH264ReferenceListEntry] {
        let len = self.len();
        &mut self.entries[..len]
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Default for NativeVulkanH264ReferenceListEntries {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH264DpbReferenceState {
    pub(in crate::renderer::native_vulkan) source_access_unit_index: Option<u32>,
    pub(in crate::renderer::native_vulkan) dpb_slot: u32,
    pub(in crate::renderer::native_vulkan) pic_order_cnt_val: i32,
    pub(in crate::renderer::native_vulkan) pic_order_cnt: [i32; 2],
    pub(in crate::renderer::native_vulkan) frame_num: u16,
    pub(in crate::renderer::native_vulkan) field_kind: NativeVulkanH264PictureFieldKind,
    pub(in crate::renderer::native_vulkan) non_existing: bool,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Default)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH264InferredNonExistingPlan {
    pub(in crate::renderer::native_vulkan) frame_nums: Vec<u16>,
    pub(in crate::renderer::native_vulkan) references:
        Vec<NativeVulkanH264InferredNonExistingReferenceSnapshot>,
    pub(in crate::renderer::native_vulkan) dropped_short_term_frame_nums: Vec<u16>,
    pub(in crate::renderer::native_vulkan) dropped_long_term_frame_indices: Vec<u16>,
    pub(in crate::renderer::native_vulkan) dropped_reference_slots: Vec<u32>,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Default)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH264AdaptiveMarkingPlan {
    pub(in crate::renderer::native_vulkan) drop_short_term_keys:
        Vec<NativeVulkanH264ShortTermPictureKey>,
    pub(in crate::renderer::native_vulkan) drop_long_term_keys:
        Vec<NativeVulkanH264LongTermPictureKey>,
    pub(in crate::renderer::native_vulkan) convert_short_term_to_long_term:
        Vec<(NativeVulkanH264ShortTermPictureKey, u16)>,
    pub(in crate::renderer::native_vulkan) current_long_term_frame_idx: Option<u16>,
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_short_term_pic_num(
    frame_num: u16,
    current_frame_num: u16,
    max_frame_num: u32,
) -> i32 {
    let max_frame_num = i32::try_from(max_frame_num.max(1)).unwrap_or(i32::MAX);
    let frame_num = i32::from(frame_num);
    let current_frame_num = i32::from(current_frame_num);
    if frame_num > current_frame_num {
        frame_num.saturating_sub(max_frame_num)
    } else {
        frame_num
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_current_pic_num(
    current_frame_num: u16,
    current_field_kind: NativeVulkanH264PictureFieldKind,
) -> i64 {
    let frame_num = i64::from(current_frame_num);
    if current_field_kind.field_pic_flag() {
        frame_num.saturating_mul(2).saturating_add(1)
    } else {
        frame_num
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_max_pic_num(
    max_frame_num: u32,
    current_field_kind: NativeVulkanH264PictureFieldKind,
) -> i64 {
    let max_frame_num = i64::from(max_frame_num.max(1));
    if current_field_kind.field_pic_flag() {
        max_frame_num.saturating_mul(2)
    } else {
        max_frame_num
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_long_term_pic_num_for_key(
    key: NativeVulkanH264LongTermPictureKey,
    current_field_kind: NativeVulkanH264PictureFieldKind,
) -> i32 {
    if current_field_kind.field_pic_flag() {
        i32::from(key.frame_idx).saturating_mul(2).saturating_add(
            if key.field_kind == current_field_kind {
                1
            } else {
                0
            },
        )
    } else {
        i32::from(key.frame_idx)
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_long_term_key_from_pic_num(
    long_term_pic_num: u32,
    current_field_kind: NativeVulkanH264PictureFieldKind,
) -> Result<NativeVulkanH264LongTermPictureKey, String> {
    let (frame_idx, field_kind) = if current_field_kind.field_pic_flag() {
        (
            long_term_pic_num / 2,
            if long_term_pic_num % 2 == 1 {
                current_field_kind
            } else {
                current_field_kind.opposite_field()
            },
        )
    } else {
        (long_term_pic_num, NativeVulkanH264PictureFieldKind::Frame)
    };
    let frame_idx = u16::try_from(frame_idx).map_err(|_| {
        format!("H.264 long_term_pic_num {long_term_pic_num} exceeds supported u16 frame index")
    })?;
    Ok(NativeVulkanH264LongTermPictureKey {
        frame_idx,
        field_kind,
    })
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_short_term_pic_num_for_key(
    key: NativeVulkanH264ShortTermPictureKey,
    current_frame_num: u16,
    current_field_kind: NativeVulkanH264PictureFieldKind,
    max_frame_num: u32,
) -> i32 {
    let frame_pic_num =
        native_vulkan_h264_short_term_pic_num(key.frame_num, current_frame_num, max_frame_num);
    if current_field_kind.field_pic_flag() {
        frame_pic_num
            .saturating_mul(2)
            .saturating_add(if key.field_kind == current_field_kind {
                1
            } else {
                0
            })
    } else {
        frame_pic_num
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_picture_order_cnt_val(
    field_pic_flag: bool,
    bottom_field_flag: bool,
    pic_order_cnt: [i32; 2],
) -> i32 {
    if field_pic_flag && bottom_field_flag {
        pic_order_cnt[1]
    } else {
        pic_order_cnt[0]
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_find_short_term_reference_by_pic_num(
    short_term_references: &BTreeMap<
        NativeVulkanH264ShortTermPictureKey,
        NativeVulkanH264DpbReferenceState,
    >,
    current_frame_num: u16,
    current_field_kind: NativeVulkanH264PictureFieldKind,
    max_frame_num: u32,
    pic_num: i32,
) -> Option<(
    NativeVulkanH264ShortTermPictureKey,
    &NativeVulkanH264DpbReferenceState,
)> {
    short_term_references
        .iter()
        .find(|(key, _)| {
            native_vulkan_h264_short_term_pic_num_for_key(
                **key,
                current_frame_num,
                current_field_kind,
                max_frame_num,
            ) == pic_num
        })
        .map(|(key, reference)| (*key, reference))
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_first_short_term_key_for_frame_num(
    short_term_references: &BTreeMap<
        NativeVulkanH264ShortTermPictureKey,
        NativeVulkanH264DpbReferenceState,
    >,
    frame_num: u16,
) -> Option<NativeVulkanH264ShortTermPictureKey> {
    short_term_references
        .keys()
        .find(|key| key.frame_num == frame_num)
        .copied()
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_ref_pic_list_modifications_supported(
    slice: &NativeVulkanH264AccessUnitSliceSnapshot,
) -> bool {
    native_vulkan_h264_ref_pic_list_modification_items_supported(
        &slice.ref_pic_list_modifications_l0,
    ) && native_vulkan_h264_ref_pic_list_modification_items_supported(
        &slice.ref_pic_list_modifications_l1,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_ref_pic_list_modification_items_supported(
    modifications: &[NativeVulkanH264RefPicListModificationSnapshot],
) -> bool {
    modifications
        .iter()
        .all(|modification| matches!(modification.modification_of_pic_nums_idc, 0 | 1 | 2))
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_apply_ref_pic_list_modifications(
    entries: &mut NativeVulkanH264ReferenceListEntries,
    modifications: &[NativeVulkanH264RefPicListModificationSnapshot],
    current_frame_num: u16,
    current_field_kind: NativeVulkanH264PictureFieldKind,
    short_term_references: &BTreeMap<
        NativeVulkanH264ShortTermPictureKey,
        NativeVulkanH264DpbReferenceState,
    >,
    long_term_references: &BTreeMap<
        NativeVulkanH264LongTermPictureKey,
        NativeVulkanH264DpbReferenceState,
    >,
    planned_output_slot: u32,
    max_frame_num: u32,
    list_label: &'static str,
) -> Result<(), String> {
    if modifications.is_empty() {
        return Ok(());
    }

    let max_frame_num_u32 = max_frame_num.max(1);
    let max_pic_num = native_vulkan_h264_max_pic_num(max_frame_num_u32, current_field_kind);
    let mut pic_num_lx_pred =
        native_vulkan_h264_current_pic_num(current_frame_num, current_field_kind);
    let mut insertion_index = 0usize;
    for modification in modifications {
        let entry = match modification.modification_of_pic_nums_idc {
            0 | 1 => {
                let diff = modification
                    .abs_diff_pic_num_minus1
                    .ok_or_else(|| {
                        "H.264 short-term ref list modification is missing abs_diff_pic_num_minus1"
                            .to_owned()
                    })?
                    .saturating_add(1);
                let diff = i64::from(diff);
                let pic_num_lx_no_wrap = if modification.modification_of_pic_nums_idc == 0 {
                    let candidate = pic_num_lx_pred - diff;
                    if candidate < 0 {
                        candidate + max_pic_num
                    } else {
                        candidate
                    }
                } else {
                    let value = pic_num_lx_pred + diff;
                    if value >= max_pic_num {
                        value - max_pic_num
                    } else {
                        value
                    }
                };
                pic_num_lx_pred = pic_num_lx_no_wrap;
                let pic_num_lx = if pic_num_lx_no_wrap
                    > native_vulkan_h264_current_pic_num(current_frame_num, current_field_kind)
                {
                    pic_num_lx_no_wrap.saturating_sub(max_pic_num)
                } else {
                    pic_num_lx_no_wrap
                };
                let pic_num_lx_i32 = i32::try_from(pic_num_lx).map_err(|_| {
                    format!("H.264 modified reference PicNum {pic_num_lx} exceeds i32 range")
                })?;
                let Some((key, reference)) =
                    native_vulkan_h264_find_short_term_reference_by_pic_num(
                        short_term_references,
                        current_frame_num,
                        current_field_kind,
                        max_frame_num_u32,
                        pic_num_lx_i32,
                    )
                else {
                    return Err(format!(
                        "H.264 {list_label} ref list modification requested unavailable short-term PicNum {pic_num_lx}"
                    ));
                };
                if reference.dpb_slot == planned_output_slot {
                    return Err(format!(
                        "H.264 {list_label} ref list modification requested frame_num {} in the output DPB slot",
                        key.frame_num
                    ));
                }
                NativeVulkanH264ReferenceListEntry::ShortTerm(key)
            }
            2 => {
                let long_term_pic_num = modification.long_term_pic_num.ok_or_else(|| {
                    "H.264 long-term ref list modification is missing long_term_pic_num".to_owned()
                })?;
                let long_term_key = native_vulkan_h264_long_term_key_from_pic_num(
                    long_term_pic_num,
                    current_field_kind,
                )?;
                let Some(reference) = long_term_references.get(&long_term_key) else {
                    return Err(format!(
                        "H.264 {list_label} ref list modification requested unavailable long-term pic num {long_term_pic_num}"
                    ));
                };
                if reference.dpb_slot == planned_output_slot {
                    return Err(format!(
                        "H.264 {list_label} ref list modification requested long-term pic num {long_term_pic_num} in the output DPB slot"
                    ));
                }
                NativeVulkanH264ReferenceListEntry::LongTerm(long_term_key)
            }
            other => {
                return Err(format!(
                    "H.264 {list_label} ref_pic_list_modification idc {other} is not supported"
                ));
            }
        };
        entries.retain(|existing| *existing != entry);
        entries.insert(insertion_index.min(entries.len()), entry)?;
        insertion_index = insertion_index.saturating_add(1);
    }

    Ok(())
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_reference_frame_nums_for_slice(
    slice: &NativeVulkanH264AccessUnitSliceSnapshot,
    short_term_references: &BTreeMap<
        NativeVulkanH264ShortTermPictureKey,
        NativeVulkanH264DpbReferenceState,
    >,
    long_term_references: &BTreeMap<
        NativeVulkanH264LongTermPictureKey,
        NativeVulkanH264DpbReferenceState,
    >,
    planned_output_slot: u32,
    max_frame_num: u32,
) -> Result<NativeVulkanH264ReferenceListEntries, String> {
    let mut short_term_entries = [(NATIVE_VULKAN_H264_EMPTY_SHORT_TERM_KEY, 0i32);
        NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES];
    let mut short_term_entry_count = 0usize;
    for key in short_term_references.keys().copied() {
        if short_term_entry_count == short_term_entries.len() {
            return Err(format!(
                "H.264 short-term reference list exceeds FFmpeg fixed capacity {NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES}"
            ));
        }
        short_term_entries[short_term_entry_count] = (
            key,
            native_vulkan_h264_short_term_pic_num_for_key(
                key,
                slice.frame_num,
                NativeVulkanH264PictureFieldKind::from_slice(slice),
                max_frame_num,
            ),
        );
        short_term_entry_count += 1;
    }
    short_term_entries[..short_term_entry_count]
        .sort_by(|left, right| right.1.cmp(&left.1).then_with(|| right.0.cmp(&left.0)));
    let mut entries = NativeVulkanH264ReferenceListEntries::new();
    for (key, _) in &short_term_entries[..short_term_entry_count] {
        entries.push(NativeVulkanH264ReferenceListEntry::ShortTerm(*key))?;
    }
    let current_field_kind = NativeVulkanH264PictureFieldKind::from_slice(slice);
    let mut long_term_entries = [(NATIVE_VULKAN_H264_EMPTY_LONG_TERM_KEY, 0i32);
        NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES];
    let mut long_term_entry_count = 0usize;
    for key in long_term_references.keys().copied() {
        if long_term_entry_count == long_term_entries.len() {
            return Err(format!(
                "H.264 long-term reference list exceeds FFmpeg fixed capacity {NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES}"
            ));
        }
        long_term_entries[long_term_entry_count] = (
            key,
            native_vulkan_h264_long_term_pic_num_for_key(key, current_field_kind),
        );
        long_term_entry_count += 1;
    }
    long_term_entries[..long_term_entry_count]
        .sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
    for (key, _) in &long_term_entries[..long_term_entry_count] {
        entries.push(NativeVulkanH264ReferenceListEntry::LongTerm(*key))?;
    }

    native_vulkan_h264_apply_ref_pic_list_modifications(
        &mut entries,
        &slice.ref_pic_list_modifications_l0,
        slice.frame_num,
        NativeVulkanH264PictureFieldKind::from_slice(slice),
        short_term_references,
        long_term_references,
        planned_output_slot,
        max_frame_num,
        "L0",
    )?;
    Ok(entries)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_b_reference_frame_nums_for_slice(
    slice: &NativeVulkanH264AccessUnitSliceSnapshot,
    short_term_references: &BTreeMap<
        NativeVulkanH264ShortTermPictureKey,
        NativeVulkanH264DpbReferenceState,
    >,
    long_term_references: &BTreeMap<
        NativeVulkanH264LongTermPictureKey,
        NativeVulkanH264DpbReferenceState,
    >,
    planned_output_slot: u32,
    max_frame_num: u32,
) -> Result<NativeVulkanH264ReferenceListEntries, String> {
    let current_poc = native_vulkan_h264_picture_order_cnt_val(
        slice.field_pic_flag,
        slice.bottom_field_flag,
        slice.pic_order_cnt,
    );
    let l0_count = slice
        .num_ref_idx_l0_active_minus1
        .map(|value| value.saturating_add(1))
        .unwrap_or(0) as usize;
    let l1_count = slice
        .num_ref_idx_l1_active_minus1
        .map(|value| value.saturating_add(1))
        .unwrap_or(0) as usize;
    let mut before = [(NATIVE_VULKAN_H264_EMPTY_SHORT_TERM_KEY, 0i32);
        NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES];
    let mut before_count = 0usize;
    let mut after = [(NATIVE_VULKAN_H264_EMPTY_SHORT_TERM_KEY, 0i32);
        NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES];
    let mut after_count = 0usize;
    for (key, reference) in short_term_references {
        if reference.pic_order_cnt_val < current_poc {
            if before_count == before.len() {
                return Err(format!(
                    "H.264 B-slice before reference list exceeds FFmpeg fixed capacity {NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES}"
                ));
            }
            before[before_count] = (*key, reference.pic_order_cnt_val);
            before_count += 1;
        } else if reference.pic_order_cnt_val > current_poc {
            if after_count == after.len() {
                return Err(format!(
                    "H.264 B-slice after reference list exceeds FFmpeg fixed capacity {NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES}"
                ));
            }
            after[after_count] = (*key, reference.pic_order_cnt_val);
            after_count += 1;
        }
    }
    before[..before_count]
        .sort_by(|left, right| right.1.cmp(&left.1).then_with(|| right.0.cmp(&left.0)));
    after[..after_count]
        .sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
    let current_field_kind = NativeVulkanH264PictureFieldKind::from_slice(slice);
    let mut long_term_entries = [(NATIVE_VULKAN_H264_EMPTY_LONG_TERM_KEY, 0i32);
        NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES];
    let mut long_term_entry_count = 0usize;
    for key in long_term_references.keys().copied() {
        if long_term_entry_count == long_term_entries.len() {
            return Err(format!(
                "H.264 B-slice long-term reference list exceeds FFmpeg fixed capacity {NATIVE_VULKAN_H264_MAX_REFERENCE_LIST_ENTRIES}"
            ));
        }
        long_term_entries[long_term_entry_count] = (
            key,
            native_vulkan_h264_long_term_pic_num_for_key(key, current_field_kind),
        );
        long_term_entry_count += 1;
    }
    long_term_entries[..long_term_entry_count]
        .sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));

    let mut l0 = NativeVulkanH264ReferenceListEntries::new();
    for (key, _) in before[..before_count]
        .iter()
        .chain(after[..after_count].iter())
    {
        l0.push(NativeVulkanH264ReferenceListEntry::ShortTerm(*key))?;
    }
    for (key, _) in &long_term_entries[..long_term_entry_count] {
        l0.push(NativeVulkanH264ReferenceListEntry::LongTerm(*key))?;
    }
    let mut l1 = NativeVulkanH264ReferenceListEntries::new();
    for (key, _) in after[..after_count]
        .iter()
        .chain(before[..before_count].iter())
    {
        l1.push(NativeVulkanH264ReferenceListEntry::ShortTerm(*key))?;
    }
    for (key, _) in &long_term_entries[..long_term_entry_count] {
        l1.push(NativeVulkanH264ReferenceListEntry::LongTerm(*key))?;
    }
    if l0.len() > 1 && l1.len() > 1 && l0.as_slice() == l1.as_slice() {
        l1.as_mut_slice().swap(0, 1);
    }
    native_vulkan_h264_apply_ref_pic_list_modifications(
        &mut l0,
        &slice.ref_pic_list_modifications_l0,
        slice.frame_num,
        NativeVulkanH264PictureFieldKind::from_slice(slice),
        short_term_references,
        long_term_references,
        planned_output_slot,
        max_frame_num,
        "L0",
    )?;
    native_vulkan_h264_apply_ref_pic_list_modifications(
        &mut l1,
        &slice.ref_pic_list_modifications_l1,
        slice.frame_num,
        NativeVulkanH264PictureFieldKind::from_slice(slice),
        short_term_references,
        long_term_references,
        planned_output_slot,
        max_frame_num,
        "L1",
    )?;
    l0.truncate(l0_count);
    l1.truncate(l1_count);
    l0.extend_from_entries(&l1)?;
    let mut unique = NativeVulkanH264ReferenceListEntries::new();
    for entry in l0.iter().copied() {
        if !unique.contains(&entry) {
            unique.push(entry)?;
        }
    }
    Ok(unique)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_adaptive_marking_plan_for_slice(
    slice: &NativeVulkanH264AccessUnitSliceSnapshot,
    short_term_references: &BTreeMap<
        NativeVulkanH264ShortTermPictureKey,
        NativeVulkanH264DpbReferenceState,
    >,
    long_term_references: &BTreeMap<
        NativeVulkanH264LongTermPictureKey,
        NativeVulkanH264DpbReferenceState,
    >,
    max_frame_num: u32,
) -> Result<NativeVulkanH264AdaptiveMarkingPlan, String> {
    let mut plan = NativeVulkanH264AdaptiveMarkingPlan::default();
    let current_field_kind = NativeVulkanH264PictureFieldKind::from_slice(slice);
    if !slice.adaptive_ref_pic_marking_mode_flag {
        if slice.idr && slice.long_term_reference_flag {
            plan.current_long_term_frame_idx = Some(0);
        }
        return Ok(plan);
    }

    let max_frame_num = max_frame_num.max(1);
    for operation in &slice.memory_management_control_operations {
        match operation.memory_management_control_operation {
            1 => {
                let pic_num = native_vulkan_h264_mmco_short_term_pic_num(
                    slice.frame_num,
                    current_field_kind,
                    operation.difference_of_pic_nums_minus1,
                    max_frame_num,
                    "MMCO 1",
                )?;
                let Some((key, _)) = native_vulkan_h264_find_short_term_reference_by_pic_num(
                    short_term_references,
                    slice.frame_num,
                    current_field_kind,
                    max_frame_num,
                    pic_num,
                ) else {
                    return Err(format!(
                        "H.264 MMCO 1 requested unavailable short-term PicNum {pic_num}"
                    ));
                };
                if !plan.drop_short_term_keys.contains(&key) {
                    plan.drop_short_term_keys.push(key);
                }
            }
            2 => {
                let long_term_pic_num = operation
                    .long_term_pic_num
                    .ok_or_else(|| "H.264 MMCO 2 is missing long_term_pic_num".to_owned())?;
                let long_term_key = native_vulkan_h264_long_term_key_from_pic_num(
                    long_term_pic_num,
                    current_field_kind,
                )?;
                if !long_term_references.contains_key(&long_term_key) {
                    return Err(format!(
                        "H.264 MMCO 2 requested unavailable long-term pic num {long_term_pic_num}"
                    ));
                }
                if !plan.drop_long_term_keys.contains(&long_term_key) {
                    plan.drop_long_term_keys.push(long_term_key);
                }
            }
            3 => {
                let pic_num = native_vulkan_h264_mmco_short_term_pic_num(
                    slice.frame_num,
                    current_field_kind,
                    operation.difference_of_pic_nums_minus1,
                    max_frame_num,
                    "MMCO 3",
                )?;
                let Some((key, _)) = native_vulkan_h264_find_short_term_reference_by_pic_num(
                    short_term_references,
                    slice.frame_num,
                    current_field_kind,
                    max_frame_num,
                    pic_num,
                ) else {
                    return Err(format!(
                        "H.264 MMCO 3 requested unavailable short-term PicNum {pic_num}"
                    ));
                };
                let long_term_frame_idx = native_vulkan_h264_optional_u16(
                    operation.long_term_frame_idx,
                    "long_term_frame_idx",
                )?;
                plan.convert_short_term_to_long_term
                    .push((key, long_term_frame_idx));
            }
            4 => {
                let max_plus1 = operation.max_long_term_frame_idx_plus1.ok_or_else(|| {
                    "H.264 MMCO 4 is missing max_long_term_frame_idx_plus1".to_owned()
                })?;
                if max_plus1 == 0 {
                    for long_term_key in long_term_references.keys().copied() {
                        if !plan.drop_long_term_keys.contains(&long_term_key) {
                            plan.drop_long_term_keys.push(long_term_key);
                        }
                    }
                } else {
                    let max_idx = u16::try_from(max_plus1.saturating_sub(1)).map_err(|_| {
                        format!(
                            "H.264 MMCO 4 max_long_term_frame_idx_plus1 {max_plus1} exceeds supported u16 range"
                        )
                    })?;
                    for long_term_key in long_term_references.keys().copied() {
                        if long_term_key.frame_idx > max_idx
                            && !plan.drop_long_term_keys.contains(&long_term_key)
                        {
                            plan.drop_long_term_keys.push(long_term_key);
                        }
                    }
                }
            }
            5 => {
                for key in short_term_references.keys().copied() {
                    if !plan.drop_short_term_keys.contains(&key) {
                        plan.drop_short_term_keys.push(key);
                    }
                }
                for long_term_key in long_term_references.keys().copied() {
                    if !plan.drop_long_term_keys.contains(&long_term_key) {
                        plan.drop_long_term_keys.push(long_term_key);
                    }
                }
            }
            6 => {
                plan.current_long_term_frame_idx = Some(native_vulkan_h264_optional_u16(
                    operation.long_term_frame_idx,
                    "long_term_frame_idx",
                )?);
            }
            other => {
                return Err(format!(
                    "H.264 MMCO {other} is not supported by the first continuous direct gate"
                ));
            }
        }
    }

    Ok(plan)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_mmco_short_term_pic_num(
    current_frame_num: u16,
    current_field_kind: NativeVulkanH264PictureFieldKind,
    difference_of_pic_nums_minus1: Option<u32>,
    _max_frame_num: u32,
    label: &'static str,
) -> Result<i32, String> {
    let difference = difference_of_pic_nums_minus1
        .ok_or_else(|| format!("H.264 {label} is missing difference_of_pic_nums_minus1"))?
        .saturating_add(1);
    let current = native_vulkan_h264_current_pic_num(current_frame_num, current_field_kind);
    let difference = i64::from(difference);
    let pic_num = current.saturating_sub(difference);
    i32::try_from(pic_num)
        .map_err(|_| format!("H.264 {label} target PicNum {pic_num} exceeds i32 range"))
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_optional_u16(
    value: Option<u32>,
    label: &'static str,
) -> Result<u16, String> {
    let value = value.ok_or_else(|| format!("H.264 value {label} is missing"))?;
    u16::try_from(value).map_err(|_| format!("H.264 {label} {value} exceeds supported u16 range"))
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAv1ReferenceMapEntry {
    pub(in crate::renderer::native_vulkan) slot: u32,
    pub(in crate::renderer::native_vulkan) order_hint: Option<u8>,
    pub(in crate::renderer::native_vulkan) frame_type: u8,
    pub(in crate::renderer::native_vulkan) frame_width: Option<u32>,
    pub(in crate::renderer::native_vulkan) frame_height: Option<u32>,
    pub(in crate::renderer::native_vulkan) render_width: Option<u32>,
    pub(in crate::renderer::native_vulkan) render_height: Option<u32>,
}

#[cfg(any(feature = "native-vulkan-video", test))]
#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAv1DecodeReferencePlanner {
    pub(in crate::renderer::native_vulkan) dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) next_output_slot: u32,
    pub(in crate::renderer::native_vulkan) reference_map:
        [Option<NativeVulkanAv1ReferenceMapEntry>; 8],
}

#[cfg(any(feature = "native-vulkan-video", test))]
impl NativeVulkanAv1DecodeReferencePlanner {
    pub(in crate::renderer::native_vulkan) fn new(dpb_slots: u32) -> Self {
        Self {
            dpb_slots: dpb_slots.max(1),
            next_output_slot: 0,
            reference_map: [None; 8],
        }
    }

    pub(in crate::renderer::native_vulkan) fn map_slot_indices(&self) -> Vec<i32> {
        self.reference_map
            .iter()
            .map(|entry| entry.map(|entry| entry.slot as i32).unwrap_or(-1))
            .collect()
    }

    pub(in crate::renderer::native_vulkan) fn map_order_hints(&self) -> Vec<Option<u8>> {
        self.reference_map
            .iter()
            .map(|entry| entry.and_then(|entry| entry.order_hint))
            .collect()
    }

    pub(in crate::renderer::native_vulkan) fn reference_name_order_hints(
        &self,
        ref_frame_indices: &[i8],
    ) -> Vec<Option<u8>> {
        let mut order_hints = vec![None; 8];
        for (reference_name_minus_one, ref_idx) in ref_frame_indices.iter().take(7).enumerate() {
            if !(0..=7).contains(ref_idx) {
                continue;
            }
            order_hints[reference_name_minus_one + 1] =
                self.reference_map[*ref_idx as usize].and_then(|entry| entry.order_hint);
        }
        order_hints
    }

    pub(in crate::renderer::native_vulkan) fn allocate_output_slot(
        &mut self,
        protected_slots: &[u32],
    ) -> Option<u32> {
        for offset in 0..self.dpb_slots {
            let slot = (self.next_output_slot + offset) % self.dpb_slots;
            if protected_slots.contains(&slot) {
                continue;
            }
            if !self
                .reference_map
                .iter()
                .flatten()
                .any(|entry| entry.slot == slot)
            {
                self.next_output_slot = (slot + 1) % self.dpb_slots;
                return Some(slot);
            }
        }
        for offset in 0..self.dpb_slots {
            let slot = (self.next_output_slot + offset) % self.dpb_slots;
            if !protected_slots.contains(&slot) {
                self.next_output_slot = (slot + 1) % self.dpb_slots;
                return Some(slot);
            }
        }
        None
    }

    pub(in crate::renderer::native_vulkan) fn plan_next(
        &mut self,
        temporal_unit: &NativeVulkanAv1TemporalUnitSnapshot,
    ) -> NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
        let reference_name_slot_indices = self.map_slot_indices();
        let map_order_hints = self.map_order_hints();
        let Some(submit) = temporal_unit.first_frame_submit.as_ref() else {
            return NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
                temporal_unit_index: temporal_unit.index,
                frame_type_label: "none",
                show_existing_frame: false,
                frame_to_show_map_idx: None,
                show_frame: false,
                order_hint: None,
                current_frame_id: None,
                expected_frame_ids: Vec::new(),
                refresh_frame_flags: 0,
                output_slot: None,
                displayed_slot: None,
                reference_name_slot_indices,
                reference_name_order_hints: vec![None; 8],
                map_order_hints,
                ref_frame_indices: Vec::new(),
                decode_reference_slots: Vec::new(),
                refreshed_reference_names: Vec::new(),
                missing_reference_names: Vec::new(),
                missing_reference_count: 0,
                references_resolved: false,
                submit_fields_ready: false,
                ready_for_decode_submit: false,
                ready_for_display_handoff: false,
                unsupported_reason: Some("AV1 temporal unit has no parsed frame header".to_owned()),
                map_slot_indices_after: self.map_slot_indices(),
                map_order_hints_after: self.map_order_hints(),
            };
        };
        let reference_name_order_hints = self.reference_name_order_hints(&submit.ref_frame_indices);

        if submit.show_existing_frame {
            let map_idx = submit.frame_to_show_map_idx;
            let displayed_entry = map_idx
                .and_then(|index| self.reference_map.get(index as usize))
                .and_then(|entry| *entry);
            let displayed_slot = displayed_entry.map(|entry| entry.slot);
            let missing_reference_names = if displayed_slot.is_some() {
                Vec::new()
            } else {
                map_idx.into_iter().collect()
            };
            let ready_for_display_handoff = displayed_slot.is_some();
            let inferred_frame_type = displayed_entry
                .map(|entry| entry.frame_type)
                .unwrap_or(submit.frame_type);
            let refresh_frame_flags = if ready_for_display_handoff && inferred_frame_type == 0 {
                0xff
            } else {
                submit.refresh_frame_flags
            };
            let refreshed_reference_names = (0..8)
                .filter(|index| (refresh_frame_flags & (1u8 << index)) != 0)
                .map(|index| index as u8)
                .collect::<Vec<_>>();
            if ready_for_display_handoff && let Some(displayed_entry) = displayed_entry {
                for index in &refreshed_reference_names {
                    self.reference_map[*index as usize] = Some(displayed_entry);
                }
            }
            return NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
                temporal_unit_index: temporal_unit.index,
                frame_type_label: native_vulkan_av1_frame_type_label(inferred_frame_type),
                show_existing_frame: true,
                frame_to_show_map_idx: submit.frame_to_show_map_idx,
                show_frame: submit.show_frame,
                order_hint: submit.order_hint,
                current_frame_id: submit.current_frame_id,
                expected_frame_ids: submit.expected_frame_ids.clone(),
                refresh_frame_flags,
                output_slot: None,
                displayed_slot,
                reference_name_slot_indices,
                reference_name_order_hints,
                map_order_hints,
                ref_frame_indices: submit.ref_frame_indices.clone(),
                decode_reference_slots: Vec::new(),
                refreshed_reference_names,
                missing_reference_count: missing_reference_names.len() as u32,
                missing_reference_names,
                references_resolved: ready_for_display_handoff,
                submit_fields_ready: false,
                ready_for_decode_submit: false,
                ready_for_display_handoff,
                unsupported_reason: if ready_for_display_handoff {
                    None
                } else {
                    Some("AV1 show_existing_frame references an unavailable map index".to_owned())
                },
                map_slot_indices_after: self.map_slot_indices(),
                map_order_hints_after: self.map_order_hints(),
            };
        }

        let mut decode_reference_slots = Vec::with_capacity(submit.ref_frame_indices.len());
        let mut missing_reference_names = Vec::new();
        for ref_idx in &submit.ref_frame_indices {
            if *ref_idx < 0 || *ref_idx > 7 {
                missing_reference_names.push(0xff);
                decode_reference_slots.push(-1);
                continue;
            }
            let map_idx = *ref_idx as usize;
            match self.reference_map[map_idx] {
                Some(entry) => decode_reference_slots.push(entry.slot as i32),
                None => {
                    missing_reference_names.push(*ref_idx as u8);
                    decode_reference_slots.push(-1);
                }
            }
        }
        let references_resolved = missing_reference_names.is_empty();
        let refreshed_reference_names = (0..8)
            .filter(|index| (submit.refresh_frame_flags & (1u8 << index)) != 0)
            .map(|index| index as u8)
            .collect::<Vec<_>>();
        let mut protected_slots = decode_reference_slots
            .iter()
            .filter_map(|slot| u32::try_from(*slot).ok())
            .collect::<Vec<_>>();
        for (index, entry) in self.reference_map.iter().enumerate() {
            if refreshed_reference_names.contains(&(index as u8)) {
                continue;
            }
            if let Some(entry) = entry
                && !protected_slots.contains(&entry.slot)
            {
                protected_slots.push(entry.slot);
            }
        }
        let output_slot = self.allocate_output_slot(&protected_slots);
        if let Some(output_slot_value) = output_slot {
            for index in &refreshed_reference_names {
                self.reference_map[*index as usize] = Some(NativeVulkanAv1ReferenceMapEntry {
                    slot: output_slot_value,
                    order_hint: submit.order_hint,
                    frame_type: submit.frame_type,
                    frame_width: submit.frame_width,
                    frame_height: submit.frame_height,
                    render_width: submit.render_width,
                    render_height: submit.render_height,
                });
            }
        }

        let submit_fields_ready = submit.vulkan_submit_candidate;
        let output_slot_available = output_slot.is_some();
        let ready_for_decode_submit =
            references_resolved && submit_fields_ready && output_slot_available;
        let unsupported_reason = if !references_resolved {
            Some(format!(
                "AV1 reference map is missing reference name(s) {:?}",
                missing_reference_names
            ))
        } else if !output_slot_available {
            Some(format!(
                "AV1 reference map has no free DPB output slot with {} slot(s); protected slots {:?}",
                self.dpb_slots, protected_slots
            ))
        } else if !submit_fields_ready {
            submit.unsupported_reason.clone().or_else(|| {
                Some("AV1 frame header is reference-ready but not submit-ready".to_owned())
            })
        } else {
            None
        };

        NativeVulkanAv1DecodeReferencePlanEntrySnapshot {
            temporal_unit_index: temporal_unit.index,
            frame_type_label: submit.frame_type_label,
            show_existing_frame: false,
            frame_to_show_map_idx: submit.frame_to_show_map_idx,
            show_frame: submit.show_frame,
            order_hint: submit.order_hint,
            current_frame_id: submit.current_frame_id,
            expected_frame_ids: submit.expected_frame_ids.clone(),
            refresh_frame_flags: submit.refresh_frame_flags,
            output_slot,
            displayed_slot: if submit.show_frame { output_slot } else { None },
            reference_name_slot_indices,
            reference_name_order_hints,
            map_order_hints,
            ref_frame_indices: submit.ref_frame_indices.clone(),
            decode_reference_slots,
            refreshed_reference_names,
            missing_reference_count: missing_reference_names.len() as u32,
            missing_reference_names,
            references_resolved,
            submit_fields_ready,
            ready_for_decode_submit,
            ready_for_display_handoff: false,
            unsupported_reason,
            map_slot_indices_after: self.map_slot_indices(),
            map_order_hints_after: self.map_order_hints(),
        }
    }
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(in crate::renderer::native_vulkan) fn native_vulkan_av1_decode_reference_plan(
    temporal_units: &[NativeVulkanAv1TemporalUnitSnapshot],
    dpb_slots: u32,
) -> Vec<NativeVulkanAv1DecodeReferencePlanEntrySnapshot> {
    let mut planner = NativeVulkanAv1DecodeReferencePlanner::new(dpb_slots);
    temporal_units
        .iter()
        .map(|temporal_unit| planner.plan_next(temporal_unit))
        .collect()
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(in crate::renderer::native_vulkan) fn native_vulkan_av1_min_decodable_dpb_plan(
    temporal_units: &[NativeVulkanAv1TemporalUnitSnapshot],
    max_dpb_slots: u32,
) -> (u32, Vec<NativeVulkanAv1DecodeReferencePlanEntrySnapshot>) {
    let max_dpb_slots = max_dpb_slots.max(1);
    let mut last_plan = Vec::new();
    for dpb_slots in 1..=max_dpb_slots {
        let plan = native_vulkan_av1_decode_reference_plan(temporal_units, dpb_slots);
        if plan
            .iter()
            .all(|entry| entry.ready_for_decode_submit || entry.ready_for_display_handoff)
        {
            return (dpb_slots, plan);
        }
        last_plan = plan;
    }
    (max_dpb_slots, last_plan)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_av1_temporal_units_max_active_references(
    temporal_units: &[NativeVulkanAv1TemporalUnitSnapshot],
) -> u32 {
    temporal_units
        .iter()
        .filter_map(|temporal_unit| temporal_unit.first_frame_submit.as_ref())
        .map(|submit| {
            submit
                .ref_frame_indices
                .iter()
                .filter(|index| **index >= 0)
                .count()
                .min(u32::MAX as usize) as u32
        })
        .max()
        .unwrap_or(0)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_av1_temporal_unit_starts_recovery(
    temporal_unit: &NativeVulkanAv1TemporalUnitSnapshot,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> bool {
    temporal_unit
        .first_frame_submit
        .as_ref()
        .is_some_and(|submit| {
            submit.frame_type == 0
                && submit.show_frame
                && submit.vulkan_submit_candidate
                && temporal_unit
                    .sequence_header
                    .as_ref()
                    .unwrap_or(sequence_header)
                    .vulkan_std_session_parameters_ready
        })
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAv1StreamingBootstrap {
    pub(in crate::renderer::native_vulkan) stream_dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) stream_max_active_reference_pictures: u32,
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_av1_align_streaming_bootstrap(
    queue: &mut NativeVulkanAv1StreamingPacketQueue,
    sequence_header: &NativeVulkanAv1SequenceHeaderSnapshot,
) -> Result<NativeVulkanAv1StreamingBootstrap, NativeVulkanError> {
    let scan_limit = native_vulkan_streaming_bootstrap_scan_limit(queue.capacity);
    let mut skipped_temporal_unit_indices = Vec::<u32>::new();
    loop {
        let bootstrap_temporal_units = queue.bootstrap_access_units();
        if bootstrap_temporal_units.is_empty() {
            return Err(NativeVulkanError::Video(format!(
                "AV1 streaming bootstrap could not find a decodable TU window after skipping {} leading TU(s)",
                skipped_temporal_unit_indices.len()
            )));
        }
        // AV1 has eight named reference slots. Real streams can also contain
        // showable non-reference frames (`refresh_frame_flags == 0`), which need
        // a transient output slot when DPB and output coincide.
        let stream_max_dpb_slots = 16;
        let stream_max_active_reference_pictures =
            native_vulkan_av1_temporal_units_max_active_references(&bootstrap_temporal_units)
                .max(7)
                .max(1);
        let (stream_dpb_slots_for_window, bootstrap_plan) =
            native_vulkan_av1_min_decodable_dpb_plan(
                &bootstrap_temporal_units,
                stream_max_dpb_slots,
            );
        let stream_dpb_slots = stream_dpb_slots_for_window.max(9).min(stream_max_dpb_slots);
        let recovery_offset = bootstrap_temporal_units.iter().position(|temporal_unit| {
            native_vulkan_av1_temporal_unit_starts_recovery(temporal_unit, sequence_header)
        });
        let Some(first_unready_offset) = bootstrap_plan
            .iter()
            .position(|entry| !(entry.ready_for_decode_submit || entry.ready_for_display_handoff))
        else {
            if recovery_offset == Some(0) {
                queue.set_loop_skip_access_units(
                    queue.bootstrap_discarded_access_units.min(u32::MAX),
                );
                return Ok(NativeVulkanAv1StreamingBootstrap {
                    stream_dpb_slots,
                    stream_max_active_reference_pictures,
                });
            }
            let discard_count = recovery_offset.filter(|offset| *offset > 0).unwrap_or(1);
            if usize::try_from(queue.bootstrap_discarded_access_units)
                .unwrap_or(usize::MAX)
                .saturating_add(discard_count)
                > scan_limit
            {
                return Err(NativeVulkanError::Video(format!(
                    "AV1 streaming bootstrap exceeded scan limit {scan_limit} while looking for a recovery TU after skipping {} leading TU(s)",
                    queue.bootstrap_discarded_access_units
                )));
            }
            for _ in 0..discard_count {
                let Some(dropped) = queue.discard_front_for_bootstrap()? else {
                    return Err(NativeVulkanError::Video(format!(
                        "AV1 streaming bootstrap reached EOS after skipping {} leading TU(s) without finding a recovery TU",
                        queue.bootstrap_discarded_access_units
                    )));
                };
                skipped_temporal_unit_indices.push(dropped.access_unit_index);
            }
            continue;
        };
        let first_unready = &bootstrap_plan[first_unready_offset];
        let discard_count = recovery_offset
            .filter(|offset| *offset > 0)
            .unwrap_or(usize::from(first_unready_offset == 0));
        if discard_count == 0 {
            return Err(NativeVulkanError::Video(format!(
                "AV1 streaming bootstrap TU {} is not decodable with optimized DPB slot count {stream_dpb_slots} after skipping {} leading TU(s): {}",
                first_unready.temporal_unit_index,
                queue.bootstrap_discarded_access_units,
                first_unready
                    .unsupported_reason
                    .as_deref()
                    .unwrap_or("missing references")
            )));
        }
        if usize::try_from(queue.bootstrap_discarded_access_units)
            .unwrap_or(usize::MAX)
            .saturating_add(discard_count)
            > scan_limit
        {
            return Err(NativeVulkanError::Video(format!(
                "AV1 streaming bootstrap exceeded scan limit {scan_limit} while looking for a decodable TU window; last leading TU {} was not decodable: {}",
                first_unready.temporal_unit_index,
                first_unready
                    .unsupported_reason
                    .as_deref()
                    .unwrap_or("missing references")
            )));
        }
        for _ in 0..discard_count {
            let Some(dropped) = queue.discard_front_for_bootstrap()? else {
                return Err(NativeVulkanError::Video(format!(
                    "AV1 streaming bootstrap reached EOS after skipping {} leading TU(s) without finding a decodable window",
                    queue.bootstrap_discarded_access_units
                )));
            };
            skipped_temporal_unit_indices.push(dropped.access_unit_index);
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH264DecodeReferencePlanner {
    pub(in crate::renderer::native_vulkan) dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) max_short_term_references: u32,
    pub(in crate::renderer::native_vulkan) max_frame_num: u32,
    pub(in crate::renderer::native_vulkan) gaps_in_frame_num_allowed: bool,
    pub(in crate::renderer::native_vulkan) previous_reference_frame_num: Option<u16>,
    pub(in crate::renderer::native_vulkan) short_term_references:
        BTreeMap<NativeVulkanH264ShortTermPictureKey, NativeVulkanH264DpbReferenceState>,
    pub(in crate::renderer::native_vulkan) long_term_references:
        BTreeMap<NativeVulkanH264LongTermPictureKey, NativeVulkanH264DpbReferenceState>,
    pub(in crate::renderer::native_vulkan) slot_to_reference_key:
        Vec<Option<NativeVulkanH264DpbSlotKey>>,
    pub(in crate::renderer::native_vulkan) short_term_reference_order:
        Vec<NativeVulkanH264ShortTermPictureKey>,
    pub(in crate::renderer::native_vulkan) next_output_slot: u32,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanH264DecodeReferencePlanner {
    pub(in crate::renderer::native_vulkan) fn new(
        dpb_slots: u32,
        max_short_term_references: u32,
        max_frame_num: u32,
        gaps_in_frame_num_allowed: bool,
    ) -> Self {
        let dpb_slots = dpb_slots.max(1);
        Self {
            dpb_slots,
            max_short_term_references: max_short_term_references.max(1),
            max_frame_num: max_frame_num.max(1),
            gaps_in_frame_num_allowed,
            previous_reference_frame_num: None,
            short_term_references: BTreeMap::new(),
            long_term_references: BTreeMap::new(),
            slot_to_reference_key: vec![None; dpb_slots as usize],
            short_term_reference_order: Vec::new(),
            next_output_slot: 0,
        }
    }

    pub(in crate::renderer::native_vulkan) fn reset(&mut self) {
        self.previous_reference_frame_num = None;
        self.short_term_references.clear();
        self.long_term_references.clear();
        self.slot_to_reference_key.fill(None);
        self.short_term_reference_order.clear();
        self.next_output_slot = 0;
    }

    pub(in crate::renderer::native_vulkan) fn choose_output_slot(&mut self) -> u32 {
        for offset in 0..self.dpb_slots {
            let slot = (self.next_output_slot + offset) % self.dpb_slots;
            if self
                .slot_to_reference_key
                .get(slot as usize)
                .is_none_or(Option::is_none)
            {
                self.next_output_slot = (slot + 1) % self.dpb_slots;
                return slot;
            }
        }
        let slot = self.next_output_slot % self.dpb_slots;
        self.next_output_slot = (slot + 1) % self.dpb_slots;
        slot
    }

    pub(in crate::renderer::native_vulkan) fn active_reference_count(&self) -> usize {
        self.short_term_references
            .len()
            .saturating_add(self.long_term_references.len())
    }

    pub(in crate::renderer::native_vulkan) fn enforce_sliding_reference_window(
        &mut self,
    ) -> Vec<(u16, u32)> {
        let mut dropped = Vec::new();
        while self.active_reference_count() > self.max_short_term_references as usize {
            let Some(old_key) = self.short_term_reference_order.first().copied() else {
                break;
            };
            self.short_term_reference_order.remove(0);
            if let Some(old_slot) = self.remove_short_term_reference(old_key) {
                dropped.push((old_key.frame_num, old_slot));
            }
        }
        dropped
    }

    pub(in crate::renderer::native_vulkan) fn infer_non_existing_short_term_references(
        &mut self,
        current_frame_num: u16,
    ) -> Result<NativeVulkanH264InferredNonExistingPlan, String> {
        let Some(previous_frame_num) = self.previous_reference_frame_num else {
            return Ok(NativeVulkanH264InferredNonExistingPlan::default());
        };
        let max_frame_num = self.max_frame_num.max(1);
        let current_frame_num_u32 = u32::from(current_frame_num);
        if current_frame_num_u32 >= max_frame_num {
            return Err(format!(
                "H.264 current frame_num {current_frame_num} is outside max_frame_num {max_frame_num}"
            ));
        }
        let previous_frame_num_u32 = u32::from(previous_frame_num) % max_frame_num;
        if previous_frame_num_u32 == current_frame_num_u32 {
            return Ok(NativeVulkanH264InferredNonExistingPlan::default());
        }
        let mut frame_num = (previous_frame_num_u32 + 1) % max_frame_num;
        if frame_num == current_frame_num_u32 {
            return Ok(NativeVulkanH264InferredNonExistingPlan::default());
        }
        if !self.gaps_in_frame_num_allowed {
            return Err(format!(
                "H.264 frame_num gap from {previous_frame_num} to {current_frame_num} but SPS gaps_in_frame_num_value_allowed_flag is false"
            ));
        }

        let mut plan = NativeVulkanH264InferredNonExistingPlan::default();
        let mut guard = 0u32;
        while frame_num != current_frame_num_u32 && guard < max_frame_num {
            let inferred_frame_num = u16::try_from(frame_num).map_err(|_| {
                format!(
                    "H.264 inferred non-existing frame_num {frame_num} exceeds supported u16 range"
                )
            })?;
            let inferred_key = NativeVulkanH264ShortTermPictureKey::frame(inferred_frame_num);
            plan.frame_nums.push(inferred_frame_num);
            let slot = self.choose_output_slot();
            self.clear_slot_for_inference(slot, &mut plan);
            let reference_state = NativeVulkanH264DpbReferenceState {
                source_access_unit_index: None,
                dpb_slot: slot,
                pic_order_cnt_val: i32::from(inferred_frame_num),
                pic_order_cnt: [i32::from(inferred_frame_num); 2],
                frame_num: inferred_frame_num,
                field_kind: inferred_key.field_kind,
                non_existing: true,
            };
            self.short_term_references
                .insert(inferred_key, reference_state);
            if let Some(slot_ref) = self.slot_to_reference_key.get_mut(slot as usize) {
                *slot_ref = Some(NativeVulkanH264DpbSlotKey::ShortTerm(inferred_key));
            }
            self.short_term_reference_order
                .retain(|existing| *existing != inferred_key);
            self.short_term_reference_order.push(inferred_key);
            for (old_frame_num, old_slot) in self.enforce_sliding_reference_window() {
                if !plan.dropped_short_term_frame_nums.contains(&old_frame_num) {
                    plan.dropped_short_term_frame_nums.push(old_frame_num);
                }
                if !plan.dropped_reference_slots.contains(&old_slot) {
                    plan.dropped_reference_slots.push(old_slot);
                }
            }
            frame_num = (frame_num + 1) % max_frame_num;
            guard = guard.saturating_add(1);
        }
        if frame_num != current_frame_num_u32 {
            return Err(format!(
                "H.264 frame_num gap inference from {previous_frame_num} to {current_frame_num} did not converge within max_frame_num {}",
                max_frame_num
            ));
        }
        plan.references = plan
            .frame_nums
            .iter()
            .filter_map(|frame_num| {
                native_vulkan_h264_first_short_term_key_for_frame_num(
                    &self.short_term_references,
                    *frame_num,
                )
                .and_then(|key| {
                    self.short_term_references.get(&key).and_then(|reference| {
                        reference.non_existing.then_some(
                            NativeVulkanH264InferredNonExistingReferenceSnapshot {
                                frame_num: reference.frame_num,
                                field_pic_flag: reference.field_kind.field_pic_flag(),
                                bottom_field_flag: reference.field_kind.bottom_field_flag(),
                                pic_order_cnt_val: reference.pic_order_cnt_val,
                                pic_order_cnt: reference.pic_order_cnt,
                                dpb_slot: reference.dpb_slot,
                            },
                        )
                    })
                })
            })
            .collect();
        Ok(plan)
    }

    pub(in crate::renderer::native_vulkan) fn record_inference_dropped_key(
        &mut self,
        key: NativeVulkanH264DpbSlotKey,
        slot: u32,
        plan: &mut NativeVulkanH264InferredNonExistingPlan,
    ) {
        match key {
            NativeVulkanH264DpbSlotKey::ShortTerm(key) => {
                self.short_term_reference_order
                    .retain(|existing| *existing != key);
                self.short_term_references.remove(&key);
                if !plan.dropped_short_term_frame_nums.contains(&key.frame_num) {
                    plan.dropped_short_term_frame_nums.push(key.frame_num);
                }
            }
            NativeVulkanH264DpbSlotKey::LongTerm(long_term_key) => {
                self.long_term_references.remove(&long_term_key);
                if !plan
                    .dropped_long_term_frame_indices
                    .contains(&long_term_key.frame_idx)
                {
                    plan.dropped_long_term_frame_indices
                        .push(long_term_key.frame_idx);
                }
            }
        }
        if !plan.dropped_reference_slots.contains(&slot) {
            plan.dropped_reference_slots.push(slot);
        }
    }

    pub(in crate::renderer::native_vulkan) fn clear_slot_for_inference(
        &mut self,
        slot: u32,
        plan: &mut NativeVulkanH264InferredNonExistingPlan,
    ) {
        let key = self
            .slot_to_reference_key
            .get(slot as usize)
            .copied()
            .flatten();
        if let Some(key) = key {
            self.record_inference_dropped_key(key, slot, plan);
        }
        if let Some(slot_ref) = self.slot_to_reference_key.get_mut(slot as usize) {
            *slot_ref = None;
        }
    }

    pub(in crate::renderer::native_vulkan) fn remove_short_term_reference(
        &mut self,
        key: NativeVulkanH264ShortTermPictureKey,
    ) -> Option<u32> {
        self.short_term_reference_order
            .retain(|existing| *existing != key);
        let reference = self.short_term_references.remove(&key)?;
        let slot = reference.dpb_slot;
        if self
            .slot_to_reference_key
            .get(slot as usize)
            .copied()
            .flatten()
            == Some(NativeVulkanH264DpbSlotKey::ShortTerm(key))
            && let Some(slot_ref) = self.slot_to_reference_key.get_mut(slot as usize)
        {
            *slot_ref = None;
        }
        Some(slot)
    }

    pub(in crate::renderer::native_vulkan) fn remove_long_term_reference(
        &mut self,
        long_term_key: NativeVulkanH264LongTermPictureKey,
    ) -> Option<u32> {
        let reference = self.long_term_references.remove(&long_term_key)?;
        let slot = reference.dpb_slot;
        if self
            .slot_to_reference_key
            .get(slot as usize)
            .copied()
            .flatten()
            == Some(NativeVulkanH264DpbSlotKey::LongTerm(long_term_key))
            && let Some(slot_ref) = self.slot_to_reference_key.get_mut(slot as usize)
        {
            *slot_ref = None;
        }
        Some(slot)
    }

    pub(in crate::renderer::native_vulkan) fn clear_slot(&mut self, slot: u32) {
        let key = self
            .slot_to_reference_key
            .get(slot as usize)
            .copied()
            .flatten();
        match key {
            Some(NativeVulkanH264DpbSlotKey::ShortTerm(key)) => {
                self.short_term_reference_order
                    .retain(|existing| *existing != key);
                self.short_term_references.remove(&key);
            }
            Some(NativeVulkanH264DpbSlotKey::LongTerm(long_term_key)) => {
                self.long_term_references.remove(&long_term_key);
            }
            None => {}
        }
        if let Some(slot_ref) = self.slot_to_reference_key.get_mut(slot as usize) {
            *slot_ref = None;
        }
    }

    pub(in crate::renderer::native_vulkan) fn convert_short_term_to_long_term(
        &mut self,
        key: NativeVulkanH264ShortTermPictureKey,
        long_term_frame_idx: u16,
    ) -> Option<(u32, Option<u32>)> {
        let long_term_key =
            NativeVulkanH264LongTermPictureKey::from_short_term(key, long_term_frame_idx);
        let replaced_slot = self.remove_long_term_reference(long_term_key);
        self.short_term_reference_order
            .retain(|existing| *existing != key);
        let reference = self.short_term_references.remove(&key)?;
        let slot = reference.dpb_slot;
        self.long_term_references.insert(long_term_key, reference);
        if let Some(slot_ref) = self.slot_to_reference_key.get_mut(slot as usize) {
            *slot_ref = Some(NativeVulkanH264DpbSlotKey::LongTerm(long_term_key));
        }
        Some((slot, replaced_slot))
    }

    pub(in crate::renderer::native_vulkan) fn plan_next(
        &mut self,
        access_unit: &NativeVulkanH264AccessUnitSnapshot,
    ) -> NativeVulkanH264DecodeReferencePlanEntrySnapshot {
        let first_slice = access_unit.first_slice.as_ref();
        let idr = first_slice.is_some_and(|slice| slice.idr);
        if idr {
            self.reset();
        }

        let current_frame_num = first_slice.map(|slice| slice.frame_num);
        let current_pic_order_cnt_val = first_slice.map(|slice| {
            native_vulkan_h264_picture_order_cnt_val(
                slice.field_pic_flag,
                slice.bottom_field_flag,
                slice.pic_order_cnt,
            )
        });
        let current_pic_order_cnt = first_slice.map(|slice| slice.pic_order_cnt);
        let current_long_term_frame_idx = first_slice
            .filter(|slice| slice.idr && slice.long_term_reference_flag)
            .map(|_| 0u16);

        let mut unsupported_reason = access_unit.first_slice_parse_error.clone();
        let requested_l0_reference_count = first_slice
            .and_then(|slice| {
                if slice.is_p || slice.is_b {
                    slice
                        .num_ref_idx_l0_active_minus1
                        .map(|value| value.saturating_add(1))
                } else {
                    Some(0)
                }
            })
            .unwrap_or(0);
        let requested_l1_reference_count = first_slice
            .and_then(|slice| {
                slice
                    .is_b
                    .then(|| slice.num_ref_idx_l1_active_minus1.map(|value| value + 1))
                    .flatten()
            })
            .unwrap_or(0);
        let mut requested_reference_count = if first_slice.is_some_and(|slice| slice.is_p) {
            requested_l0_reference_count
        } else {
            0
        };
        if unsupported_reason.is_none() {
            if let Some(slice) = first_slice {
                unsupported_reason = if !native_vulkan_h264_ref_pic_list_modifications_supported(
                    slice,
                ) {
                    Some("H.264 unsupported reference list modification is not supported by the continuous direct gate".to_owned())
                } else if slice.is_p && requested_reference_count == 0 {
                    Some("H.264 P-slice requested zero active references".to_owned())
                } else if slice.is_b
                    && (requested_l0_reference_count == 0 || requested_l1_reference_count == 0)
                {
                    Some("H.264 B-slice requested zero active L0/L1 references".to_owned())
                } else if !slice.is_intra && !slice.is_p && !slice.is_b {
                    Some(format!(
                        "H.264 slice_type={} is not supported by the first continuous direct gate",
                        slice.slice_type
                    ))
                } else {
                    None
                };
            } else {
                unsupported_reason = Some(format!(
                    "H.264 AU {} has no parsed first slice",
                    access_unit.index
                ));
            }
        }

        let mut inferred_non_existing_plan = NativeVulkanH264InferredNonExistingPlan::default();
        if unsupported_reason.is_none()
            && let (Some(slice), Some(current_frame_num)) = (first_slice, current_frame_num)
            && !slice.idr
            && slice.is_reference
        {
            match self.infer_non_existing_short_term_references(current_frame_num) {
                Ok(plan) => inferred_non_existing_plan = plan,
                Err(err) => unsupported_reason = Some(err),
            }
        }

        let planned_output_slot = if current_frame_num.is_some() {
            self.choose_output_slot()
        } else {
            self.next_output_slot % self.dpb_slots
        };
        let evicted_key = self
            .slot_to_reference_key
            .get(planned_output_slot as usize)
            .copied()
            .flatten();
        let evicted_frame_num = match evicted_key {
            Some(NativeVulkanH264DpbSlotKey::ShortTerm(key)) => Some(key.frame_num),
            _ => None,
        };
        let evicted_long_term_frame_idx = match evicted_key {
            Some(NativeVulkanH264DpbSlotKey::LongTerm(long_term_key)) => {
                Some(long_term_key.frame_idx)
            }
            _ => None,
        };

        let mut reference_entries = NativeVulkanH264ReferenceListEntries::new();
        if unsupported_reason.is_none()
            && let Some(slice) = first_slice
        {
            if slice.is_b {
                match native_vulkan_h264_b_reference_frame_nums_for_slice(
                    slice,
                    &self.short_term_references,
                    &self.long_term_references,
                    planned_output_slot,
                    self.max_frame_num,
                ) {
                    Ok(entries) => {
                        requested_reference_count = entries.len() as u32;
                        reference_entries = entries;
                    }
                    Err(err) => unsupported_reason = Some(err),
                }
            } else if requested_reference_count > 0 {
                match native_vulkan_h264_reference_frame_nums_for_slice(
                    slice,
                    &self.short_term_references,
                    &self.long_term_references,
                    planned_output_slot,
                    self.max_frame_num,
                ) {
                    Ok(entries) => reference_entries = entries,
                    Err(err) => unsupported_reason = Some(err),
                }
            }
        }
        if unsupported_reason.is_none()
            && requested_reference_count > self.max_short_term_references
        {
            unsupported_reason = Some(format!(
                "H.264 slice requests {requested_reference_count} active references but stream/driver plan keeps {}",
                self.max_short_term_references
            ));
        }
        let mut adaptive_marking_plan = NativeVulkanH264AdaptiveMarkingPlan::default();
        if unsupported_reason.is_none()
            && let Some(slice) = first_slice
            && slice.is_reference
        {
            match native_vulkan_h264_adaptive_marking_plan_for_slice(
                slice,
                &self.short_term_references,
                &self.long_term_references,
                self.max_frame_num,
            ) {
                Ok(plan) => adaptive_marking_plan = plan,
                Err(err) => unsupported_reason = Some(err),
            }
        }

        let references = if unsupported_reason.is_none() && requested_reference_count > 0 {
            reference_entries
                .iter()
                .copied()
                .take(requested_reference_count as usize)
                .filter_map(|entry| {
                    let (
                        used_for_long_term_reference,
                        long_term_frame_idx,
                        long_term_pic_num,
                        reference,
                    ) = match entry {
                        NativeVulkanH264ReferenceListEntry::ShortTerm(key) => {
                            (false, None, None, self.short_term_references.get(&key))
                        }
                        NativeVulkanH264ReferenceListEntry::LongTerm(long_term_key) => (
                            true,
                            Some(long_term_key.frame_idx),
                            first_slice.and_then(|slice| {
                                u16::try_from(native_vulkan_h264_long_term_pic_num_for_key(
                                    long_term_key,
                                    NativeVulkanH264PictureFieldKind::from_slice(slice),
                                ))
                                .ok()
                            }),
                            self.long_term_references.get(&long_term_key),
                        ),
                    };
                    reference.map(|reference| NativeVulkanH264DecodeReferenceSnapshot {
                        frame_num: reference.frame_num,
                        field_pic_flag: reference.field_kind.field_pic_flag(),
                        bottom_field_flag: reference.field_kind.bottom_field_flag(),
                        used_for_long_term_reference,
                        long_term_frame_idx,
                        long_term_pic_num,
                        non_existing: reference.non_existing,
                        pic_order_cnt_val: reference.pic_order_cnt_val,
                        pic_order_cnt: reference.pic_order_cnt,
                        available: reference.dpb_slot != planned_output_slot,
                        source_access_unit_index: reference.source_access_unit_index,
                        dpb_slot: Some(reference.dpb_slot),
                    })
                })
                .collect::<NativeVulkanH264DecodeReferences>()
        } else {
            NativeVulkanH264DecodeReferences::new()
        };
        let available_reference_count = references
            .iter()
            .filter(|reference| reference.available)
            .count() as u32;
        let missing_reference_count =
            (references.len() as u32).saturating_sub(available_reference_count);
        let ready_for_decode_submit = current_frame_num.is_some()
            && current_pic_order_cnt_val.is_some()
            && current_pic_order_cnt.is_some()
            && unsupported_reason.is_none()
            && missing_reference_count == 0;

        let current_reference_long_term_frame_idx = adaptive_marking_plan
            .current_long_term_frame_idx
            .or(current_long_term_frame_idx);
        let mut dropped_reference_frame_nums = Vec::<u16>::new();
        let mut dropped_long_term_frame_indices = Vec::<u16>::new();
        let mut long_term_reference_conversions =
            Vec::<NativeVulkanH264LongTermReferenceConversionSnapshot>::new();
        let mut dropped_reference_slots = Vec::<u32>::new();
        if ready_for_decode_submit {
            if let Some(evicted_key) = evicted_key {
                match evicted_key {
                    NativeVulkanH264DpbSlotKey::ShortTerm(key) => {
                        self.remove_short_term_reference(key);
                    }
                    NativeVulkanH264DpbSlotKey::LongTerm(long_term_key) => {
                        self.remove_long_term_reference(long_term_key);
                    }
                }
            }
            self.clear_slot(planned_output_slot);
            for key in adaptive_marking_plan.drop_short_term_keys {
                if let Some(slot) = self.remove_short_term_reference(key) {
                    dropped_reference_frame_nums.push(key.frame_num);
                    dropped_reference_slots.push(slot);
                }
            }
            for long_term_key in adaptive_marking_plan.drop_long_term_keys {
                if let Some(slot) = self.remove_long_term_reference(long_term_key) {
                    if !dropped_long_term_frame_indices.contains(&long_term_key.frame_idx) {
                        dropped_long_term_frame_indices.push(long_term_key.frame_idx);
                    }
                    dropped_reference_slots.push(slot);
                }
            }
            for (key, long_term_frame_idx) in adaptive_marking_plan.convert_short_term_to_long_term
            {
                if let Some((slot, replaced_slot)) =
                    self.convert_short_term_to_long_term(key, long_term_frame_idx)
                {
                    long_term_reference_conversions.push(
                        NativeVulkanH264LongTermReferenceConversionSnapshot {
                            frame_num: key.frame_num,
                            long_term_frame_idx,
                            dpb_slot: slot,
                        },
                    );
                    if let Some(replaced_slot) = replaced_slot {
                        if !dropped_long_term_frame_indices.contains(&long_term_frame_idx) {
                            dropped_long_term_frame_indices.push(long_term_frame_idx);
                        }
                        if !dropped_reference_slots.contains(&replaced_slot) {
                            dropped_reference_slots.push(replaced_slot);
                        }
                    }
                }
            }
            if let (
                Some(slice),
                Some(current_frame_num),
                Some(current_pic_order_cnt_val),
                Some(current_pic_order_cnt),
            ) = (
                first_slice,
                current_frame_num,
                current_pic_order_cnt_val,
                current_pic_order_cnt,
            ) && slice.is_reference
            {
                let current_short_term_key = NativeVulkanH264ShortTermPictureKey::from_slice(slice);
                let reference_state = NativeVulkanH264DpbReferenceState {
                    source_access_unit_index: Some(access_unit.index),
                    dpb_slot: planned_output_slot,
                    pic_order_cnt_val: current_pic_order_cnt_val,
                    pic_order_cnt: current_pic_order_cnt,
                    frame_num: current_frame_num,
                    field_kind: current_short_term_key.field_kind,
                    non_existing: false,
                };
                if let Some(long_term_frame_idx) = current_reference_long_term_frame_idx {
                    let long_term_key =
                        NativeVulkanH264LongTermPictureKey::from_slice(slice, long_term_frame_idx);
                    if let Some(replaced_slot) = self.remove_long_term_reference(long_term_key) {
                        if replaced_slot != planned_output_slot {
                            dropped_reference_slots.push(replaced_slot);
                        }
                        if !dropped_long_term_frame_indices.contains(&long_term_frame_idx) {
                            dropped_long_term_frame_indices.push(long_term_frame_idx);
                        }
                    }
                    self.long_term_references
                        .insert(long_term_key, reference_state);
                    if let Some(slot) = self
                        .slot_to_reference_key
                        .get_mut(planned_output_slot as usize)
                    {
                        *slot = Some(NativeVulkanH264DpbSlotKey::LongTerm(long_term_key));
                    }
                } else {
                    if let Some(slot) = self
                        .slot_to_reference_key
                        .get_mut(planned_output_slot as usize)
                    {
                        *slot = Some(NativeVulkanH264DpbSlotKey::ShortTerm(
                            current_short_term_key,
                        ));
                    }
                    self.short_term_references
                        .insert(current_short_term_key, reference_state);
                    self.short_term_reference_order
                        .retain(|key| *key != current_short_term_key);
                    self.short_term_reference_order.push(current_short_term_key);
                    for (old_frame_num, old_slot) in self.enforce_sliding_reference_window() {
                        dropped_reference_frame_nums.push(old_frame_num);
                        dropped_reference_slots.push(old_slot);
                    }
                }
                self.previous_reference_frame_num = Some(current_frame_num);
            }
        }

        NativeVulkanH264DecodeReferencePlanEntrySnapshot {
            access_unit_index: access_unit.index,
            pts_ms: access_unit.pts_ms,
            nal_type_label: first_slice.map(|slice| slice.nal_type_label),
            current_frame_num,
            current_pic_order_cnt_val,
            current_pic_order_cnt,
            current_long_term_frame_idx: current_reference_long_term_frame_idx,
            planned_output_slot,
            setup_slot_index: first_slice
                .filter(|slice| slice.is_reference)
                .map(|_| planned_output_slot as i32),
            evicted_frame_num,
            evicted_long_term_frame_idx,
            dropped_reference_frame_nums,
            dropped_long_term_frame_indices,
            inferred_non_existing_frame_nums: inferred_non_existing_plan.frame_nums,
            inferred_non_existing_references: inferred_non_existing_plan.references,
            inferred_dropped_reference_frame_nums: inferred_non_existing_plan
                .dropped_short_term_frame_nums,
            inferred_dropped_long_term_frame_indices: inferred_non_existing_plan
                .dropped_long_term_frame_indices,
            inferred_dropped_reference_slots: inferred_non_existing_plan.dropped_reference_slots,
            long_term_reference_conversions,
            dropped_reference_slots,
            requested_reference_count,
            references,
            available_reference_count,
            missing_reference_count,
            unsupported_reason,
            ready_for_decode_submit,
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
#[allow(dead_code)]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_decode_reference_plan(
    access_units: &[NativeVulkanH264AccessUnitSnapshot],
    dpb_slots: u32,
    max_short_term_references: u32,
    max_frame_num: u32,
) -> Vec<NativeVulkanH264DecodeReferencePlanEntrySnapshot> {
    native_vulkan_h264_decode_reference_plan_with_gaps(
        access_units,
        dpb_slots,
        max_short_term_references,
        max_frame_num,
        true,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_decode_reference_plan_with_gaps(
    access_units: &[NativeVulkanH264AccessUnitSnapshot],
    dpb_slots: u32,
    max_short_term_references: u32,
    max_frame_num: u32,
    gaps_in_frame_num_allowed: bool,
) -> Vec<NativeVulkanH264DecodeReferencePlanEntrySnapshot> {
    let mut planner = NativeVulkanH264DecodeReferencePlanner::new(
        dpb_slots,
        max_short_term_references,
        max_frame_num,
        gaps_in_frame_num_allowed,
    );
    let mut plan = Vec::with_capacity(access_units.len());

    for access_unit in access_units {
        plan.push(planner.plan_next(access_unit));
    }

    plan
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_access_units_max_active_references(
    access_units: &[NativeVulkanH264AccessUnitSnapshot],
) -> u32 {
    access_units
        .iter()
        .filter_map(|access_unit| access_unit.first_slice.as_ref())
        .map(|slice| {
            let l0 = (slice.is_p || slice.is_b)
                .then(|| slice.num_ref_idx_l0_active_minus1.map(|value| value + 1))
                .flatten()
                .unwrap_or(0);
            let l1 = slice
                .is_b
                .then(|| slice.num_ref_idx_l1_active_minus1.map(|value| value + 1))
                .flatten()
                .unwrap_or(0);
            l0.saturating_add(l1)
        })
        .max()
        .unwrap_or(0)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH264StreamingBootstrap {
    pub(in crate::renderer::native_vulkan) stream_sps_dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) stream_dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) stream_max_active_reference_pictures: u32,
    pub(in crate::renderer::native_vulkan) max_frame_num: u32,
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_streaming_bootstrap_scan_limit(
    capacity: usize,
) -> usize {
    std::env::var("GILDER_VULKAN_STREAMING_BOOTSTRAP_SCAN_LIMIT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| capacity.max(1).saturating_mul(128).max(4096))
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_streaming_queue_has_field_pictures(
    queue: &NativeVulkanH264StreamingPacketQueue,
) -> bool {
    queue.bootstrap_access_units().iter().any(|access_unit| {
        access_unit
            .first_slice
            .as_ref()
            .is_some_and(|slice| slice.field_pic_flag)
    })
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_picture_layout_candidates(
    sps: &NativeVulkanH264SpsSnapshot,
    stream_has_field_pictures: bool,
) -> Vec<vk::VideoDecodeH264PictureLayoutFlagsKHR> {
    if sps.frame_mbs_only_flag {
        return vec![vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE];
    }
    if stream_has_field_pictures {
        return vec![
            vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_INTERLEAVED_LINES,
            vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_SEPARATE_PLANES,
        ];
    }
    vec![
        vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_INTERLEAVED_LINES,
        vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_SEPARATE_PLANES,
        vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE,
    ]
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_access_unit_starts_recovery(
    access_unit: &NativeVulkanH264AccessUnitSnapshot,
) -> bool {
    access_unit
        .first_slice
        .as_ref()
        .is_some_and(|slice| slice.idr)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_first_recovery_access_unit_offset(
    access_units: &[NativeVulkanH264AccessUnitSnapshot],
) -> Option<usize> {
    access_units
        .iter()
        .position(native_vulkan_h264_access_unit_starts_recovery)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_streaming_dpb_slot_budget(
    sps_dpb_slots: u32,
    active_reference_pictures: u32,
) -> u32 {
    sps_dpb_slots
        .max(active_reference_pictures.saturating_add(1))
        .max(1)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_align_streaming_bootstrap(
    queue: &mut NativeVulkanH264StreamingPacketQueue,
    parameter_sets: &NativeVulkanH264ParameterSetSnapshot,
) -> Result<NativeVulkanH264StreamingBootstrap, NativeVulkanError> {
    let scan_limit = native_vulkan_streaming_bootstrap_scan_limit(queue.capacity);
    let mut skipped_access_unit_indices = Vec::<u32>::new();
    loop {
        let bootstrap_access_units = queue.bootstrap_access_units();
        if bootstrap_access_units.is_empty() {
            return Err(NativeVulkanError::Video(format!(
                "H.264 streaming bootstrap could not find a decodable AU window after skipping {} leading AU(s)",
                skipped_access_unit_indices.len()
            )));
        }
        let stream_sps_dpb_slots = native_vulkan_h264_sps_dpb_slot_count(&parameter_sets.sps);
        let max_frame_num = native_vulkan_h264_sps_max_frame_num(&parameter_sets.sps);
        let stream_max_active_reference_pictures = parameter_sets
            .sps
            .max_num_ref_frames
            .max(native_vulkan_h264_access_units_max_active_references(
                &bootstrap_access_units,
            ))
            .max(1);
        let stream_max_dpb_slots = native_vulkan_h264_streaming_dpb_slot_budget(
            stream_sps_dpb_slots,
            stream_max_active_reference_pictures,
        );
        let (window_dpb_slots, bootstrap_plan) =
            native_vulkan_h264_min_decodable_dpb_plan_with_gaps(
                &bootstrap_access_units,
                stream_max_dpb_slots,
                stream_max_active_reference_pictures,
                max_frame_num,
                parameter_sets.sps.gaps_in_frame_num_value_allowed_flag,
            );
        let stream_dpb_slots = window_dpb_slots.max(stream_sps_dpb_slots);
        let recovery_offset =
            native_vulkan_h264_first_recovery_access_unit_offset(&bootstrap_access_units);
        let Some(first_unready_offset) = bootstrap_plan
            .iter()
            .position(|entry| !entry.ready_for_decode_submit)
        else {
            if recovery_offset == Some(0) {
                queue.set_loop_skip_access_units(
                    queue.bootstrap_discarded_access_units.min(u32::MAX),
                );
                return Ok(NativeVulkanH264StreamingBootstrap {
                    stream_sps_dpb_slots,
                    stream_dpb_slots,
                    stream_max_active_reference_pictures,
                    max_frame_num,
                });
            }
            let discard_count = recovery_offset.filter(|offset| *offset > 0).unwrap_or(1);
            if usize::try_from(queue.bootstrap_discarded_access_units)
                .unwrap_or(usize::MAX)
                .saturating_add(discard_count)
                > scan_limit
            {
                return Err(NativeVulkanError::Video(format!(
                    "H.264 streaming bootstrap exceeded scan limit {scan_limit} while looking for a recovery AU after skipping {} leading AU(s)",
                    queue.bootstrap_discarded_access_units
                )));
            }
            for _ in 0..discard_count {
                let Some(dropped) = queue.discard_front_for_bootstrap()? else {
                    return Err(NativeVulkanError::Video(format!(
                        "H.264 streaming bootstrap reached EOS after skipping {} leading AU(s) without finding a recovery AU",
                        queue.bootstrap_discarded_access_units
                    )));
                };
                skipped_access_unit_indices.push(dropped.access_unit_index);
            }
            continue;
        };
        let first_unready = &bootstrap_plan[first_unready_offset];
        let discard_count = recovery_offset
            .filter(|offset| *offset > 0)
            .unwrap_or(usize::from(first_unready_offset == 0));
        if discard_count == 0 {
            return Err(NativeVulkanError::Video(format!(
                "H.264 streaming bootstrap AU {} is not decodable with optimized DPB slot count {stream_dpb_slots} after skipping {} leading AU(s): {}",
                first_unready.access_unit_index,
                queue.bootstrap_discarded_access_units,
                first_unready
                    .unsupported_reason
                    .as_deref()
                    .unwrap_or("missing references")
            )));
        }
        if usize::try_from(queue.bootstrap_discarded_access_units)
            .unwrap_or(usize::MAX)
            .saturating_add(discard_count)
            > scan_limit
        {
            return Err(NativeVulkanError::Video(format!(
                "H.264 streaming bootstrap exceeded scan limit {scan_limit} while looking for a decodable AU window; last leading AU {} was not decodable: {}",
                first_unready.access_unit_index,
                first_unready
                    .unsupported_reason
                    .as_deref()
                    .unwrap_or("missing references")
            )));
        }
        for _ in 0..discard_count {
            let Some(dropped) = queue.discard_front_for_bootstrap()? else {
                return Err(NativeVulkanError::Video(format!(
                    "H.264 streaming bootstrap reached EOS after skipping {} leading AU(s) without finding a decodable window",
                    queue.bootstrap_discarded_access_units
                )));
            };
            skipped_access_unit_indices.push(dropped.access_unit_index);
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
#[allow(dead_code)]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_min_decodable_dpb_plan(
    access_units: &[NativeVulkanH264AccessUnitSnapshot],
    max_dpb_slots: u32,
    max_short_term_references: u32,
    max_frame_num: u32,
) -> (u32, Vec<NativeVulkanH264DecodeReferencePlanEntrySnapshot>) {
    native_vulkan_h264_min_decodable_dpb_plan_with_gaps(
        access_units,
        max_dpb_slots,
        max_short_term_references,
        max_frame_num,
        true,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h264_min_decodable_dpb_plan_with_gaps(
    access_units: &[NativeVulkanH264AccessUnitSnapshot],
    max_dpb_slots: u32,
    max_short_term_references: u32,
    max_frame_num: u32,
    gaps_in_frame_num_allowed: bool,
) -> (u32, Vec<NativeVulkanH264DecodeReferencePlanEntrySnapshot>) {
    let max_dpb_slots = max_dpb_slots.max(1);
    let mut last_plan = Vec::new();
    for dpb_slots in 1..=max_dpb_slots {
        let plan = native_vulkan_h264_decode_reference_plan_with_gaps(
            access_units,
            dpb_slots,
            max_short_term_references,
            max_frame_num,
            gaps_in_frame_num_allowed,
        );
        if plan.iter().all(|entry| entry.ready_for_decode_submit) {
            return (dpb_slots, plan);
        }
        last_plan = plan;
    }
    (max_dpb_slots, last_plan)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH265DecodeReferencePlanner {
    pub(in crate::renderer::native_vulkan) dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) max_pic_order_cnt_lsb: u32,
    pub(in crate::renderer::native_vulkan) poc_to_decoded_slot: BTreeMap<i32, (u32, u32)>,
    pub(in crate::renderer::native_vulkan) slot_to_poc: Vec<Option<i32>>,
    pub(in crate::renderer::native_vulkan) next_output_slot: u32,
    pub(in crate::renderer::native_vulkan) prev_poc_lsb: Option<i32>,
    pub(in crate::renderer::native_vulkan) prev_poc_msb: i32,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH265ReferenceRequest {
    pub(in crate::renderer::native_vulkan) delta_poc: i32,
    pub(in crate::renderer::native_vulkan) poc: i32,
    pub(in crate::renderer::native_vulkan) used_for_long_term_reference: bool,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanH265ReferenceRequest {
    const fn empty() -> Self {
        Self {
            delta_poc: 0,
            poc: 0,
            used_for_long_term_reference: false,
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
const NATIVE_VULKAN_H265_MAX_REFERENCE_REQUESTS: usize = 16;

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH265ActiveDpbReference {
    pub(in crate::renderer::native_vulkan) poc: i32,
    pub(in crate::renderer::native_vulkan) used_for_long_term_reference: bool,
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH265BeginSlotPolicy {
    pub(in crate::renderer::native_vulkan) active_only: bool,
    pub(in crate::renderer::native_vulkan) include_setup_slot: bool,
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_begin_slot_policy_from_env()
-> NativeVulkanH265BeginSlotPolicy {
    NativeVulkanH265BeginSlotPolicy {
        active_only: matches!(
            std::env::var("GILDER_VULKAN_H265_BEGIN_REFERENCE_SLOTS")
                .ok()
                .as_deref(),
            Some("active-only") | Some("active")
        ),
        include_setup_slot: matches!(
            std::env::var("GILDER_VULKAN_H265_BEGIN_SETUP_SLOT")
                .ok()
                .as_deref(),
            Some("1") | Some("true") | Some("yes") | Some("begin")
        ),
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_ref_pic_set_st_curr_before(
    access_unit_index: u32,
    available_references: &[&NativeVulkanH265DecodeReferenceSnapshot],
) -> Result<[u8; 8], NativeVulkanError> {
    native_vulkan_h265_ref_pic_set_slots_by(
        access_unit_index,
        available_references,
        "StCurrBefore",
        |reference| !reference.used_for_long_term_reference && reference.delta_poc < 0,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_ref_pic_set_st_curr_after(
    access_unit_index: u32,
    available_references: &[&NativeVulkanH265DecodeReferenceSnapshot],
) -> Result<[u8; 8], NativeVulkanError> {
    native_vulkan_h265_ref_pic_set_slots_by(
        access_unit_index,
        available_references,
        "StCurrAfter",
        |reference| !reference.used_for_long_term_reference && reference.delta_poc > 0,
    )
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_ref_pic_set_lt_curr(
    access_unit_index: u32,
    available_references: &[&NativeVulkanH265DecodeReferenceSnapshot],
) -> Result<[u8; 8], NativeVulkanError> {
    native_vulkan_h265_ref_pic_set_slots_by(
        access_unit_index,
        available_references,
        "LtCurr",
        |reference| reference.used_for_long_term_reference,
    )
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_h265_ref_pic_set_slots_by(
    access_unit_index: u32,
    available_references: &[&NativeVulkanH265DecodeReferenceSnapshot],
    label: &'static str,
    include: fn(&NativeVulkanH265DecodeReferenceSnapshot) -> bool,
) -> Result<[u8; 8], NativeVulkanError> {
    let mut slots = [0xffu8; 8];
    let mut reference_count = 0usize;
    for reference in available_references
        .iter()
        .copied()
        .filter(|reference| include(reference))
    {
        if reference_count >= slots.len() {
            return Err(NativeVulkanError::Video(format!(
                "H.265 AU {access_unit_index} has {} {label} references; Vulkan STD H.265 decode supports at most 8 entries",
                reference_count + 1
            )));
        }
        let dpb_slot = reference.dpb_slot.ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "H.265 AU {access_unit_index} reference POC {} has no DPB slot",
                reference.poc
            ))
        })?;
        slots[reference_count] = native_vulkan_h265_u8(dpb_slot, "RefPicSet slotIndex")
            .map_err(NativeVulkanError::Video)?;
        reference_count += 1;
    }
    Ok(slots)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_begin_slot_refs(
    active_dpb_refs: &[Option<NativeVulkanH265ActiveDpbReference>],
    references: &[NativeVulkanH265DecodeReferenceSnapshot],
    reset_before_decode: bool,
    policy: NativeVulkanH265BeginSlotPolicy,
) -> Vec<(u32, Option<NativeVulkanH265ActiveDpbReference>)> {
    active_dpb_refs
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(slot, active_reference)| {
            let reference_override = references
                .iter()
                .find(|reference| reference.available && reference.dpb_slot == Some(slot as u32))
                .map(|reference| NativeVulkanH265ActiveDpbReference {
                    poc: reference.poc,
                    used_for_long_term_reference: reference.used_for_long_term_reference,
                });
            let had_active_reference = active_reference.is_some() || reference_override.is_some();
            if policy.active_only && !had_active_reference {
                return None;
            }
            let reference = if reset_before_decode {
                None
            } else {
                reference_override.or(active_reference)
            };
            Some((slot as u32, reference))
        })
        .collect()
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_apply_reference_usage(
    active_dpb_refs: &mut [Option<NativeVulkanH265ActiveDpbReference>],
    references: &[NativeVulkanH265DecodeReferenceSnapshot],
) {
    for reference in references.iter().filter(|reference| reference.available) {
        let Some(dpb_slot) = reference.dpb_slot else {
            continue;
        };
        let Some(slot) = active_dpb_refs.get_mut(dpb_slot as usize) else {
            continue;
        };
        *slot = Some(NativeVulkanH265ActiveDpbReference {
            poc: reference.poc,
            used_for_long_term_reference: reference.used_for_long_term_reference,
        });
    }
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanH265DecodeReferencePlanner {
    pub(in crate::renderer::native_vulkan) fn new(
        dpb_slots: u32,
        max_pic_order_cnt_lsb: u32,
    ) -> Self {
        let dpb_slots = dpb_slots.max(1);
        Self {
            dpb_slots,
            max_pic_order_cnt_lsb: max_pic_order_cnt_lsb.max(1),
            poc_to_decoded_slot: BTreeMap::new(),
            slot_to_poc: vec![None; dpb_slots as usize],
            next_output_slot: 0,
            prev_poc_lsb: None,
            prev_poc_msb: 0,
        }
    }

    pub(in crate::renderer::native_vulkan) fn reset_for_idr(&mut self) {
        self.poc_to_decoded_slot.clear();
        self.slot_to_poc.fill(None);
        self.next_output_slot = 0;
        self.prev_poc_lsb = Some(0);
        self.prev_poc_msb = 0;
    }

    pub(in crate::renderer::native_vulkan) fn choose_output_slot(
        &mut self,
        protected_pocs: &[i32],
    ) -> u32 {
        for offset in 0..self.dpb_slots {
            let slot = (self.next_output_slot + offset) % self.dpb_slots;
            if self
                .slot_to_poc
                .get(slot as usize)
                .is_none_or(Option::is_none)
            {
                self.next_output_slot = (slot + 1) % self.dpb_slots;
                return slot;
            }
        }
        for offset in 0..self.dpb_slots {
            let slot = (self.next_output_slot + offset) % self.dpb_slots;
            let slot_poc = self.slot_to_poc.get(slot as usize).copied().flatten();
            if slot_poc.is_none_or(|poc| !protected_pocs.contains(&poc)) {
                self.next_output_slot = (slot + 1) % self.dpb_slots;
                return slot;
            }
        }
        let slot = self.next_output_slot % self.dpb_slots;
        self.next_output_slot = (slot + 1) % self.dpb_slots;
        slot
    }

    pub(in crate::renderer::native_vulkan) fn derive_current_poc(
        &mut self,
        slice: &NativeVulkanH265AccessUnitSliceSnapshot,
    ) -> Option<i32> {
        if slice.idr {
            self.prev_poc_lsb = Some(0);
            self.prev_poc_msb = 0;
            return Some(0);
        }
        let poc_lsb = slice.pic_order_cnt_lsb? as i32;
        let max_lsb = i32::try_from(self.max_pic_order_cnt_lsb).unwrap_or(i32::MAX);
        let prev_lsb = self.prev_poc_lsb.unwrap_or(0);
        let prev_msb = self.prev_poc_msb;
        let half_max_lsb = max_lsb / 2;
        let poc_msb = if poc_lsb < prev_lsb && prev_lsb.saturating_sub(poc_lsb) >= half_max_lsb {
            prev_msb.saturating_add(max_lsb)
        } else if poc_lsb > prev_lsb && poc_lsb.saturating_sub(prev_lsb) > half_max_lsb {
            prev_msb.saturating_sub(max_lsb)
        } else {
            prev_msb
        };
        self.prev_poc_lsb = Some(poc_lsb);
        self.prev_poc_msb = poc_msb;
        Some(poc_msb.saturating_add(poc_lsb))
    }

    pub(in crate::renderer::native_vulkan) fn derive_long_term_reference_poc(
        &self,
        slice: &NativeVulkanH265AccessUnitSliceSnapshot,
        current_poc: i32,
        reference: &NativeVulkanH265LongTermReferenceSnapshot,
    ) -> Option<i32> {
        if !reference.used_by_current {
            return None;
        }
        let max_lsb = i32::try_from(self.max_pic_order_cnt_lsb.max(1)).unwrap_or(i32::MAX);
        let poc_lsb = i32::try_from(reference.poc_lsb).ok()?;
        if let Some(delta_poc_msb_cycle_lt) = reference.delta_poc_msb_cycle_lt {
            let current_poc_lsb = slice.pic_order_cnt_lsb? as i32;
            let delta_msb = i32::try_from(delta_poc_msb_cycle_lt).ok()?;
            return Some(
                current_poc
                    .saturating_sub(delta_msb.saturating_mul(max_lsb))
                    .saturating_sub(current_poc_lsb.saturating_sub(poc_lsb)),
            );
        }
        self.poc_to_decoded_slot
            .keys()
            .copied()
            .find(|decoded_poc| decoded_poc.rem_euclid(max_lsb) == poc_lsb.rem_euclid(max_lsb))
            .or(Some(poc_lsb))
    }

    pub(in crate::renderer::native_vulkan) fn plan_next(
        &mut self,
        access_unit: &NativeVulkanH265AccessUnitSnapshot,
    ) -> NativeVulkanH265DecodeReferencePlanEntrySnapshot {
        let first_slice = access_unit.first_slice.as_ref();
        let idr = first_slice.is_some_and(|slice| slice.idr);
        if idr {
            self.reset_for_idr();
        }
        let current_poc = first_slice.and_then(|slice| self.derive_current_poc(slice));
        let mut unsupported_reason = access_unit.first_slice_parse_error.clone();
        let mut reference_requests =
            [NativeVulkanH265ReferenceRequest::empty(); NATIVE_VULKAN_H265_MAX_REFERENCE_REQUESTS];
        let mut reference_request_count = 0usize;
        if let (Some(slice), Some(current_poc)) = (first_slice, current_poc) {
            for delta_poc in slice.short_term_reference_delta_pocs.iter().copied() {
                if let Some(request) = reference_requests.get_mut(reference_request_count) {
                    *request = NativeVulkanH265ReferenceRequest {
                        delta_poc,
                        poc: current_poc.saturating_add(delta_poc),
                        used_for_long_term_reference: false,
                    };
                    reference_request_count += 1;
                } else if unsupported_reason.is_none() {
                    unsupported_reason = Some(format!(
                        "H.265 slice requests more than FFmpeg HEVC_MAX_REFS ({NATIVE_VULKAN_H265_MAX_REFERENCE_REQUESTS}) active references"
                    ));
                }
            }
            for long_term_reference in &slice.long_term_references {
                if let Some(poc) =
                    self.derive_long_term_reference_poc(slice, current_poc, long_term_reference)
                {
                    if let Some(request) = reference_requests.get_mut(reference_request_count) {
                        *request = NativeVulkanH265ReferenceRequest {
                            delta_poc: poc.saturating_sub(current_poc),
                            poc,
                            used_for_long_term_reference: true,
                        };
                        reference_request_count += 1;
                    } else if unsupported_reason.is_none() {
                        unsupported_reason = Some(format!(
                            "H.265 slice requests more than FFmpeg HEVC_MAX_REFS ({NATIVE_VULKAN_H265_MAX_REFERENCE_REQUESTS}) active references"
                        ));
                    }
                }
            }
        }
        let reference_requests = &reference_requests[..reference_request_count];
        let mut protected_pocs = [0i32; NATIVE_VULKAN_H265_MAX_REFERENCE_REQUESTS];
        for (index, request) in reference_requests.iter().enumerate() {
            protected_pocs[index] = request.poc;
        }
        let planned_output_slot = if current_poc.is_some() {
            self.choose_output_slot(&protected_pocs[..reference_requests.len()])
        } else {
            self.next_output_slot % self.dpb_slots
        };
        let evicted_poc = self
            .slot_to_poc
            .get(planned_output_slot as usize)
            .copied()
            .flatten();
        let mut references = NativeVulkanH265DecodeReferences::new();
        for request in reference_requests.iter().copied() {
            let source = self.poc_to_decoded_slot.get(&request.poc).copied();
            let available = source.is_some_and(|(_, slot)| slot != planned_output_slot);
            references.push(NativeVulkanH265DecodeReferenceSnapshot {
                delta_poc: request.delta_poc,
                poc: request.poc,
                used_for_long_term_reference: request.used_for_long_term_reference,
                available,
                source_access_unit_index: source.map(|(index, _)| index),
                dpb_slot: source.map(|(_, slot)| slot),
            });
        }
        let mut missing_reference_pocs = Vec::with_capacity(reference_requests.len());
        for reference in references.iter().filter(|reference| !reference.available) {
            missing_reference_pocs.push(reference.poc);
        }
        let available_reference_count = references
            .iter()
            .filter(|reference| reference.available)
            .count() as u32;
        let missing_reference_count = missing_reference_pocs.len() as u32;
        let ready_for_decode_submit =
            current_poc.is_some() && unsupported_reason.is_none() && missing_reference_count == 0;

        if ready_for_decode_submit && let Some(current_poc) = current_poc {
            if let Some(evicted_poc) = evicted_poc {
                self.poc_to_decoded_slot.remove(&evicted_poc);
            }
            if let Some(slot) = self.slot_to_poc.get_mut(planned_output_slot as usize) {
                *slot = Some(current_poc);
            }
            self.poc_to_decoded_slot
                .insert(current_poc, (access_unit.index, planned_output_slot));
        }

        NativeVulkanH265DecodeReferencePlanEntrySnapshot {
            access_unit_index: access_unit.index,
            pts_ms: access_unit.pts_ms,
            nal_type_label: first_slice.map(|slice| slice.nal_type_label),
            current_poc,
            planned_output_slot,
            setup_slot_index: current_poc.map(|_| planned_output_slot as i32),
            evicted_poc,
            references,
            available_reference_count,
            missing_reference_count,
            missing_reference_pocs,
            unsupported_reason,
            ready_for_decode_submit,
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_decode_reference_plan(
    access_units: &[NativeVulkanH265AccessUnitSnapshot],
    dpb_slots: u32,
    max_pic_order_cnt_lsb: u32,
) -> Vec<NativeVulkanH265DecodeReferencePlanEntrySnapshot> {
    let mut planner = NativeVulkanH265DecodeReferencePlanner::new(dpb_slots, max_pic_order_cnt_lsb);
    let mut plan = Vec::with_capacity(access_units.len());

    for access_unit in access_units {
        plan.push(planner.plan_next(access_unit));
    }

    plan
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_min_decodable_dpb_plan(
    access_units: &[NativeVulkanH265AccessUnitSnapshot],
    max_dpb_slots: u32,
    max_pic_order_cnt_lsb: u32,
) -> (u32, Vec<NativeVulkanH265DecodeReferencePlanEntrySnapshot>) {
    let max_dpb_slots = max_dpb_slots.max(1);
    let mut last_plan = Vec::new();
    for dpb_slots in 1..=max_dpb_slots {
        let plan = native_vulkan_h265_decode_reference_plan(
            access_units,
            dpb_slots,
            max_pic_order_cnt_lsb,
        );
        if plan.iter().all(|entry| entry.ready_for_decode_submit) {
            return (dpb_slots, plan);
        }
        last_plan = plan;
    }
    (max_dpb_slots, last_plan)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_access_unit_starts_recovery(
    access_unit: &NativeVulkanH265AccessUnitSnapshot,
) -> bool {
    access_unit
        .first_slice
        .as_ref()
        .is_some_and(|slice| slice.idr)
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) struct NativeVulkanH265StreamingBootstrap {
    pub(in crate::renderer::native_vulkan) stream_sps_dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) stream_dpb_slots: u32,
    pub(in crate::renderer::native_vulkan) stream_max_active_reference_pictures: u32,
    pub(in crate::renderer::native_vulkan) stream_max_pic_order_cnt_lsb: u32,
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_align_streaming_bootstrap(
    queue: &mut NativeVulkanH265StreamingPacketQueue,
    parameter_sets: &NativeVulkanH265ParameterSetSnapshot,
) -> Result<NativeVulkanH265StreamingBootstrap, NativeVulkanError> {
    let scan_limit = native_vulkan_streaming_bootstrap_scan_limit(queue.capacity);
    let mut skipped_access_unit_indices = Vec::<u32>::new();
    loop {
        let bootstrap_access_units = queue.bootstrap_access_units();
        if bootstrap_access_units.is_empty() {
            return Err(NativeVulkanError::Video(format!(
                "H.265 streaming bootstrap could not find a decodable AU window after skipping {} leading AU(s)",
                skipped_access_unit_indices.len()
            )));
        }
        let stream_sps_dpb_slots = native_vulkan_h265_sps_dpb_slot_count(&parameter_sets.sps);
        let stream_max_pic_order_cnt_lsb =
            native_vulkan_h265_sps_max_pic_order_cnt_lsb(&parameter_sets.sps);
        let stream_max_active_reference_pictures =
            native_vulkan_h265_access_units_max_active_references(&bootstrap_access_units)
                .max(stream_sps_dpb_slots.saturating_sub(1))
                .max(1);
        let (window_dpb_slots, bootstrap_plan) = native_vulkan_h265_min_decodable_dpb_plan(
            &bootstrap_access_units,
            stream_sps_dpb_slots,
            stream_max_pic_order_cnt_lsb,
        );
        let stream_dpb_slots = window_dpb_slots.max(stream_sps_dpb_slots);
        let recovery_offset = bootstrap_access_units
            .iter()
            .position(native_vulkan_h265_access_unit_starts_recovery);
        let Some(first_unready_offset) = bootstrap_plan
            .iter()
            .position(|entry| !entry.ready_for_decode_submit)
        else {
            if recovery_offset == Some(0) {
                queue.set_loop_skip_access_units(
                    queue.bootstrap_discarded_access_units.min(u32::MAX),
                );
                return Ok(NativeVulkanH265StreamingBootstrap {
                    stream_sps_dpb_slots,
                    stream_dpb_slots,
                    stream_max_active_reference_pictures,
                    stream_max_pic_order_cnt_lsb,
                });
            }
            let discard_count = recovery_offset.filter(|offset| *offset > 0).unwrap_or(1);
            if usize::try_from(queue.bootstrap_discarded_access_units)
                .unwrap_or(usize::MAX)
                .saturating_add(discard_count)
                > scan_limit
            {
                return Err(NativeVulkanError::Video(format!(
                    "H.265 streaming bootstrap exceeded scan limit {scan_limit} while looking for a recovery AU after skipping {} leading AU(s)",
                    queue.bootstrap_discarded_access_units
                )));
            }
            for _ in 0..discard_count {
                let Some(dropped) = queue.discard_front_for_bootstrap()? else {
                    return Err(NativeVulkanError::Video(format!(
                        "H.265 streaming bootstrap reached EOS after skipping {} leading AU(s) without finding a recovery AU",
                        queue.bootstrap_discarded_access_units
                    )));
                };
                skipped_access_unit_indices.push(dropped.access_unit_index);
            }
            continue;
        };
        let first_unready = &bootstrap_plan[first_unready_offset];
        let discard_count = recovery_offset
            .filter(|offset| *offset > 0)
            .unwrap_or(usize::from(first_unready_offset == 0));
        if discard_count == 0 {
            return Err(NativeVulkanError::Video(format!(
                "H.265 streaming bootstrap AU {} is not decodable with optimized DPB slot count {stream_dpb_slots} after skipping {} leading AU(s); missing POCs {:?}",
                first_unready.access_unit_index,
                queue.bootstrap_discarded_access_units,
                first_unready.missing_reference_pocs
            )));
        }
        if usize::try_from(queue.bootstrap_discarded_access_units)
            .unwrap_or(usize::MAX)
            .saturating_add(discard_count)
            > scan_limit
        {
            return Err(NativeVulkanError::Video(format!(
                "H.265 streaming bootstrap exceeded scan limit {scan_limit} while looking for a decodable AU window; last leading AU {} was missing POCs {:?}",
                first_unready.access_unit_index, first_unready.missing_reference_pocs
            )));
        }
        for _ in 0..discard_count {
            let Some(dropped) = queue.discard_front_for_bootstrap()? else {
                return Err(NativeVulkanError::Video(format!(
                    "H.265 streaming bootstrap reached EOS after skipping {} leading AU(s) without finding a decodable window",
                    queue.bootstrap_discarded_access_units
                )));
            };
            skipped_access_unit_indices.push(dropped.access_unit_index);
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_h265_access_units_max_active_references(
    access_units: &[NativeVulkanH265AccessUnitSnapshot],
) -> u32 {
    access_units
        .iter()
        .filter_map(|access_unit| access_unit.first_slice.as_ref())
        .map(|slice| {
            let short_term_count = slice
                .short_term_reference_delta_pocs
                .len()
                .min(u32::MAX as usize) as u32;
            let long_term_count = slice
                .long_term_references
                .iter()
                .filter(|reference| reference.used_by_current)
                .count()
                .min(u32::MAX as usize) as u32;
            short_term_count.saturating_add(long_term_count)
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn av1_planner_uses_transient_output_slot_when_reference_map_is_full() {
        let mut temporal_units = (0..8)
            .map(|index| test_av1_temporal_unit(index, 1u8 << index))
            .collect::<Vec<_>>();
        temporal_units.push(test_av1_temporal_unit(8, 0));

        let (dpb_slots, plan) = native_vulkan_av1_min_decodable_dpb_plan(&temporal_units, 16);
        let transient = plan.last().expect("AV1 transient frame is planned");

        assert_eq!(dpb_slots, 9);
        assert!(transient.ready_for_decode_submit);
        assert_eq!(transient.output_slot, Some(8));
        assert_eq!(transient.displayed_slot, Some(8));
        assert!(transient.refreshed_reference_names.is_empty());
        assert_eq!(
            transient.map_slot_indices_after,
            vec![0, 1, 2, 3, 4, 5, 6, 7]
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn h264_streaming_dpb_budget_keeps_output_slot_separate_from_single_reference() {
        let access_units = vec![
            test_h264_access_unit(0, true, 0),
            test_h264_access_unit(1, false, 1),
            test_h264_access_unit(2, false, 1),
        ];

        let (one_slot_count, one_slot_plan) =
            native_vulkan_h264_min_decodable_dpb_plan_with_gaps(&access_units, 1, 1, 16, false);
        assert_eq!(one_slot_count, 1);
        assert!(!one_slot_plan[1].ready_for_decode_submit);
        assert_eq!(one_slot_plan[1].missing_reference_count, 1);

        let budget = native_vulkan_h264_streaming_dpb_slot_budget(1, 1);
        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan_with_gaps(
            &access_units,
            budget,
            1,
            16,
            false,
        );
        assert_eq!(dpb_slots, 2);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[1].references[0].source_access_unit_index, Some(0));
        assert_ne!(
            plan[1].references[0].dpb_slot,
            Some(plan[1].planned_output_slot)
        );
        assert_eq!(plan[2].references[0].source_access_unit_index, Some(1));
        assert_ne!(
            plan[2].references[0].dpb_slot,
            Some(plan[2].planned_output_slot)
        );
    }

    #[cfg(feature = "native-vulkan-video")]
    #[test]
    fn h264_min_decodable_plan_uses_transient_slot_for_three_reference_p_slice() {
        let access_units = vec![
            test_h264_access_unit(0, true, 0),
            test_h264_access_unit(1, false, 1),
            test_h264_access_unit(2, false, 2),
            test_h264_access_unit(3, false, 3),
        ];

        let (three_slots, three_slot_plan) =
            native_vulkan_h264_min_decodable_dpb_plan_with_gaps(&access_units, 3, 3, 16, false);
        assert_eq!(three_slots, 3);
        assert!(!three_slot_plan[3].ready_for_decode_submit);
        assert_eq!(three_slot_plan[3].requested_reference_count, 3);
        assert_eq!(three_slot_plan[3].available_reference_count, 2);
        assert_eq!(three_slot_plan[3].missing_reference_count, 1);

        let budget = native_vulkan_h264_streaming_dpb_slot_budget(3, 3);
        let (dpb_slots, plan) = native_vulkan_h264_min_decodable_dpb_plan_with_gaps(
            &access_units,
            budget,
            3,
            16,
            false,
        );
        assert_eq!(dpb_slots, 4);
        assert!(plan.iter().all(|entry| entry.ready_for_decode_submit));
        assert_eq!(plan[3].requested_reference_count, 3);
        assert_eq!(plan[3].available_reference_count, 3);
        assert_eq!(plan[3].missing_reference_count, 0);
    }

    #[cfg(feature = "native-vulkan-video")]
    fn test_h264_access_unit(
        index: u32,
        idr: bool,
        l0_reference_count: u32,
    ) -> NativeVulkanH264AccessUnitSnapshot {
        NativeVulkanH264AccessUnitSnapshot {
            index,
            bytes: 1,
            byte_hash: index as u64,
            pts_ns: None,
            duration_ns: None,
            pts_ms: Some(u64::from(index) * 16),
            duration_ms: Some(16),
            has_annex_b_start_codes: true,
            has_parameter_sets: idr,
            h264_sps_count: u32::from(idr),
            h264_pps_count: u32::from(idr),
            h264_idr_count: u32::from(idr),
            h264_slice_count: 1,
            first_slice: Some(test_h264_slice(index as u16, idr, l0_reference_count)),
            first_slice_parse_error: None,
            idr_decode_ready: idr,
            decode_ready: true,
        }
    }

    #[cfg(feature = "native-vulkan-video")]
    fn test_h264_slice(
        frame_num: u16,
        idr: bool,
        l0_reference_count: u32,
    ) -> NativeVulkanH264AccessUnitSliceSnapshot {
        NativeVulkanH264AccessUnitSliceSnapshot {
            nal_type: if idr { 5 } else { 1 },
            nal_type_label: if idr { "idr" } else { "non-idr" },
            nal_ref_idc: 3,
            first_mb_in_slice: 0,
            first_slice_segment_in_pic_flag: true,
            slice_type: if idr { 2 } else { 0 },
            slice_type_normalized: if idr { 2 } else { 0 },
            pps_id: 0,
            frame_num,
            idr_pic_id: if idr { frame_num } else { 0 },
            num_ref_idx_l0_active_minus1: (!idr).then_some(l0_reference_count.saturating_sub(1)),
            num_ref_idx_l1_active_minus1: None,
            ref_pic_list_modification_l0: false,
            ref_pic_list_modifications_l0: Vec::new(),
            ref_pic_list_modification_l1: false,
            ref_pic_list_modifications_l1: Vec::new(),
            adaptive_ref_pic_marking_mode_flag: false,
            memory_management_control_operations: Vec::new(),
            field_pic_flag: false,
            bottom_field_flag: false,
            is_reference: true,
            is_intra: idr,
            is_p: !idr,
            is_b: false,
            long_term_reference_flag: false,
            pic_order_cnt: [i32::from(frame_num) * 2; 2],
            slice_offsets: NativeVulkanH264SliceOffsets::single(0),
            idr,
            irap: idr,
        }
    }

    fn test_av1_temporal_unit(
        index: u32,
        refresh_frame_flags: u8,
    ) -> NativeVulkanAv1TemporalUnitSnapshot {
        NativeVulkanAv1TemporalUnitSnapshot {
            index,
            bytes: 0,
            byte_hash: index as u64,
            pts_ns: None,
            duration_ns: None,
            pts_ms: None,
            duration_ms: None,
            obu_count: 1,
            sequence_header_count: 0,
            temporal_delimiter_count: 0,
            frame_header_count: 1,
            tile_group_count: 1,
            frame_count: 1,
            decode_candidate: true,
            tile_payload_bytes: 1,
            frame_payload_bytes: 1,
            first_frame_header_obu_offset: Some(0),
            first_tile_group_obu_offset: Some(0),
            sequence_header_present: false,
            sequence_header: None,
            first_frame_submit: Some(test_av1_frame_submit(index, refresh_frame_flags)),
            obus: Vec::new(),
        }
    }

    fn test_av1_frame_submit(
        index: u32,
        refresh_frame_flags: u8,
    ) -> NativeVulkanAv1FrameSubmitSnapshot {
        NativeVulkanAv1FrameSubmitSnapshot {
            parser: "test",
            frame_header_obu_offset: 0,
            frame_header_payload_offset: 0,
            frame_header_payload_size: 1,
            frame_header_offset_for_vulkan: 0,
            tile_count: 1,
            tile_columns: 1,
            tile_rows: 1,
            tile_size_bytes: 1,
            tile_offsets: vec![0],
            tile_sizes: vec![1],
            tile_payload_total_bytes: 1,
            frame_obu_payload_bytes: 1,
            frame_type: 1,
            frame_type_label: "inter",
            show_existing_frame: false,
            frame_to_show_map_idx: None,
            display_frame_id: None,
            current_frame_id: Some(index),
            expected_frame_ids: Vec::new(),
            show_frame: true,
            showable_frame: true,
            error_resilient_mode: false,
            disable_cdf_update: false,
            allow_screen_content_tools: 0,
            force_integer_mv: 0,
            allow_high_precision_mv: false,
            interpolation_filter: 0,
            interpolation_filter_label: "eighttap",
            is_filter_switchable: false,
            is_motion_mode_switchable: false,
            use_ref_frame_mvs: false,
            reference_select: false,
            skip_mode_present: false,
            allow_warped_motion: false,
            order_hint: Some(index as u8),
            primary_ref_frame: Some(0),
            refresh_frame_flags,
            reference_order_hints: Vec::new(),
            frame_refs_short_signaling: false,
            last_frame_idx: None,
            gold_frame_idx: None,
            ref_frame_indices: Vec::new(),
            render_and_frame_size_different: Some(false),
            frame_width: Some(640),
            frame_height: Some(368),
            render_width: Some(640),
            render_height: Some(368),
            found_frame_header: true,
            found_tile_payload: true,
            vulkan_submit_candidate: true,
            unsupported_reason: None,
        }
    }
}
