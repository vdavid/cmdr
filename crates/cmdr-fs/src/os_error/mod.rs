//! [`OsErrorClass`]: what an OS error code MEANS, typed, so no layer above this one
//! compares an errno number.
//!
//! The code itself still travels (a `VolumeError`'s `raw_os_error`, a write error's
//! `errno`), because it's the OS's own diagnostic and the technical-details block
//! shows it. Everything that DECIDES (the listing error's reason, a retry, an outage
//! wait, the advice on a refusal) asks for the class instead. The class is a pure
//! function of the code on the OS that produced it, so carrying one is carrying the
//! other.
//!
//! Numbers differ per OS (`ENOTEMPTY` is 66 on macOS and 39 on Linux; Windows hands
//! back Win32 codes), so reading one is the platform seam in `platform/`.

mod platform;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// One kind of OS failure. A variant names a distinct answer some caller gives;
/// codes every caller treats alike share one.
///
/// ❌ Never classify by an error's message text: that's what this replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsErrorClass {
    /// The OS itself said no (`EPERM`): a locked file, System Integrity Protection, a
    /// privacy protection. Administrator rights change nothing.
    NotPermitted,
    /// The item's own permissions said no (`EACCES`). An administrator could.
    AccessDenied,
    /// No such file or folder (`ENOENT`).
    NotFound,
    /// A signal interrupted the call (`EINTR`).
    Interrupted,
    /// The device couldn't read or write (`EIO`).
    Io,
    /// The device behind the path isn't configured any more (`ENXIO`): its
    /// hardware left.
    DeviceNotConfigured,
    /// No device answers for the path (`ENODEV`): the mount is gone.
    NoDevice,
    /// Something else holds the resource (`EBUSY`): a mount point, a device.
    Busy,
    /// Another app has the file open in a way that keeps this one out. Windows'
    /// sharing and lock violations; ❗ no Unix code means this, so it's never
    /// produced there (`EBUSY` is [`Busy`](Self::Busy)).
    InUseByAnotherApp,
    /// Something is already at that name (`EEXIST`).
    AlreadyExists,
    /// The two paths are on different devices (`EXDEV`).
    CrossDevice,
    /// Something that isn't a folder is where one has to be (`ENOTDIR`).
    NotADirectory,
    /// A folder is where a file has to be (`EISDIR`).
    IsADirectory,
    /// The folder still holds something (`ENOTEMPTY`).
    NotEmpty,
    /// An argument the call can't take (`EINVAL`).
    InvalidArgument,
    /// The call doesn't apply to this kind of file (`ENOTTY`).
    InappropriateIoctl,
    /// The call isn't implemented at all (`ENOSYS`).
    NotImplemented,
    /// The filesystem doesn't do this (`ENOTSUP`, and Linux's `EOPNOTSUPP`, which is
    /// the same number there).
    Unsupported,
    /// macOS's `EOPNOTSUPP`, a separate number from its `ENOTSUP`. Linux never
    /// produces it.
    OperationNotSupported,
    /// The file would grow past what the filesystem allows (`EFBIG`).
    FileTooLarge,
    /// The disk is full (`ENOSPC`).
    NoSpace,
    /// The account's quota is spent (`EDQUOT`).
    QuotaExceeded,
    /// The volume is mounted read-only (`EROFS`).
    ReadOnlyFilesystem,
    /// Too many hard links to one item (`EMLINK`).
    TooManyLinks,
    /// The name is longer than the filesystem holds (`ENAMETOOLONG`).
    NameTooLong,
    /// The links lead round in a loop (`ELOOP`).
    LinkLoop,
    /// Out of memory (`ENOMEM`).
    OutOfMemory,
    /// Try again; the call would block (`EAGAIN`).
    WouldBlock,
    /// No locks left (`ENOLCK`).
    NoLocks,
    /// The operation was cancelled (`ECANCELED`).
    Canceled,
    /// The other end of a pipe or socket went away (`EPIPE`).
    BrokenPipe,
    /// The network is down (`ENETDOWN`).
    NetworkDown,
    /// No route to the network (`ENETUNREACH`).
    NetworkUnreachable,
    /// The network dropped the connection on reset (`ENETRESET`).
    NetworkReset,
    /// The connection was aborted on this side (`ECONNABORTED`).
    ConnectionAborted,
    /// The other side reset the connection (`ECONNRESET`).
    ConnectionReset,
    /// The socket isn't connected (`ENOTCONN`).
    NotConnected,
    /// The other side refused the connection (`ECONNREFUSED`).
    ConnectionRefused,
    /// A deadline passed with no answer (`ETIMEDOUT`).
    TimedOut,
    /// The host is down (`EHOSTDOWN`).
    HostDown,
    /// No route to the host (`EHOSTUNREACH`).
    HostUnreachable,
    /// A network file handle went stale (`ESTALE`).
    Stale,
    /// The item is on a remote machine this call can't reach through (`EREMOTE`).
    Remote,
    /// The other end broke the protocol (`EPROTO`).
    Protocol,
    /// A reply that isn't a message the protocol knows (`EBADMSG`).
    BadMessage,
    /// The server refused the credentials (macOS `EAUTH`).
    AuthFailed,
    /// The server needs credentials first (macOS `ENEEDAUTH`).
    AuthNeeded,
    /// The device is powered off (macOS `EPWROFF`).
    DevicePoweredOff,
    /// The device reported a fault (macOS `EDEVERR`).
    DeviceError,
    /// No such extended attribute (macOS `ENOATTR`).
    NoAttribute,
    /// A code no caller tells apart from the others.
    Other,
}

impl OsErrorClass {
    /// The class of `code`, a number this OS produced (an `io::Error`'s
    /// `raw_os_error`).
    #[must_use]
    pub fn of_code(code: i32) -> Self {
        platform::class_of(code)
    }

    /// The class of `err`, or `None` when it carries no OS code (a Rust-made
    /// `io::Error`, a backend's own words).
    #[must_use]
    pub fn of_io(err: &std::io::Error) -> Option<Self> {
        err.raw_os_error().map(Self::of_code)
    }

    /// This OS's code for the class, for a backend that reports a class it read off
    /// the wire as an OS error. `None` for a class this OS has no code for
    /// ([`InUseByAnotherApp`](Self::InUseByAnotherApp) and [`Other`](Self::Other) on
    /// Unix, every class in the browser build).
    #[must_use]
    pub fn host_code(self) -> Option<i32> {
        platform::code_of(self)
    }

    /// Whether this means "the link went away", not "the request was refused".
    ///
    /// The one transport set: the transfer engine's per-file retry and outage wait,
    /// and the local engine's error mapping all ask it. ❌ Don't keep a second list.
    #[must_use]
    pub fn is_transport(self) -> bool {
        matches!(
            self,
            Self::BrokenPipe
                | Self::ConnectionReset
                | Self::ConnectionAborted
                | Self::NotConnected
                | Self::TimedOut
                | Self::NetworkDown
                | Self::NetworkUnreachable
                | Self::HostUnreachable
        )
    }
}
