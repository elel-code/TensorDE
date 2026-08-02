use vulkanalia::vk;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexFormat {
    Uint16,
    Uint32,
}

impl IndexFormat {
    pub(super) const fn as_vk(self) -> vk::IndexType {
        match self {
            Self::Uint16 => vk::IndexType::UINT16,
            Self::Uint32 => vk::IndexType::UINT32,
        }
    }

    pub(super) const fn alignment(self) -> u64 {
        match self {
            Self::Uint16 => 2,
            Self::Uint32 => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoadOp<T> {
    Load,
    Clear(T),
    Discard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreOp {
    Store,
    Discard,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ResolveMode {
    #[default]
    None,
    SampleZero,
    Average,
    Min,
    Max,
}

impl ResolveMode {
    pub(super) const fn to_vk(self) -> vk::ResolveModeFlags {
        match self {
            Self::None => vk::ResolveModeFlags::NONE,
            Self::SampleZero => vk::ResolveModeFlags::SAMPLE_ZERO,
            Self::Average => vk::ResolveModeFlags::AVERAGE,
            Self::Min => vk::ResolveModeFlags::MIN,
            Self::Max => vk::ResolveModeFlags::MAX,
        }
    }
}
