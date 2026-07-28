use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NativeVulkanAudioOutputMode {
    ClockOnly,
    Auto,
}

impl NativeVulkanAudioOutputMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClockOnly => "clock-only",
            Self::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NativeVulkanAudioOutputPolicy {
    Plan,
    Explicit(NativeVulkanAudioOutputMode),
}

impl NativeVulkanAudioOutputPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Explicit(mode) => mode.as_str(),
        }
    }

    pub fn parse_cli(value: &str) -> Result<Self, String> {
        match value {
            "plan" => Ok(Self::Plan),
            "clock-only" => Ok(Self::Explicit(NativeVulkanAudioOutputMode::ClockOnly)),
            "auto" => Ok(Self::Explicit(NativeVulkanAudioOutputMode::Auto)),
            _ => Err(format!("unsupported --audio-output: {value}")),
        }
    }

    pub fn resolve(self, muted: bool) -> NativeVulkanAudioOutputMode {
        match self {
            Self::Plan if muted => NativeVulkanAudioOutputMode::ClockOnly,
            Self::Plan => NativeVulkanAudioOutputMode::Auto,
            Self::Explicit(mode) => mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_policy_plan_follows_effective_muted_policy() {
        let policy = NativeVulkanAudioOutputPolicy::parse_cli("plan").unwrap();

        assert_eq!(policy.resolve(true), NativeVulkanAudioOutputMode::ClockOnly);
        assert_eq!(policy.resolve(false), NativeVulkanAudioOutputMode::Auto);
        assert_eq!(policy.as_str(), "plan");
    }

    #[test]
    fn output_policy_explicit_auto_overrides_effective_muted_policy() {
        let policy = NativeVulkanAudioOutputPolicy::parse_cli("auto").unwrap();

        assert_eq!(policy.resolve(true), NativeVulkanAudioOutputMode::Auto);
        assert_eq!(policy.resolve(false), NativeVulkanAudioOutputMode::Auto);
        assert_eq!(policy.as_str(), "auto");
    }
}
