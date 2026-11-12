//! The two spellings of a remote volume's tree, and the one translation between
//! them.
//!
//! A remote backend has no mount, so it needs an app-facing spelling of its
//! files that is self-describing. `sftp://test_user_1@testnas.local:22/srv/data/photos` is
//! one; `/srv/data/photos` is not, and that matters because
//! `commands/volumes.rs::resolve_path_to_volume` falls through to the mount
//! table, which on both platforms walks up to `/` and answers the LOCAL root for
//! any absolute path it doesn't recognize. Every un-hinted resolver call (a
//! restored tab, a favorite, a drag between panes, go-to-path, MCP, the trash)
//! would send a remote path to the boot disk. It is also the convention the app
//! already has for every volume without a local mount (`mtp://`, `adb://`).
//!
//! So a remote volume is rooted at `<prefix><remote root>`, where the prefix is
//! minted here from the volume id's own identity tuple ([`sftp_app_root`],
//! [`webdav_app_root`], [`s3_app_root`], [`nfs_app_root`], [`adb_app_root`], each
//! with its split back:
//! [`server_of_path`], [`adb_serial_of_path`]) and the tree under it is the
//! server's own. The whole path grammar lives in this one module, so the next
//! scheme's author finds both halves. [`RemoteRoot`]
//! holds both and is the ONLY translation: `to_remote_path` going down,
//! `to_app_path` coming back. ❗ Both refuse a path off the root, by the same
//! component-wise check after `..`: coming back, the input is a server's answer,
//! and a misbehaving server must not mint an app path off the volume.
//!
//! ❗ **A bare server-absolute path is REFUSED, not anchored.** Five app sites
//! run a path through [`super::root_anchored`] before the volume sees it, and
//! that helper JOINS anything not under the root onto it: `/srv/data/x` becomes
//! `sftp://…/srv/data/srv/data/x`, strips back to a real server path, and never
//! refuses. With the prefix in place the app never spells a remote path bare, so
//! leniency buys nothing but that hole. The three root aliases stay (`/`, `.`,
//! and the empty path), because a backend's own code passes a bare `/` for its
//! root (`cmdr-webdav`'s `query.rs` does, from `is_root` and
//! `get_space_info_impl`).

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

use super::{canonical_nfs_export, nfs_volume_id, sftp_volume_id, webdav_volume_id};

/// A remote volume's root, in both the spellings it has.
///
/// Built once per volume and held by it, so the translation costs no allocation
/// per call beyond the answer itself.
#[derive(Clone)]
pub struct RemoteRoot {
    /// `sftp://<user>@<host>:<port>`: what every app path on this volume starts
    /// with. From [`sftp_app_root`] or one of its twins, ❌ never
    /// hand-built, so a path and the id it resolves to can't disagree.
    prefix: String,
    /// The same, as a `Path`, so the match is by whole COMPONENTS. A string
    /// compare would let `…testnas.local:2222` borrow `…testnas.local:22`'s volume.
    prefix_path: PathBuf,
    /// What the app addresses the root by: `<prefix><remote>`.
    app: PathBuf,
    /// The server-side directory this volume is rooted at, normalized and
    /// absolute.
    remote: PathBuf,
}

impl RemoteRoot {
    /// Builds a root from its prefix and the server-side directory under it.
    ///
    /// `remote_root` is normalized and made absolute, so a backend handed a
    /// relative or `..`-carrying root still ends up with one spelling of it.
    pub fn new(prefix: String, remote_root: &Path) -> Self {
        let remote = normalize_remote_path(remote_root);
        let app = PathBuf::from(format!("{prefix}{}", remote.to_string_lossy()));
        Self {
            prefix_path: PathBuf::from(&prefix),
            prefix,
            app,
            remote,
        }
    }

    /// What the app addresses this volume's root by, and what `Volume::root`
    /// answers.
    pub fn app_root(&self) -> &Path {
        &self.app
    }

    /// The server-side directory this volume is rooted at.
    pub fn remote_root(&self) -> &Path {
        &self.remote
    }

