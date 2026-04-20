//! KDL specification version selection.

/// Which KDL specification version the formatter should target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum KdlVersion {
    /// KDL v1 (legacy).
    V1,
    /// KDL v2 (current).
    #[default]
    V2,
}

#[cfg(feature = "clap")]
impl clap::ValueEnum for KdlVersion {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::V1, Self::V2]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            Self::V1 => clap::builder::PossibleValue::new("v1"),
            Self::V2 => clap::builder::PossibleValue::new("v2"),
        })
    }
}
