//! What `propose_server` accepts, checked against what's saved, before anything is written.
//!
//! Pure over the call and a [`SavedServers`] snapshot. The order: no key that could carry a
//! secret, no key outside the protocol's field map, every field one plain value, then the
//! saved-server checks (an edit can't move a server, an add can't be one already saved, and no
//! name can be another saved server's). The field map and why: `DETAILS.md` § Proposing a server.

use cmdr_fs::validation::{PlainLeafRefusal, validate_plain_leaf};
use serde::Deserialize;
use serde_json::Value;

use super::problem::NameRule;
use crate::agent::store::proposals::{ServerDraft, ServerEdit};
use crate::agent::suggested_ops::server::{AddClash, SFTP_PORT, SMB_PORT, SavedServers};
use crate::agent::types::DraftProtocol;
use crate::network::saved_server_fields;

/// The remote root the add form saves when its folder field stays empty (`normalizeRoot`).
const DEFAULT_ROOT: &str = "/";

/// A DNS name's limit.
const MAX_HOST_BYTES: usize = 253;
const MAX_URL_BYTES: usize = 2_048;
const MAX_PATH_BYTES: usize = 1_024;
const MAX_ID_BYTES: usize = 512;
const MAX_RATIONALE_CHARS: usize = 1_000;

/// One field a server proposal may name, by its wire key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Name,
    Host,
    Port,
    Username,
    Share,
    Path,
    Url,
    Export,
    Bucket,
    Account,
    StartFolder,
    Uid,
    Gid,
}

impl Field {
    const ALL: [Field; 13] = [
        Field::Name,
        Field::Host,
        Field::Port,
        Field::Username,
        Field::Share,
        Field::Path,
        Field::Url,
        Field::Export,
        Field::Bucket,
        Field::Account,
        Field::StartFolder,
        Field::Uid,
        Field::Gid,
    ];

    pub(super) fn key(self) -> &'static str {
        match self {
            Field::Name => "name",
            Field::Host => "host",
            Field::Port => "port",
            Field::Username => "username",
            Field::Share => "share",
            Field::Path => "path",
            Field::Url => "url",
            Field::Export => "export",
            Field::Bucket => "bucket",
            Field::Account => "account",
            Field::StartFolder => "startFolder",
            Field::Uid => "uid",
            Field::Gid => "gid",
        }
    }
}

/// The keys every call may carry beside the fields.
const CONTROL_KEYS: [&str; 5] = ["sweepId", "groupId", "editing", "rationale", "protocol"];

/// Keys that would carry a secret, or something that unlocks one, compared lowercased. Refused
/// by name rather than as an unknown key, so the model learns the rule rather than a spelling.
const SECRET_KEYS: [&str; 8] = [
    "password",
    "secret",
    "secretaccesskey",
    "keyfile",
    "privatekey",
    "passphrase",
    "token",
    "apikey",
];

/// What one protocol takes, on an add and on an edit (`DETAILS.md` § Proposing a server).
struct FieldMap {
    add: &'static [Field],
    required: &'static [Field],
    edit: &'static [Field],
    /// Where the server is, and who it signs in as (a username, NFS's uid and gid): an edit
    /// may only restate it, on every protocol.
    identity: &'static [Field],
}

fn field_map(protocol: DraftProtocol) -> FieldMap {
    use Field::*;
    match protocol {
        DraftProtocol::Smb => FieldMap {
            add: &[Host, Port, Username, Share, Name],
            required: &[Host],
            edit: &[Name],
            identity: &[Host, Port, Username],
        },
        DraftProtocol::Sftp => FieldMap {
            add: &[Host, Port, Username, Path, StartFolder, Name],
            required: &[Host, Username],
            edit: &[Name, Path, StartFolder],
            identity: &[Host, Port, Username],
        },
        DraftProtocol::Webdav => FieldMap {
            add: &[Url, Username, Path, StartFolder, Name],
            required: &[Url, Username],
            edit: &[Name, Path, StartFolder],
            identity: &[Url, Username],
        },
        DraftProtocol::Nfs => FieldMap {
            add: &[Host, Port, Export, Uid, Gid, StartFolder, Name],
            required: &[Host, Export],
            edit: &[Name, StartFolder],
            identity: &[Host, Port, Export, Uid, Gid],
        },
        DraftProtocol::S3 => FieldMap {
            add: &[Account, Bucket, Name],
            required: &[Account, Bucket],
            edit: &[Name],
            identity: &[Account, Bucket],
        },
    }
}