    /// The absolute server-side path for `path`, or `None` when `path` isn't on
    /// this volume.
    ///
    /// Accepts the three root aliases, this volume's own prefixed paths, and
    /// paths relative to the root. ❌ Refuses everything else, including a bare
    /// server-absolute path and another server's prefix.
    ///
    /// # Two ways of guessing that would each send a request somewhere wrong
    ///
    /// - **The prefix and the root are matched by whole COMPONENTS.** A string
    ///   prefix compare strips `/srv/data` off a sibling `/srv/data-1/photos`
    ///   and asks the server for `-1/photos`, which is a legal name.
    /// - **`..` is resolved before the check, never after.**
    ///   `photos/../../etc` is the same escape spelled relatively, and the
    ///   server would resolve it happily.
    pub fn to_remote_path(&self, path: &Path) -> Option<String> {
        // `.`, `/`, and the empty path are how the app (and a backend's own
        // code) spell "this volume's root".
        if path.as_os_str().is_empty() || path == Path::new(".") || path == Path::new("/") {
            return Some(self.remote.to_string_lossy().into_owned());
        }
        let joined = if let Ok(under_prefix) = path.strip_prefix(&self.prefix_path) {
            normalize(&Path::new("/").join(under_prefix))
        } else if path.is_absolute() || carries_a_scheme(path) {
            // A bare server path, or another server's, or another backend's.
            // None of the three is on this volume.
            return None;
        } else {
            normalize(&self.remote.join(path))
        };
        joined
            .starts_with(&self.remote)
            .then(|| joined.to_string_lossy().into_owned())
    }

    /// The app-facing spelling of a server-side path: the exact inverse of
    /// [`to_remote_path`](Self::to_remote_path).
    ///
    /// This is what a backend's `display_path_for` answers, so the listing-cache
    /// patcher spells paths the way the panes hold them.
    ///
    /// ❗ `None` when `remote` isn't under this volume's root, checked the way
    /// `to_remote_path` checks: by whole components, after `..`. The input is a
    /// SERVER's answer here, and a misbehaving one (an `href` above the
    /// collection, `..` for a name) would otherwise mint an app path off this
    /// volume. A caller drops that entry.
    pub fn to_app_path(&self, remote: &str) -> Option<PathBuf> {
        let normalized = normalize_remote_path(Path::new(remote));
        normalized
            .starts_with(&self.remote)
            .then(|| PathBuf::from(format!("{}{}", self.prefix, normalized.to_string_lossy())))
    }
}

/// A server-side directory, normalized and made absolute: `.` and `..` resolved
/// lexically, and a relative path read from `/`.
///
/// ❗ The one spelling [`RemoteRoot::new`] gives its root, so anything compared
/// against a root (a start folder, say) has to come through here too: two
/// normalizations would let one directory compare unequal to itself.
pub fn normalize_remote_path(path: &Path) -> PathBuf {
    normalize(&Path::new("/").join(path))
}

/// Whether `path` opens with a `<scheme>://` that isn't this volume's.
///
/// Checked separately from `is_absolute` because a scheme path is RELATIVE in
/// `Path`'s terms (`sftp://…` starts with a `sftp:` component), so without this
/// another server's path would be joined onto our root and sent to ours.
fn carries_a_scheme(path: &Path) -> bool {
    path.to_string_lossy()
        .split_once("://")
        .is_some_and(|(scheme, _)| !scheme.is_empty() && !scheme.contains('/'))
}

/// Resolves `.` and `..` lexically, with no round trip and no symlink following.
///
/// Lexical on purpose: asking the server would be a round trip per path AND a
/// TOCTOU window, and the question here is "did the caller address something
/// outside this volume", which is about the path they wrote rather than about
/// what it resolves to. `..` at the root is absorbed, matching what a POSIX
/// server does with `/..`.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => out.push(Component::RootDir),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
            // Windows-only, and every backend here is reached over a network
            // from wherever Cmdr runs, so there is no drive letter to carry.
            Component::Prefix(_) => {}
        }
    }
    if out.as_os_str().is_empty() {
        out.push(Component::RootDir);
    }
    out
}

// ── The app-path schemes: the prefixes a remote volume's paths carry ──────

/// The `sftp://<user>@<host>:<port>` prefix every app path on an SFTP volume
/// carries.
///
/// Minted from the same identity tuple as [`sftp_volume_id`], so the crate and
/// the app spell it from one function and a saved server's row, a restored tab,
/// and the live volume all agree. The volume's app root is this plus the remote
/// root (`sftp://test_user_1@testnas.local:22/srv/data`).
///
/// ❗ **The prefix is what makes a remote path self-describing.** The mount table
/// answers the LOCAL root for any absolute path it doesn't recognize, on both
/// platforms, so a scheme-free `/srv/data/x` resolves to the boot disk at every
/// resolver site. [`RemoteRoot`] is the translation, and
/// `commands/volumes.rs::resolve_path_to_volume` is the arm that reads it.
///
/// # Case folding
///
/// The host is lowercased and the username is not, exactly as in
/// [`sftp_volume_id`], so a path and the id it resolves to agree on identity.
pub fn sftp_app_root(host: &str, port: u16, username: &str) -> String {
    remote_app_root(SFTP_SCHEME, host, port, username)
}

