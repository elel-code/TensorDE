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