/// Why a field's value won't do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldRule {
    /// One of [`validate_plain_leaf`]'s, on the field or on one folder of a path.
    Plain(PlainLeafRefusal),
    /// A path that doesn't start at `/`.
    NotAbsolute,
    TooLong {
        max: usize,
    },
    /// A host that isn't a plain ASCII host name or IP literal, or port 0.
    NotAnAddress,
    /// A URL that isn't `http` or `https` with a host.
    NotHttp,
    /// A start folder outside the server's remote root.
    OutsideRoot,
}

impl FieldRule {
    /// What's wrong, as the end of a sentence the model reads.
    pub(super) fn reason(self) -> String {
        match self {
            FieldRule::Plain(refusal) => NameRule::Plain(refusal).reason(),
            FieldRule::NotAbsolute => "it must start with /".into(),
            FieldRule::TooLong { max } => format!("it's longer than {max}"),
            FieldRule::NotAnAddress => "it isn't a plain host name or IP address (ASCII only, no port)".into(),
            FieldRule::NotHttp => "it must be an http or https URL with a host".into(),
            FieldRule::OutsideRoot => "it must be the remote root or a folder under it".into(),
        }
    }
}

/// What a server proposal got wrong, typed: the token the model branches on, and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ServerProblem {
    /// Not an object, or a field of the wrong type.
    Malformed,
    /// A turn Cmdr started: a server card nobody asked for is the shape an injection wants.
    RefusedInBackground,
    SecretRefused {
        field: String,
    },
    /// A key outside this protocol's field map for an add or an edit.
    UnboundField {
        field: String,
    },
    MissingField {
        field: &'static str,
    },
    BadField {
        field: &'static str,
        rule: FieldRule,
    },
    /// A protocol, or a shape of one (a new S3 account), only the user adds.
    ProtocolNotProposable,
    /// `editing` (or an S3 add's `account`) names nothing saved.
    UnknownServer {
        id: String,
    },
    /// An edit naming another address, account, or protocol than the saved one.
    AddressChange {
        field: &'static str,
    },
    /// An add of a server that's already saved, with the id to propose an edit of.
    AlreadySaved {
        server_id: String,
    },
    /// A name another saved server already wears, folded.
    NameTaken {
        name: String,
    },
    GroupIdWithoutSweep,
}

/// A checked server proposal: what the group stores, and where.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PlannedServer {
    pub sweep_id: Option<i64>,
    pub group_id: Option<i64>,
    pub rationale: Option<String>,
    pub draft: ServerDraft,
    pub edit: Option<ServerEdit>,
    /// What the card leads with: the name, else the address.
    pub display_name: String,
}

/// The call, typed. Unknown keys are refused before this reads anything.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ask {
    sweep_id: Option<i64>,
    group_id: Option<i64>,
    editing: Option<String>,
    rationale: Option<String>,
    protocol: String,
    name: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    share: Option<String>,
    path: Option<String>,
    url: Option<String>,
    export: Option<String>,
    bucket: Option<String>,
    account: Option<String>,
    start_folder: Option<String>,
    uid: Option<u32>,
    gid: Option<u32>,
}

impl Ask {
    fn has(&self, field: Field) -> bool {
        match field {
            Field::Name => self.name.is_some(),
            Field::Host => self.host.is_some(),
            Field::Port => self.port.is_some(),
            Field::Username => self.username.is_some(),
            Field::Share => self.share.is_some(),
            Field::Path => self.path.is_some(),
            Field::Url => self.url.is_some(),
            Field::Export => self.export.is_some(),
            Field::Bucket => self.bucket.is_some(),
            Field::Account => self.account.is_some(),
            Field::StartFolder => self.start_folder.is_some(),
            Field::Uid => self.uid.is_some(),
            Field::Gid => self.gid.is_some(),
        }
    }
}

