//! The seams a storage backend reaches its host through.
//!
//! A backend crate implements [`Volume`](super::Volume) and knows one protocol.
//! Everything else it needs — telling the open panes a directory changed, asking
//! for stored credentials, spawning a watcher task, reporting a connection drop —
//! belongs to the application around it. This module is the complete list of
//! those questions, so "what does a filesystem backend need from Cmdr?" has one
//! readable answer, and a new backend can be written against it without opening
//! the app crate at all.
//!
//! ## How a backend gets one
//!
//! The app builds ONE [`VolumeHost`] at startup and hands a clone to every
//! backend it constructs. A backend stores it as a field and reads a seam where
//! it needs one:
//!
//! ```ignore
//! let host = VolumeHost::builder()
//!     .runtime(tokio_handle)
//!     .listings(Arc::new(AppListings))
//!     .events(Arc::new(AppVolumeEvents::new(app_handle)))
//!     // …the rest…
//!     .build();
//! let volume = FtpVolume::connect(params, host.clone()).await?;
//! ```
//!
//! ❌ **Don't reach a seam through a process-wide static of your own.** The host
//! is a value on purpose: a test builds one with fakes and passes it in, so no
//! seam needs an install-and-restore guard, and nothing has to be serialized
//! against the rest of the test binary.
//!
//! ## The dispatch rule
//!
//! **A seam may be called per mutation, never per directory entry.** Every seam
//! here is a `dyn` trait object, which is free at human cadence (a file landed, a
//! session dropped, a scan started) and is not free inside a loop over 250 000
//! entries. If you want to ask a seam something per entry, hoist the call: take
//! one answer before the loop and carry it. `DETAILS.md` § "The dispatch rule"
//! has the reasoning and what to do when a per-entry answer really is needed.
//!
//! ## Every seam degrades, none panics
//!
//! [`VolumeHost::detached`] answers nothing: no pane updates, no events, no
//! credentials, a fresh fallback runtime. That's what a bench, a CLI tool, or a
//! test that doesn't care reaches for, and it means no backend needs an
//! `Option<VolumeHost>` or an error path for "there was no host".
//!
//! ## The seams
//!
//! - [`runtime`]: the tokio runtime background work spawns onto. NOT a trait —
//!   the host injects a real [`tokio::runtime::Handle`].
//! - [`listings`]: what the open panes are showing, and telling them it changed.
//! - [`events`]: typed events the frontend renders (a connection came or went).
//! - [`credentials`]: the OS secret store, for a backend that authenticates.
//! - [`host_keys`]: the SSH host keys this machine already trusts.
//! - [`indexing`]: telling the file index that live watching lost continuity.
//! - [`settings`]: the live, user-tunable knobs a backend reads per dispatch.
//! - [`activity`]: whether the user is busy on a volume, so bulk work stands aside.
//! - [`analytics`]: PII-free product counters.
//!
//! Rationale, what deliberately ISN'T a seam, and the sites each one covers:
//! `DETAILS.md`.

pub mod activity;
pub mod analytics;
pub mod credentials;
pub mod events;
pub mod host_keys;
pub mod indexing;
pub mod listings;
pub mod runtime;
pub mod settings;

use std::sync::Arc;

use tokio::runtime::Handle;

use activity::UserActivity;
use analytics::AnalyticsSink;
use credentials::CredentialStore;
use events::VolumeEventSink;
use host_keys::HostKeys;
use indexing::IndexNotifier;
use listings::ListingHost;
use settings::BackendSettings;

/// Everything a storage backend asks its host, in one cheaply-cloned value.
///
/// Build one per process with [`VolumeHost::builder`] and hand a clone to each
/// backend. Cloning is eight `Arc` bumps and a `Handle` clone, so a backend can
/// hold one per volume instance without thinking about it.
#[derive(Clone)]
pub struct VolumeHost {
    runtime: Option<Handle>,
    listings: Arc<dyn ListingHost>,
    events: Arc<dyn VolumeEventSink>,
    credentials: Arc<dyn CredentialStore>,
    host_keys: Arc<dyn HostKeys>,
    indexing: Arc<dyn IndexNotifier>,
    activity: Arc<dyn UserActivity>,
    analytics: Arc<dyn AnalyticsSink>,
    settings: Arc<dyn BackendSettings>,
}

impl VolumeHost {
    /// Starts building a host. Every seam it doesn't get answers nothing, the
    /// same way [`detached`](Self::detached) does.
    pub fn builder() -> VolumeHostBuilder {
        VolumeHostBuilder { host: Self::detached() }
    }

