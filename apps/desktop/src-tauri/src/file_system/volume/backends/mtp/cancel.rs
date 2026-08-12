//! Bridging Cmdr's cancellation into the poll-based token mtp-rs expects, for
//! the duration of one call.

use tokio_util::sync::CancellationToken;

/// Bridges Cmdr's `CancellationToken` to mtp-rs's poll-based `CancelToken` for
/// the duration of one call.
///
/// mtp-rs checks an `Arc<AtomicBool>` between PTP roundtrips, so something has
/// to mirror the token into it. A task parked on `cancelled()` costs nothing
/// while it waits (no polling), and the guard cancels its own child token when
/// the bridge drops, which retires that task at the end of every call — clean
/// exit, cancel, and error alike.
///
/// Live only for calls the caller actually made cancelable: [`Self::open`]
/// returns `None` for `None`, and the backend then passes no token to mtp-rs,
/// exactly as before.
pub(super) struct MtpCancelBridge {
    token: mtp_rs::CancelToken,
    _retire: tokio_util::sync::DropGuard,
}

impl MtpCancelBridge {
    pub(super) fn open(cancel: Option<&CancellationToken>) -> Option<Self> {
        let cancel = cancel?;
        let token = mtp_rs::CancelToken::new();
        // A CHILD token, so dropping the bridge retires the mirror task without
        // touching the caller's token (which outlives this one call).
        let scoped = cancel.child_token();
        let watch = scoped.clone();
        let mirror = token.clone();
        tokio::spawn(async move {
            watch.cancelled().await;
            mirror.cancel();
        });
        Some(Self {
            token,
            _retire: scoped.drop_guard(),
        })
    }

    pub(super) fn token(&self) -> &mtp_rs::CancelToken {
        &self.token
    }
}