/// Check a `propose_server` call against what's saved.
pub(super) fn plan_server(params: &Value, saved: &SavedServers) -> Result<PlannedServer, ServerProblem> {
    let object = params.as_object().ok_or(ServerProblem::Malformed)?;
    if let Some(key) = object
        .keys()
        .find(|key| SECRET_KEYS.contains(&key.to_lowercase().as_str()))
    {
        return Err(ServerProblem::SecretRefused { field: key.clone() });
    }
    if let Some(key) = object
        .keys()
        .find(|key| !CONTROL_KEYS.contains(&key.as_str()) && !Field::ALL.iter().any(|f| f.key() == key.as_str()))
    {
        return Err(ServerProblem::UnboundField { field: key.clone() });
    }
    let ask: Ask = serde_json::from_value(params.clone()).map_err(|_| ServerProblem::Malformed)?;
    let protocol = DraftProtocol::from_token(&ask.protocol).ok_or(ServerProblem::ProtocolNotProposable)?;
    if ask.group_id.is_some() && ask.sweep_id.is_none() {
        return Err(ServerProblem::GroupIdWithoutSweep);
    }
    check_shape(&ask, protocol)?;
    check_values(&ask)?;

    let (draft, edit, own_row) = match &ask.editing {
        Some(editing) => plan_edit(&ask, protocol, editing, saved)?,
        None => plan_add(&ask, protocol, saved)?,
    };
    if let Some(name) = &ask.name {
        check_name_free(name, own_row.as_deref(), saved)?;
    }
    // An edit that keeps its name leads with what the servers list calls it now.
    let listed_name = edit
        .as_ref()
        .and(own_row.as_deref())
        .and_then(|id| saved.row(id))
        .map(|row| row.name.clone());
    let display_name = draft
        .display_name
        .clone()
        .or(listed_name)
        .unwrap_or_else(|| address_of(&draft));
    Ok(PlannedServer {
        sweep_id: ask.sweep_id,
        group_id: ask.group_id,
        rationale: ask.rationale,
        draft,
        edit,
        display_name,
    })
}

/// Every field the call names is one this protocol takes for an add (or an edit), and an add
/// names every field it needs.
fn check_shape(ask: &Ask, protocol: DraftProtocol) -> Result<(), ServerProblem> {
    let map = field_map(protocol);
    let allowed = |field: &Field| match ask.editing {
        Some(_) => map.edit.contains(field) || map.identity.contains(field),
        None => map.add.contains(field),
    };
    if let Some(field) = Field::ALL.iter().find(|field| ask.has(**field) && !allowed(field)) {
        return Err(ServerProblem::UnboundField {
            field: field.key().to_string(),
        });
    }
    if ask.editing.is_none() {
        if protocol == DraftProtocol::S3 && !ask.has(Field::Account) {
            // A bucket under no saved account would be a new account, which only the user adds.
            return Err(ServerProblem::ProtocolNotProposable);
        }
        if let Some(field) = map.required.iter().find(|field| !ask.has(**field)) {
            return Err(ServerProblem::MissingField { field: field.key() });
        }
    }
    Ok(())
}

/// Every value one plain, visible thing: a name a reviewer reads as exactly what lands, an
/// address that is only an address, a path made of plain folder names.
fn check_values(ask: &Ask) -> Result<(), ServerProblem> {
    let bad = |field: Field, rule: FieldRule| ServerProblem::BadField {
        field: field.key(),
        rule,
    };
    for (field, value) in [
        (Field::Name, &ask.name),
        (Field::Share, &ask.share),
        (Field::Username, &ask.username),
        (Field::Bucket, &ask.bucket),
    ] {
        if let Some(value) = value {
            validate_plain_leaf(value).map_err(|refusal| bad(field, FieldRule::Plain(refusal)))?;
        }
    }
    for (field, value) in [
        (Field::Path, &ask.path),
        (Field::Export, &ask.export),
        (Field::StartFolder, &ask.start_folder),
    ] {
        if let Some(value) = value {
            check_path(value).map_err(|rule| bad(field, rule))?;
        }
    }
    if let Some(host) = &ask.host {
        check_host(host).map_err(|rule| bad(Field::Host, rule))?;
    }
    if ask.port == Some(0) {
        return Err(bad(Field::Port, FieldRule::NotAnAddress));
    }
    if let Some(url) = &ask.url {
        check_url(url)?;
    }
    for (key, value) in [("editing", &ask.editing), ("account", &ask.account)] {
        if value.as_ref().is_some_and(|id| id.len() > MAX_ID_BYTES) {
            return Err(ServerProblem::BadField {
                field: key,
                rule: FieldRule::TooLong { max: MAX_ID_BYTES },
            });
        }
    }
    if ask
        .rationale
        .as_ref()
        .is_some_and(|text| text.chars().count() > MAX_RATIONALE_CHARS)
    {
        return Err(ServerProblem::BadField {
            field: "rationale",
            rule: FieldRule::TooLong {
                max: MAX_RATIONALE_CHARS,
            },
        });
    }
    Ok(())
}

