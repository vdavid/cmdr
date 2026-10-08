//! "Copy share link": a URL anyone can open to download one file, for as long
//! as the link lasts. [`Volume::share_link`](super::Volume::share_link) mints
//! one; S3 is the only backend that can today (a presigned GET).

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How long a share link lasts. The three choices the menu and the palette
/// offer, ❗ all inside S3's seven-day ceiling on a SigV4 signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareLinkExpiry {
    /// One hour.
    OneHour,
    /// One day.
    OneDay,
    /// Seven days: the default, and the longest a SigV4 signature may live.
    SevenDays,
}

impl ShareLinkExpiry {
    /// How long that is.
    #[must_use]
    pub fn duration(self) -> Duration {
        Duration::from_secs(match self {
            Self::OneHour => 3_600,
            Self::OneDay => 86_400,
            Self::SevenDays => 604_800,
        })
    }
}

/// A minted share link.
///
/// ❗ **The URL is a credential**: its signature lets anyone holding it read the
/// file until it expires. So `Debug` prints no URL and there's no `Display`, and
/// the only way out is [`into_url`](Self::into_url), at the one place that
/// hands it to the user (the clipboard). ❌ Never log what that returns.
#[derive(Clone, PartialEq, Eq)]
pub struct ShareLink(String);

impl ShareLink {
    /// Wraps a minted URL.
    #[must_use]
    pub fn new(url: String) -> Self {
        Self(url)
    }

    /// The URL itself, for the clipboard. ❌ Never for a log line.
    #[must_use]
    pub fn into_url(self) -> String {
        self.0
    }
}

impl std::fmt::Debug for ShareLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ShareLink(<redacted>)")
    }
}
