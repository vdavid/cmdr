//! What a search needs from its host besides the query: the name rules, the home
//! folder, and the importance weights.
//!
//! All three used to be read ambiently (`cfg!(target_os)`, `dirs::home_dir()`, a
//! static map). As values they make the core host-independent: a test runs the macOS
//! rules on Linux, and a browser host that presents as a Mac says so instead of
//! silently taking the Linux answer its `cfg` would give it.
//!
//! This is the aggregate, above its parts: the name rules are a leaf of their own
//! (`name_rules.rs`), and the weights are ranking's.

use super::name_rules::NameRules;
use super::ranking::ImportanceWeights;

/// Everything one search reads besides the query and the rows.
#[derive(Debug, Clone, Copy)]
pub struct SearchContext<'a> {
    /// The volume's name rules.
    pub rules: NameRules,
    /// The absolute home folder, written as `~` in a result's parent path; `None`
    /// when the host has none.
    pub home_dir: Option<&'a str>,
    /// Folder importance for ranking; empty for an unscored volume.
    pub weights: &'a ImportanceWeights,
}

impl<'a> SearchContext<'a> {
    /// This machine's rules with no home folder: the context the engine's own tests
    /// and benches search under.
    #[cfg(any(test, feature = "testing"))]
    pub fn for_test(weights: &'a ImportanceWeights) -> Self {
        Self {
            rules: NameRules::host(),
            home_dir: None,
            weights,
        }
    }
}