/// An absolute path whose every folder is a plain leaf.
fn check_path(path: &str) -> Result<(), FieldRule> {
    if path.len() > MAX_PATH_BYTES {
        return Err(FieldRule::TooLong { max: MAX_PATH_BYTES });
    }
    if !path.starts_with('/') {
        return Err(FieldRule::NotAbsolute);
    }
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .try_for_each(|segment| validate_plain_leaf(segment).map_err(FieldRule::Plain))
}

/// A plain ASCII host name or IP literal. ASCII only, so a look-alike of a saved host
/// (`nаspolya` with a Cyrillic `а`) can't stand in for it; an internationalized name goes in
/// its `xn--` form.
fn check_host(host: &str) -> Result<(), FieldRule> {
    if host.len() > MAX_HOST_BYTES {
        return Err(FieldRule::TooLong { max: MAX_HOST_BYTES });
    }
    let plain = !host.is_empty()
        && host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | ':' | '[' | ']'));
    if plain && url::Host::parse(host).is_ok() {
        Ok(())
    } else {
        Err(FieldRule::NotAnAddress)
    }
}

/// An `http` or `https` URL with a host, carrying no credential.
fn check_url(url: &str) -> Result<(), ServerProblem> {
    let bad = |rule| ServerProblem::BadField {
        field: Field::Url.key(),
        rule,
    };
    if url.len() > MAX_URL_BYTES {
        return Err(bad(FieldRule::TooLong { max: MAX_URL_BYTES }));
    }
    let parsed = url::Url::parse(url.trim()).map_err(|_| bad(FieldRule::NotHttp))?;
    if parsed.password().is_some() {
        return Err(ServerProblem::SecretRefused {
            field: Field::Url.key().to_string(),
        });
    }
    // ASCII for the same reason a host is: the URL parser would quietly turn a look-alike
    // host into its `xn--` form, and the card would show the look-alike.
    let plain = url.is_ascii() && url.trim() == url && !url.chars().any(char::is_control);
    if !plain
        || !matches!(parsed.scheme(), "http" | "https")
        || parsed.host().is_none()
        || !parsed.username().is_empty()
    {
        return Err(bad(FieldRule::NotHttp));
    }
    Ok(())
}

/// A new server: the add form's defaults for what the call leaves out, refused when it's saved
/// already. Answers the draft and the row it would join (an S3 account, an NFS server).
fn plan_add(
    ask: &Ask,
    protocol: DraftProtocol,
    saved: &SavedServers,
) -> Result<(ServerDraft, Option<ServerEdit>, Option<String>), ServerProblem> {
    let mut draft = ServerDraft {
        display_name: ask.name.clone(),
        host: ask.host.clone(),
        port: ask.port,
        username: ask.username.clone(),
        share: ask.share.clone(),
        remote_root: ask.path.clone(),
        url: ask.url.as_ref().map(|url| url.trim().to_string()),
        bucket: ask.bucket.clone(),
        account_id: ask.account.clone(),
        export: ask.export.clone(),
        uid: ask.uid,
        gid: ask.gid,
        start_folder: ask.start_folder.clone(),
        ..ServerDraft::new(protocol)
    };
    match protocol {
        DraftProtocol::Smb => draft.port = Some(draft.port.unwrap_or(SMB_PORT)),
        DraftProtocol::Sftp => {
            draft.port = Some(draft.port.unwrap_or(SFTP_PORT));
            draft.remote_root = Some(draft.remote_root.unwrap_or_else(|| DEFAULT_ROOT.to_string()));
        }
        DraftProtocol::Webdav => {
            draft.remote_root = Some(draft.remote_root.unwrap_or_else(|| DEFAULT_ROOT.to_string()));
        }
        DraftProtocol::Nfs => {
            // The add form's own fallback when it can't read who owns the export, and the only
            // one that asks the server nothing: this Mac's ids, on the card for the user to see.
            let this_mac = crate::saved_servers::nfs_places::this_mac_identity();
            draft.uid = Some(draft.uid.unwrap_or(this_mac.uid));
            draft.gid = Some(draft.gid.unwrap_or(this_mac.gid));
        }
        DraftProtocol::S3 => {}
    }
    check_start_folder(&draft)?;
    let placed = saved.place_add(&draft).map_err(|clash| match clash {
        AddClash::UnknownAccount { id } => ServerProblem::UnknownServer { id },
        AddClash::AlreadySaved { server_id } => ServerProblem::AlreadySaved { server_id },
        AddClash::Unkeyable => ServerProblem::BadField {
            field: Field::Url.key(),
            rule: FieldRule::NotHttp,
        },
    })?;
    Ok((draft, None, placed.own_row))
}

