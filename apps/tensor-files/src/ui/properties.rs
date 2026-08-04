#[path = "properties/geometry.rs"]
pub(crate) mod geometry;
#[path = "properties/paint.rs"]
pub(crate) mod paint;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellPropertyRow {
    pub(crate) label: &'static str,
    pub(crate) value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellPropertiesOverlay {
    pub(crate) title: String,
    pub(crate) rows: Vec<ShellPropertyRow>,
}

pub(crate) fn property_row(label: &'static str, value: String) -> ShellPropertyRow {
    ShellPropertyRow { label, value }
}