    /// A host that answers nothing: pane updates and events go nowhere, no
    /// credentials are stored, no SSH host key is trusted, the index hears
    /// nothing, every volume reads as idle, and background work spawns onto the
    /// fallback runtime.
    ///
    /// This is what a bench, a CLI tool, or a test that only exercises protocol
    /// code reaches for. It's a complete host, not a stub: nothing a backend
    /// calls on it fails or panics.
    pub fn detached() -> Self {
        Self {
            runtime: None,
            listings: Arc::new(listings::NoListings),
            events: Arc::new(events::NoVolumeEvents),
            credentials: Arc::new(credentials::NoCredentials),
            host_keys: Arc::new(host_keys::NoHostKeys),
            indexing: Arc::new(indexing::NoIndexNotifier),
            activity: Arc::new(activity::AlwaysIdle),
            analytics: Arc::new(analytics::NoAnalytics),
            settings: Arc::new(settings::DefaultBackendSettings),
        }
    }

    /// The runtime background work spawns onto: whatever the host injected, or
    /// the shared fallback. See [`runtime`] for why a backend must never call
    /// `tokio::spawn` directly.
    pub fn runtime(&self) -> Handle {
        runtime::resolve(self.runtime.as_ref())
    }

    /// What the open panes are showing, and how to tell them something changed.
    pub fn listings(&self) -> &dyn ListingHost {
        self.listings.as_ref()
    }

    /// Where a typed event for the frontend goes.
    pub fn events(&self) -> &dyn VolumeEventSink {
        self.events.as_ref()
    }

    /// The OS secret store.
    pub fn credentials(&self) -> &dyn CredentialStore {
        self.credentials.as_ref()
    }

    /// The SSH host keys this machine already trusts.
    pub fn host_keys(&self) -> &dyn HostKeys {
        self.host_keys.as_ref()
    }

    /// How the file index hears that live watching lost continuity.
    pub fn indexing(&self) -> &dyn IndexNotifier {
        self.indexing.as_ref()
    }

    /// Whether the user is busy on a volume right now.
    pub fn activity(&self) -> &dyn UserActivity {
        self.activity.as_ref()
    }

    /// PII-free product counters.
    pub fn analytics(&self) -> &dyn AnalyticsSink {
        self.analytics.as_ref()
    }

    /// The live, user-tunable knobs a backend reads per dispatch.
    pub fn settings(&self) -> &dyn BackendSettings {
        self.settings.as_ref()
    }
}

/// Builds a [`VolumeHost`], one seam at a time.
///
/// Every seam starts at its no-op answer, so the app installs the ones it can
/// answer and a test installs only the ones it asserts on.
pub struct VolumeHostBuilder {
    host: VolumeHost,
}

impl VolumeHostBuilder {
    /// The tokio runtime background work spawns onto. Without it, backends fall
    /// back to a shared runtime this crate builds (see [`runtime`]).
    #[must_use]
    pub fn runtime(mut self, handle: Handle) -> Self {
        self.host.runtime = Some(handle);
        self
    }

    /// What the open panes are showing.
    #[must_use]
    pub fn listings(mut self, listings: Arc<dyn ListingHost>) -> Self {
        self.host.listings = listings;
        self
    }

    /// Where typed events for the frontend go.
    #[must_use]
    pub fn events(mut self, events: Arc<dyn VolumeEventSink>) -> Self {
        self.host.events = events;
        self
    }

    /// The OS secret store.
    #[must_use]
    pub fn credentials(mut self, credentials: Arc<dyn CredentialStore>) -> Self {
        self.host.credentials = credentials;
        self
    }

    /// The SSH host keys this machine already trusts.
    #[must_use]
    pub fn host_keys(mut self, host_keys: Arc<dyn HostKeys>) -> Self {
        self.host.host_keys = host_keys;
        self
    }

    /// How the file index hears about a watch gap.
    #[must_use]
    pub fn indexing(mut self, indexing: Arc<dyn IndexNotifier>) -> Self {
        self.host.indexing = indexing;
        self
    }

    /// Whether the user is busy on a volume.
    #[must_use]
    pub fn activity(mut self, activity: Arc<dyn UserActivity>) -> Self {
        self.host.activity = activity;
        self
    }

    /// Where PII-free product counters go.
    #[must_use]
    pub fn analytics(mut self, analytics: Arc<dyn AnalyticsSink>) -> Self {
        self.host.analytics = analytics;
        self
    }

    /// The live, user-tunable knobs.
    #[must_use]
    pub fn settings(mut self, settings: Arc<dyn BackendSettings>) -> Self {
        self.host.settings = settings;
        self
    }

    /// The finished host, ready to hand to every backend.
    #[must_use]
    pub fn build(self) -> VolumeHost {
        self.host
    }
}

#[cfg(test)]
mod host_test;