/// The `webdav://<user>@<host>:<port>` prefix every app path on a WebDAV volume
/// carries. The SFTP twin, for the same reasons: [`sftp_app_root`].
///
/// ❗ `webdav://` whatever the transport is. `http` and `https` to one host and
/// port are the same server (the port tells the two default listeners apart),
/// which is the same call [`webdav_volume_id`] makes, so the two can't disagree.
pub fn webdav_app_root(host: &str, port: u16, username: &str) -> String {
    remote_app_root(WEBDAV_SCHEME, host, port, username)
}

/// The `s3://<access key id>@<host>:<port>` prefix every app path on an S3
/// account's places carries. The SFTP twin, for the same reasons:
/// [`sftp_app_root`].
///
/// ❗ The prefix is the ACCOUNT, and a place's root hangs under it: `/` for the
/// account root that lists buckets, `/<bucket>` for a bucket place. So one
/// object has one app spelling whichever place a pane reached it through, and a
/// tab restores onto it either way.
pub fn s3_app_root(host: &str, port: u16, access_key_id: &str) -> String {
    remote_app_root(S3_SCHEME, host, port, access_key_id)
}

/// The scheme [`s3_app_root`] mints and [`server_of_path`] reads back.
const S3_SCHEME: &str = "s3";

/// `text` as a person reads it, or a log keeps it: every S3 app path's access key id taken out,
/// wherever in the text it sits (`s3://AKIA…@host:443/photos` → `s3://host:443/photos`).
///
/// ❗ **Display only, ❌ never identity.** The key id stays in the app path because the account
/// IS the endpoint plus the key ([`super::s3_volume_id`], [`server_of_path`]): a shown path
/// doesn't route back to its volume, so a copied path or an MCP reply keeps the key. The key id
/// isn't a secret, but it names the account, and a log or a screenshot travels (#706).
///
/// The account is what [`s3_app_root`] writes between `s3://` and the first `@`, ending at a `/`
/// or anything a key id can't hold (a quote, a space, a log marker). The AWS CLI's
/// `s3://bucket/key` has none, and other schemes' accounts aren't touched. Text without one
/// comes back borrowed.
pub fn without_s3_key_ids(text: &str) -> Cow<'_, str> {
    const OPENER: &str = "s3://";
    if !text.contains(OPENER) {
        return Cow::Borrowed(text);
    }
    let mut out: Option<String> = None;
    let mut copied = 0;
    for (at, _) in text.match_indices(OPENER) {
        let starts_a_word = text[..at]
            .chars()
            .next_back()
            .is_none_or(|before| !before.is_alphanumeric() && !"+-.".contains(before));
        let account_start = at + OPENER.len();
        let after = &text[account_start..];
        let account_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || "-_.~+=".contains(c)))
            .unwrap_or(after.len());
        if !starts_a_word || account_len == 0 || !after[account_len..].starts_with('@') {
            continue;
        }
        let out = out.get_or_insert_with(|| String::with_capacity(text.len()));
        out.push_str(&text[copied..account_start]);
        copied = account_start + account_len + 1;
    }
    match out {
        Some(mut out) => {
            out.push_str(&text[copied..]);
            Cow::Owned(out)
        }
        None => Cow::Borrowed(text),
    }
}

/// The `nfs://<host>:<port>/<export segment>` prefix every app path on an NFS
/// export carries, and the place's root: the tree under it is the EXPORT's, so a
/// volume's [`RemoteRoot`] speaks export-relative paths. The SFTP twin otherwise:
/// [`sftp_app_root`].
///
/// ❗ **No account in the authority.** The uid and gid a place acts as are a
/// setting, ❌ not its identity ([`nfs_volume_id`]), so the path doesn't carry
/// them either.
///
/// ❗ **The export rides as ONE segment**, its leading `/` dropped and only `%`
/// and `/` escaped (`%25`, `%2F`): `/Public` is `nfs://testnas.local:2049/Public`,
/// `/volume1/data` is `nfs://testnas.local:2049/volume1%2Fdata`, and the root export
/// `/` is the segment `%2F`. Without it, [`server_of_path`] couldn't tell where a
/// multi-segment export ends and the folder under it begins, and it mints the id
/// from the path alone. `server-path-utils.ts` mirrors this exactly.
///
/// The host is folded the way the id folds it; the export is canonicalized
/// ([`canonical_nfs_export`]) and otherwise kept by bytes.
pub fn nfs_app_root(host: &str, port: u16, export: &str) -> String {
    let host = host.to_lowercase();
    format!("{NFS_SCHEME}://{host}:{port}/{}", nfs_export_segment(export))
}

