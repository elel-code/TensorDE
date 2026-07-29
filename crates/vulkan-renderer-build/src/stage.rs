use std::{fmt, str::FromStr};

use crate::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}

impl ShaderStage {
    pub const fn slang_name(self) -> &'static str {
        match self {
            Self::Vertex => "vertex",
            Self::Fragment => "fragment",
            Self::Compute => "compute",
        }
    }
}

impl fmt::Display for ShaderStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.slang_name())
    }
}

impl FromStr for ShaderStage {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "vertex" => Ok(Self::Vertex),
            "fragment" => Ok(Self::Fragment),
            "compute" => Ok(Self::Compute),
            _ => Err(Error::InvalidStage(value.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_names_round_trip() {
        for stage in [
            ShaderStage::Vertex,
            ShaderStage::Fragment,
            ShaderStage::Compute,
        ] {
            assert_eq!(stage.to_string().parse::<ShaderStage>().unwrap(), stage);
        }
    }
}