/// A change to a saved place: its address and who it signs in as stay exactly as saved, and
/// only the map's editable fields move. Answers the draft, the edit (with `previous`), and the row.
fn plan_edit(
    ask: &Ask,
    protocol: DraftProtocol,
    editing: &str,
    saved: &SavedServers,
) -> Result<(ServerDraft, Option<ServerEdit>, Option<String>), ServerProblem> {
    let (row, place) = saved.place(editing).ok_or_else(|| ServerProblem::UnknownServer {
        id: editing.to_string(),
    })?;
    if row.protocol != protocol {
        return Err(ServerProblem::AddressChange { field: "protocol" });
    }
    let previous = place.draft.clone();
    if let Some(field) = field_map(protocol)
        .identity
        .iter()
        .find(|field| ask.has(**field) && !restates(ask, **field, &previous))
    {
        return Err(ServerProblem::AddressChange { field: field.key() });
    }
    let mut draft = previous.clone();
    if ask.name.is_some() {
        draft.display_name = ask.name.clone();
    }
    if ask.path.is_some() {
        draft.remote_root = ask.path.clone();
    }
    if ask.start_folder.is_some() {
        draft.start_folder = ask.start_folder.clone();
    }
    check_start_folder(&draft)?;
    let edit = ServerEdit {
        editing: editing.to_string(),
        previous,
    };
    Ok((draft, Some(edit), Some(row.id.clone())))
}

/// Whether the call's value for an identity field is the saved one, under the store's own
/// reading (a host's case, a URL's trailing slash, an export's spelling).
fn restates(ask: &Ask, field: Field, previous: &ServerDraft) -> bool {
    match field {
        Field::Host => same_host(ask.host.as_deref(), previous.host.as_deref()),
        Field::Port => ask.port == previous.port,
        Field::Username => ask.username == previous.username,
        Field::Url => {
            url_identity(ask.url.as_deref()).is_some_and(|url| Some(url) == url_identity(previous.url.as_deref()))
        }
        Field::Export => {
            ask.export.as_deref().map(cmdr_fs::volume::canonical_nfs_export)
                == previous.export.as_deref().map(cmdr_fs::volume::canonical_nfs_export)
        }
        Field::Account => ask.account == previous.account_id,
        Field::Bucket => ask.bucket.as_deref().map(str::trim) == previous.bucket.as_deref().map(str::trim),
        Field::Uid => ask.uid == previous.uid,
        Field::Gid => ask.gid == previous.gid,
        _ => true,
    }
}

fn same_host(asked: Option<&str>, saved: Option<&str>) -> bool {
    matches!((asked, saved), (Some(a), Some(b)) if a.trim().eq_ignore_ascii_case(b.trim()))
}

/// A URL as a place: scheme, host, port, and path without its trailing `/`.
fn url_identity(url: Option<&str>) -> Option<(String, String, u16, String)> {
    let parsed = url::Url::parse(url?.trim()).ok()?;
    Some((
        parsed.scheme().to_string(),
        parsed.host_str()?.to_lowercase(),
        parsed.port_or_known_default()?,
        parsed.path().trim_end_matches('/').to_string(),
    ))
}

/// SFTP and WebDAV refuse a start folder outside the remote root at save; refuse it here first.
fn check_start_folder(draft: &ServerDraft) -> Result<(), ServerProblem> {
    if !matches!(draft.protocol, DraftProtocol::Sftp | DraftProtocol::Webdav) {
        return Ok(());
    }
    let root = draft.remote_root.as_deref().unwrap_or(DEFAULT_ROOT);
    saved_server_fields::start_folder_under_root(root, draft.start_folder.as_deref())
        .map(|_| ())
        .map_err(|_| ServerProblem::BadField {
            field: Field::StartFolder.key(),
            rule: FieldRule::OutsideRoot,
        })
}

/// No other saved server wears this name (`SavedServers::name_holder`).
fn check_name_free(name: &str, own_row: Option<&str>, saved: &SavedServers) -> Result<(), ServerProblem> {
    match saved.name_holder(name, own_row) {
        Some(row) => Err(ServerProblem::NameTaken { name: row.name.clone() }),
        None => Ok(()),
    }
}

/// What a card with no name leads with.
fn address_of(draft: &ServerDraft) -> String {
    let address = match draft.protocol {
        DraftProtocol::Webdav => draft.url.clone(),
        DraftProtocol::S3 => draft.bucket.clone(),
        DraftProtocol::Smb | DraftProtocol::Sftp | DraftProtocol::Nfs => draft.host.clone(),
    };
    address.unwrap_or_default()
}