/// The scheme [`nfs_app_root`] mints and [`server_of_path`] reads back.
const NFS_SCHEME: &str = "nfs";

/// The one path segment an export rides as in [`nfs_app_root`].
fn nfs_export_segment(export: &str) -> String {
    let export = canonical_nfs_export(export);
    if export == "/" {
        return "%2F".to_string();
    }
    let mut out = String::with_capacity(export.len());
    for ch in export[1..].chars() {
        match ch {
            '%' => out.push_str("%25"),
            '/' => out.push_str("%2F"),
            other => out.push(other),
        }
    }
    out
}

/// The export one [`nfs_export_segment`] spells, or `None` for a segment no
/// export mints: empty, or carrying an escape other than `%25` / `%2F`.
fn nfs_export_of_segment(segment: &str) -> Option<String> {
    if segment.is_empty() {
        return None;
    }
    if segment == "%2F" {
        return Some("/".to_string());
    }
    let mut export = String::with_capacity(segment.len() + 1);
    export.push('/');
    let mut rest = segment;
    while let Some(at) = rest.find('%') {
        export.push_str(&rest[..at]);
        let escape = rest.get(at..at + 3)?;
        export.push(match escape {
            "%25" => '%',
            "%2F" => '/',
            _ => return None,
        });
        rest = &rest[at + 3..];
    }
    export.push_str(rest);
    Some(export)
}

/// The scheme [`sftp_app_root`] mints and [`server_of_path`] reads back.
const SFTP_SCHEME: &str = "sftp";

/// The scheme [`webdav_app_root`] mints and [`server_of_path`] reads back.
const WEBDAV_SCHEME: &str = "webdav";

/// `{scheme}://{username}@{host}:{port}`, with the host folded the way the volume
/// id folds it.
fn remote_app_root(scheme: &str, host: &str, port: u16, username: &str) -> String {
    let host = host.to_lowercase();
    format!("{scheme}://{username}@{host}:{port}")
}

/// The server account an app path names, as [`server_of_path`] reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPath {
    /// The backend its scheme names: [`BackendKind::Sftp`](super::BackendKind::Sftp),
    /// [`BackendKind::Webdav`](super::BackendKind::Webdav),
    /// [`BackendKind::S3`](super::BackendKind::S3), or
    /// [`BackendKind::Nfs`](super::BackendKind::Nfs).
    pub kind: super::BackendKind,
    /// The id that account mints ([`sftp_volume_id`], [`webdav_volume_id`],
    /// [`s3_volume_id`](super::s3_volume_id) for the bucket the path is in, or
    /// [`nfs_volume_id`] for the export).
    pub volume_id: String,
}

/// The server account an `sftp://`, `webdav://`, or
/// `s3://<user>@<host>:<port>[/…]` app path names, or the export an
/// `nfs://<host>:<port>/<export segment>[/…]` one names ([`nfs_app_root`]), or
/// `None` for any other path, and for one missing its user (NFS: its export),
/// host, or port.
///
/// The one split of what [`remote_app_root`] joins, kept beside it so the two
/// can't drift. Pure for the reason [`adb_serial_of_path`] is: the path IS the
/// identity its id is minted from, so a saved server nobody has connected answers
/// the same as a live one, and the kind rides along so a caller can ask
/// [`BackendKind::can_be_indexed`](super::BackendKind::can_be_indexed) without a
/// registry.
///
/// The user is everything before the authority's LAST `@` and the port everything
/// after its last `:`, which is how an IPv6 host (`sftp://test_user_1@::1:22`) still
/// splits.
pub fn server_of_path(path: &str) -> Option<ServerPath> {
    if let Some(rest) = path.strip_prefix(NFS_SCHEME).and_then(|rest| rest.strip_prefix("://")) {
        return nfs_server_of_path(rest);
    }
    let schemes = [
        (SFTP_SCHEME, super::BackendKind::Sftp),
        (WEBDAV_SCHEME, super::BackendKind::Webdav),
        (S3_SCHEME, super::BackendKind::S3),
    ];
    let (rest, kind) = schemes.into_iter().find_map(|(scheme, kind)| {
        let rest = path.strip_prefix(scheme)?.strip_prefix("://")?;
        Some((rest, kind))
    })?;
    let mut segments = rest.split('/');
    let authority = segments.next()?;
    let (username, host_port) = authority.rsplit_once('@')?;
    let (host, port) = host_port.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    if username.is_empty() || host.is_empty() {
        return None;
    }
    let volume_id = match kind {
        // ❗ An S3 account has a place per bucket, and the first segment names
        // it; none is the account root. A pane on the account root that browsed
        // into a bucket holds the root's id while this names the bucket's; both
        // are S3, which is what every caller here asks about.
        super::BackendKind::S3 => {
            let bucket = segments.next().filter(|bucket| !bucket.is_empty());
            super::s3_volume_id(host, port, username, bucket)
        }
        super::BackendKind::Webdav => webdav_volume_id(host, port, username),
        _ => sftp_volume_id(host, port, username),
    };
    Some(ServerPath { kind, volume_id })
}

