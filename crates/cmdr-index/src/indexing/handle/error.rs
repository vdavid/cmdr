//! What the index's public calls fail with.

use crate::indexing::events::Diagnostic;

/// Why an index operation couldn't be done.
///
/// Every variant a caller can act on is its own shape carrying the data needed
/// to act; [`Internal`](IndexError::Internal) is the residue, and its payload is
/// a log-only [`Diagnostic`]. ❌ Never branch on that text:
/// if a cause needs handling, it gets a
/// variant.
#[derive(Debug)]
pub enum IndexError {
    /// No index is registered for this volume, so there's nothing to act on. The
    /// normal answer for a drive that was never enabled, was turned off, or has
    /// been ejected — not a failure in itself.
    NotIndexed {
        /// The volume that isn't indexed.
        volume_id: String,
    },
    /// The index has no data directory, so nothing can be opened on disk. Means
    /// the host never handed the index its configuration.
    NotConfigured,
    /// This volume's transport isn't compiled on this platform, so it can't be
    /// indexed here.
    UnsupportedVolume {
        /// The volume that has nowhere to run.
        volume_id: String,
    },
    /// Something below the API failed. Log-only detail; classify by the variants
    /// above, never by this text.
    Internal(Diagnostic),
}

impl std::fmt::Display for IndexError {
    /// Diagnostic text for logs. Never rendered to a person: the app maps the
    /// variant to its own copy.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotIndexed { volume_id } => write!(f, "no index registered for volume '{volume_id}'"),
            Self::NotConfigured => f.write_str("the index has no data directory configured"),
            Self::UnsupportedVolume { volume_id } => {
                write!(f, "volume '{volume_id}' has no indexing transport on this platform")
            }
            Self::Internal(diagnostic) => f.write_str(diagnostic.as_str()),
        }
    }
}

impl std::error::Error for IndexError {}

impl From<String> for IndexError {
    /// The bridge for internals that still report a formatted diagnostic. Every
    /// one of these is a candidate for its own variant; nothing may match on the
    /// text in the meantime.
    fn from(diagnostic: String) -> Self {
        Self::Internal(Diagnostic::from(diagnostic))
    }
}

impl From<crate::indexing::host::config::DataDirUnset> for IndexError {
    fn from(_: crate::indexing::host::config::DataDirUnset) -> Self {
        Self::NotConfigured
    }
}
