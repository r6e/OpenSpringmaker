//! Shared pick-list item type for keyed option lists (end-types, fixities, etc.).
//!
//! `KeyLabel` and the shared `END_TYPES` const are defined here so both
//! `compression::view` and `conical::view` can use the same canonical list
//! without duplicating it.

/// A (key, label) pair for end-type and fixity pick-lists.
///
/// The `Display` impl renders the human-readable label; the key is used to
/// store the value in form state and round-trip through save/load.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyLabel {
    pub(crate) key: &'static str,
    pub(crate) label: &'static str,
}

impl std::fmt::Display for KeyLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label)
    }
}

/// All end-type options in display order.
pub(crate) const END_TYPES: &[KeyLabel] = &[
    KeyLabel {
        key: "plain",
        label: "Plain",
    },
    KeyLabel {
        key: "plain_ground",
        label: "Plain ground",
    },
    KeyLabel {
        key: "squared",
        label: "Squared",
    },
    KeyLabel {
        key: "squared_ground",
        label: "Squared and ground",
    },
];

/// All end-fixity options in display order (buckling boundary condition).
pub(crate) const FIXITIES: &[KeyLabel] = &[
    KeyLabel {
        key: "fixed_fixed",
        label: "Fixed-Fixed",
    },
    KeyLabel {
        key: "fixed_pinned",
        label: "Fixed-Pinned",
    },
    KeyLabel {
        key: "pinned_pinned",
        label: "Pinned-Pinned",
    },
    KeyLabel {
        key: "fixed_free",
        label: "Fixed-Free",
    },
];

/// All topology options in display order.
pub(crate) const TOPOLOGIES: &[KeyLabel] = &[
    KeyLabel {
        key: "nested",
        label: "Nested",
    },
    KeyLabel {
        key: "series",
        label: "Series",
    },
];

/// Find a `KeyLabel` by its stored key string. Returns `None` if the key is
/// unrecognised (e.g. a future format loaded into an older binary).
pub(crate) fn find_by_key<'a>(options: &'a [KeyLabel], key: &str) -> Option<&'a KeyLabel> {
    options.iter().find(|kl| kl.key == key)
}