/// [`server_of_path`]'s NFS half, for what follows `nfs://`.
fn nfs_server_of_path(rest: &str) -> Option<ServerPath> {
    let mut segments = rest.split('/');
    let (host, port) = segments.next()?.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    let export = nfs_export_of_segment(segments.next()?)?;
    if host.is_empty() {
        return None;
    }
    Some(ServerPath {
        kind: super::BackendKind::Nfs,
        volume_id: nfs_volume_id(host, port, &export),
    })
}

/// The `adb://<serial>` prefix every app path on an ADB volume carries, and the
/// volume's root.
///
/// Minted from the same serial as [`adb_volume_id`](super::adb_volume_id), for
/// the reason [`sftp_app_root`] is: the crate, the device provider's row, and a
/// restored tab all spell it from one function. The device's own tree hangs
/// under it (`adb://R58M1/sdcard/DCIM`), and [`RemoteRoot`] is the translation.
///
/// ❗ The serial goes in VERBATIM, case and all. The id's slug is folded for
/// readability, but the id's digest and the ADB server both key on the exact
/// serial, so a folded prefix would name a device the server doesn't list.
pub fn adb_app_root(serial: &str) -> String {
    format!("{ADB_PATH_SCHEME}{serial}")
}

/// The scheme every [`adb_app_root`] starts with.
const ADB_PATH_SCHEME: &str = "adb://";

/// The serial an `adb://<serial>[/…]` app path names, verbatim, or `None` for
/// any other path.
///
/// The one split of what [`adb_app_root`] joins, kept beside it so the two can't
/// drift: the device provider asks it which phone a path is on, and the index
/// asks it which phone's index a path belongs to. It reads the serial only; the
/// device path under it is the volume's own translation ([`RemoteRoot`]).
pub fn adb_serial_of_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix(ADB_PATH_SCHEME)?;
    let serial = rest.split('/').next()?;
    (!serial.is_empty()).then_some(serial)
}

/// Who said no when a backend refuses a path: the question a refusal's advice
/// turns on, since the fix is in a different place for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteRefuser {
    /// A phone (ADB, MTP): Android keeps the path for the system and its apps.
    Phone,
    /// A connected server (SFTP, WebDAV, NFS): the path's permissions, or the
    /// account or identity it was connected with.
    Server,
    /// An object store account (S3): its keys, or a provider that paused it.
    ObjectStore,
}

/// Who refuses for an app path, or `None` for a path this host's own OS answers
/// for (a local path, or a mounted share, which only `statfs` can tell apart).
///
/// Reads the path's scheme and nothing else, so it's pure, and ❌ a refusal is never
/// classified from its message.
pub fn refuser_of_path(path: &str) -> Option<RemoteRefuser> {
    if adb_serial_of_path(path).is_some() || path.strip_prefix(MTP_PATH_SCHEME).is_some_and(|rest| !rest.is_empty()) {
        return Some(RemoteRefuser::Phone);
    }
    // `server_of_path` names only sftp, webdav, s3, and nfs.
    server_of_path(path).map(|server| match server.kind {
        super::BackendKind::S3 => RemoteRefuser::ObjectStore,
        _ => RemoteRefuser::Server,
    })
}

/// The scheme every MTP app path starts with (`mtp://{device_id}/{storage_id}[/…]`).
const MTP_PATH_SCHEME: &str = "mtp://";

#[cfg(test)]
#[path = "remote_paths_test.rs"]
mod remote_paths_test;
