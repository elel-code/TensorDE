use serde::{Serialize, Serializer};
use zvariant::{DynamicType, Signature, Structure, Value};

use crate::Result;

/// An owned D-Bus body decoded from its runtime signature.
///
/// `Fields` preserves top-level D-Bus argument boundaries. It can be passed
/// directly to typed send and reply APIs because its dynamic signature and
/// serialization match the original body rather than wrapping fields in
/// variants.
#[derive(Debug, Default)]
pub enum DynamicBody {
    #[default]
    Empty,
    Fields(Structure<'static>),
}

impl DynamicBody {
    /// Builds an owned body from runtime-typed top-level arguments.
    ///
    /// An empty iterator produces an empty D-Bus body. Every supplied value is
    /// one top-level argument; values are not wrapped in variants implicitly.
    pub fn from_fields<I, V>(fields: I) -> Result<Self>
    where
        I: IntoIterator<Item = V>,
        V: Into<Value<'static>>,
    {
        let mut fields = fields.into_iter().peekable();
        if fields.peek().is_none() {
            return Ok(Self::Empty);
        }
        let mut structure = zvariant::StructureBuilder::new();
        for field in fields {
            structure.push_value(field.into());
        }
        Ok(Self::Fields(structure.build()?))
    }

    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub fn fields(&self) -> &[Value<'static>] {
        match self {
            Self::Empty => &[],
            Self::Fields(fields) => fields.fields(),
        }
    }

    pub(crate) const fn from_structure(structure: Structure<'static>) -> Self {
        Self::Fields(structure)
    }
}

impl DynamicType for DynamicBody {
    fn signature(&self) -> Signature {
        match self {
            Self::Empty => Signature::Unit,
            Self::Fields(fields) => fields.signature().clone(),
        }
    }
}

impl Serialize for DynamicBody {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Empty => ().serialize(serializer),
            Self::Fields(fields) => fields.serialize(serializer),
        }
    }
}
