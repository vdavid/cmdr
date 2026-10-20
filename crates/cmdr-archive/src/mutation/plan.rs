//! Planning an edit: which original entries a [`Changeset`] keeps, and under
//! what name. Pure bookkeeping over the source archive's central directory; the
//! temp+rename write that follows lives in [`super::mutator`].

use cmdr_fs::log_private::LogPrivate;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;
use zip::read::ZipFile;

use super::changeset::{Changeset, MutationError};
use crate::apple_double;

/// One original entry kept in the rewrite, with its post-edit name.
pub(super) struct RetainedEntry {
    /// The entry's index in the source `ZipArchive`.
    pub(super) index: usize,
    /// `None` keeps the stored name bytes as they are.
    pub(super) new_name: Option<String>,
    pub(super) compressed_size: u64,
}

/// A kept entry's post-edit name, for [`refuse_an_entry_under_a_file`].
struct KeptName {
    name: String,
    is_dir: bool,
    /// Whether the edit renamed it, which makes it an entry the edit writes.
    renamed: bool,
}

/// What an edit keeps of the original archive.
pub(super) struct RetainedPlan {
    /// How many bytes precede the zip data (a self-extractor stub), copied to
    /// the start of the new archive verbatim. 0 for a plain zip.
    pub(super) prefix_len: u64,
    pub(super) entries: Vec<RetainedEntry>,
    /// The prefix and the kept entries' compressed bytes, for the progress total.
    pub(super) bytes: u64,
    /// Original items (sidecars aside) the edit drops or renames, for the
    /// user-facing affected-item tally (`entries_changed`), NOT the
    /// retained-rewrite total.
    pub(super) deleted_count: usize,
    pub(super) renamed_count: usize,
}

/// Plans the retained set: for each original entry, its post-edit name (dropped
/// by a delete, prefix-rewritten by a rename, or unchanged). One raw lookup per
/// entry reads the name + compressed size together.
///
/// Deletes and renames name entries as the reader shows them, which for a name
/// stored without the UTF-8 flag (Shift-JIS, GBK, CP866) isn't what the `zip`
/// crate decodes. See [`planned_name`].
pub(super) fn plan_retained(
    src: &mut ZipArchive<File>,
    archive_path: &Path,
    changeset: &Changeset,
) -> Result<RetainedPlan, MutationError> {
    // Every delete and rename takes the item's AppleDouble sidecar along, and
    // every add drops a stale one at its name (§ `with_sidecar_deletes`).
    let deletes = with_sidecar_deletes(changeset);
    let renames = with_sidecar_renames(&changeset.renames);
    let shown = shown_names(archive_path);
    refuse_a_folded_duplicate(src, &shown)?;
    let mut shown_names = ByHeaderOffset::new(shown);
    let mut tree_paths = changeset
        .deletes_spare_hidden
        .then(|| shown_tree_paths(archive_path).map(ByHeaderOffset::new))
        .transpose()?;

    let prefix_len = leading_bytes_len(src);
    let mut plan = RetainedPlan {
        prefix_len,
        entries: Vec::new(),
        bytes: prefix_len,
        deleted_count: 0,
        renamed_count: 0,
    };
    let mut kept: Vec<KeptName> = Vec::with_capacity(src.len());
    for index in 0..src.len() {
        let file = src.by_index_raw(index).map_err(MutationError::Zip)?;
        let header_start = file.header_start();
        let original_name = planned_name(&file, shown_names.take(header_start))?;
        let tree_path = tree_paths.as_mut().map(|paths| paths.take(header_start).flatten());
        let compressed_size = file.compressed_size();
        let uncarriable = !carries_its_encryption(&file);
        // A folder by its name's trailing slash or by its recorded mode (a
        // Unix `S_IFDIR` or the DOS directory bit), as the reader decides.
        let is_dir = file.is_dir() || file.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o040000);
        drop(file);
        // A sidecar is metadata, never an item the user counts.
        let is_item = !apple_double::is_in_macosx_folder(&original_name);
        let deleted = match &tree_path {
            // An item matches by where the tree shows it, so one the tree hides
            // never goes. A sidecar isn't in the tree; it goes with its item.
            Some(tree_path) if is_item => tree_path.as_deref().is_some_and(|path| is_deleted(path, &deletes)),
            _ => is_deleted(&original_name, &deletes),
        };
        match (!deleted).then(|| renamed_name(&original_name, &renames)) {
            Some(new_name) => {
                // Refuse before creating any temp: keeping an encrypted entry whose
                // scheme the raw copy can't carry could leave it undecryptable.
                // Bailing here keeps the original untouched.
                if uncarriable {
                    return Err(MutationError::EncryptedEntryRetained { name: original_name });
                }
                let renamed = new_name != original_name;
                if renamed && is_item {
                    plan.renamed_count += 1;
                }
                kept.push(KeptName {
                    name: new_name.clone(),
                    is_dir,
                    renamed,
                });
                plan.entries.push(RetainedEntry {
                    index,
                    new_name: renamed.then_some(new_name),
                    compressed_size,
                });
                plan.bytes += compressed_size;
            }
            None if is_item => plan.deleted_count += 1,
            None => {}
        }
    }
    refuse_an_entry_under_a_file(&kept, changeset)?;
    Ok(plan)
}

/// General-purpose bit 6: PKWARE Strong Encryption.
const STRONG_ENCRYPTION_FLAG: u16 = 1 << 6;
/// General-purpose bit 13: the local header's values are masked (an encrypted
/// central directory).
const MASKED_HEADER_FLAG: u16 = 1 << 13;

/// Whether a raw copy keeps `file` decryptable with its password: always for
/// an unencrypted entry; for WinZip AES, whose framing (salt, verifier, MAC)
/// sits in the stored bytes and whose extra field the copy rewrites as read;
/// for ZipCrypto, whose 12-byte header is in the stored bytes and whose flags,
/// CRC, and DOS time the copy keeps (the vendored `zip` patch), except under
/// bit 3 with a DOS time the `zip` crate couldn't parse: the password's check
/// byte is that time's, and the crate keeps no raw copy of it to write back.
/// PKWARE Strong Encryption and masked headers carry state the `zip` crate
/// doesn't model, so nothing vouches for a copy of those.
fn carries_its_encryption<R: Read>(file: &ZipFile<'_, R>) -> bool {
    use zip::read::HasZipMetadata as _;
    if !file.encrypted() {
        return true;
    }
    let flags = file.flags();
    if flags.as_u16() & (STRONG_ENCRYPTION_FLAG | MASKED_HEADER_FLAG) != 0 {
        return false;
    }
    let aes = file.get_metadata().aes_settings().is_some();
    aes || !flags.is_using_data_descriptor() || file.last_modified().is_some()
}

/// Refuses an edit that would write an entry under a FILE entry: an add or
/// mkdir under a file the archive keeps or adds, or a file added where kept
/// entries sit beneath its name. Every reader drops such an entry (a file can't
/// be a folder too), so it would sit in the zip unseen and unextractable, which
/// is how a move once lost the files it "added". The planner above resolves
/// that clash with the user first; this is the backstop for any route that
/// didn't.
///
/// Only entries the edit WRITES (adds, mkdirs, and renamed kept entries) are
/// checked, so a zip that already holds such an entry stays editable, deletes
/// included. Sidecars live in their own tree and never count.
fn refuse_an_entry_under_a_file(kept: &[KeptName], changeset: &Changeset) -> Result<(), MutationError> {
    struct Name<'a> {
        name: &'a str,
        is_dir: bool,
        written: bool,
    }
    let kept = kept.iter().map(|k| Name {
        name: k.name.trim_end_matches('/'),
        is_dir: k.is_dir,
        written: k.renamed,
    });
    let mkdirs = changeset.mkdirs.iter().map(|dir| Name {
        name: dir.trim_matches('/'),
        is_dir: true,
        written: true,
    });
    let adds = changeset.adds.iter().map(|add| Name {
        name: add.inner_path.trim_matches('/'),
        is_dir: false,
        written: true,
    });
    let names: Vec<Name<'_>> = kept
        .chain(mkdirs)
        .chain(adds)
        .filter(|n| !n.name.is_empty() && !apple_double::is_in_macosx_folder(n.name))
        .collect();
    if !names.iter().any(|n| n.written) {
        return Ok(());
    }

    let files: HashSet<&str> = names.iter().filter(|n| !n.is_dir).map(|n| n.name).collect();
    // Each folder path any entry implies, with one entry beneath it.
    let mut beneath: HashMap<&str, &str> = HashMap::new();
    for n in &names {
        for (slash, _) in n.name.match_indices('/') {
            beneath.entry(&n.name[..slash]).or_insert(n.name);
        }
    }
    let refuse = |entry: &str, file: &str| MutationError::EntryUnderFile {
        entry: entry.to_string(),
        file: file.to_string(),
    };
    for n in names.iter().filter(|n| n.written) {
        if let Some((slash, _)) = n
            .name
            .match_indices('/')
            .find(|(slash, _)| files.contains(&n.name[..*slash]))
        {
            return Err(refuse(n.name, &n.name[..slash]));
        }
        if !n.is_dir
            && let Some(entry) = beneath.get(n.name)
        {
            return Err(refuse(entry, n.name));
        }
    }
    Ok(())
}

/// Every entry's local header offset and name as Cmdr's reader shows it (see
/// [`crate::read::shown_names_with_header_offsets`]). Empty when the reader
/// can't parse the archive, which [`planned_name`] covers.
pub(super) fn shown_names(archive_path: &Path) -> Vec<(u64, String)> {
    let parsed = crate::read::LocalFileSource::open(archive_path)
        .map_err(crate::read::ArchiveError::from)
        .and_then(|source| crate::read::shown_names_with_header_offsets(&source));
    parsed.unwrap_or_else(|err| {
        log::warn!(target: "archive_mutator", "Couldn't read {}'s entry names the way the reader shows them, so the edit matches the zip crate's: {err}", LogPrivate::path(archive_path));
        Vec::new()
    })
}

/// Every entry's local header offset, with its path in the reader's tree when
/// the tree shows it (see [`crate::read::shown_entry_paths`]). Unlike
/// [`shown_names`] this can't degrade to the `zip` crate's view: without the
/// tree there's no telling which entries extracted, so the edit stops, the
/// archive untouched.
fn shown_tree_paths(archive_path: &Path) -> Result<Vec<(u64, Option<String>)>, MutationError> {
    crate::read::LocalFileSource::open(archive_path)
        .map_err(MutationError::OpenOriginal)
        .and_then(|source| {
            crate::read::shown_entry_paths(std::sync::Arc::new(source))
                .map_err(|err| MutationError::OpenOriginal(std::io::Error::other(err)))
        })
}

/// Refuses an archive in which two entries share a name. The `zip` crate keeps
/// one entry per raw name (the later one's data, in the earlier one's slot),
/// and the rewrite copies through it, so the other copy would silently vanish.
/// The `zip` crate's own record count (`central_directory_len`, the vendored
/// patch) beating its entry count is exactly that fold, so the refusal holds
/// even when the reader can't parse the archive. `shown`, the reader's names,
/// only says which name, and is empty in that case.
pub(super) fn refuse_a_folded_duplicate(src: &ZipArchive<File>, shown: &[(u64, String)]) -> Result<(), MutationError> {
    if src.central_directory_len() <= src.len() {
        return Ok(());
    }
    let mut seen = HashSet::new();
    let name = shown
        .iter()
        .map(|(_, name)| name)
        .find(|name| !seen.insert(*name))
        .cloned();
    Err(MutationError::DuplicateEntryName { name })
}

/// How many bytes sit before the zip data: everything ahead of the first local
/// header, or ahead of the central directory in an archive with no entries.
/// A self-extractor's stub lives there, and so does whatever else a tool
/// prepended; the rewrite keeps all of it.
fn leading_bytes_len(src: &ZipArchive<File>) -> u64 {
    (0..src.len())
        .filter_map(|index| src.by_index_data(index).ok().map(|entry| entry.header_start()))
        .min()
        .unwrap_or_else(|| src.central_directory_start())
}

/// The name an edit matches `file` by: `shown`, what the reader shows for it,
/// the name the user picked. The `zip` crate's own reading is the fallback, for
/// an entry the reader didn't list. Planning by name never touches the name
/// bytes: an entry the edit keeps under its name is copied with its stored bytes.
fn planned_name<R: Read>(file: &ZipFile<'_, R>, shown: Option<String>) -> Result<String, MutationError> {
    match shown {
        Some(shown) => Ok(shown),
        None => Ok(file.name().map_err(MutationError::Zip)?.into_owned()),
    }
}

/// The reader's per-entry facts, handed to the `zip` crate's entries by local
/// header offset, the one thing both crates agree on. Entries sharing an offset
/// (a malformed or hostile zip can point two central records at one local
/// header) take theirs in directory order, which both crates keep once
/// [`refuse_a_folded_duplicate`] has ruled out a fold. Keyed by offset alone,
/// both twins took the later one's name, so deleting either took the other too.
struct ByHeaderOffset<T>(HashMap<u64, VecDeque<T>>);

impl<T> ByHeaderOffset<T> {
    fn new(entries: Vec<(u64, T)>) -> Self {
        let mut by_offset: HashMap<u64, VecDeque<T>> = HashMap::new();
        for (offset, value) in entries {
            by_offset.entry(offset).or_default().push_back(value);
        }
        Self(by_offset)
    }

    /// The next fact for the entry at `offset`, or `None` when the reader listed none.
    fn take(&mut self, offset: u64) -> Option<T> {
        self.0.get_mut(&offset).and_then(VecDeque::pop_front)
    }
}

/// The deletes `changeset` asks for, each with its item's sidecars: the item's
/// own (`__MACOSX/<parent>/._<name>`) and, for a folder, every one inside it
/// (`__MACOSX/<item>/…`). Plus the sidecar name of every add and sourced mkdir:
/// what's there is stale (the new item writes its own, or carries nothing), and
/// leaving it would put two entries at one name, which the writer refuses.
fn with_sidecar_deletes(changeset: &Changeset) -> Vec<String> {
    let mut deletes = Vec::with_capacity(changeset.deletes.len() * 3 + changeset.adds.len());
    for delete in &changeset.deletes {
        let item = delete.trim_matches('/');
        deletes.push(item.to_string());
        if !item.is_empty() && !apple_double::is_in_macosx_folder(item) {
            deletes.push(format!("{}/{item}", apple_double::MACOSX_FOLDER));
            deletes.push(apple_double::sidecar_entry_name(item));
        }
    }
    let written = changeset
        .adds
        .iter()
        .map(|add| add.inner_path.as_str())
        .chain(changeset.mkdir_sources.keys().map(String::as_str));
    deletes.extend(written.map(apple_double::sidecar_entry_name));
    deletes
}

/// The renames `changeset` asks for, each followed by its item's sidecars
/// moving with it, the same two shapes [`with_sidecar_deletes`] drops.
fn with_sidecar_renames(renames: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(renames.len() * 3);
    for (from, to) in renames {
        out.push((from.clone(), to.clone()));
        let (from, to) = (from.trim_matches('/'), to.trim_matches('/'));
        if from.is_empty() || apple_double::is_in_macosx_folder(from) {
            continue;
        }
        let folder = apple_double::MACOSX_FOLDER;
        out.push((format!("{folder}/{from}"), format!("{folder}/{to}")));
        out.push((
            apple_double::sidecar_entry_name(from),
            apple_double::sidecar_entry_name(to),
        ));
    }
    out
}

/// Whether a delete drops the entry named `name`: it is one of `deletes` or
/// under one. The match takes a whole path component: `foo` takes `foo` and
/// everything under `foo/`, but never a sibling `foobar`.
fn is_deleted(name: &str, deletes: &[String]) -> bool {
    let logical = name.trim_end_matches('/');
    deletes
        .iter()
        .any(|delete| path_matches_or_under(logical, delete.trim_matches('/')))
}

/// Computes a kept entry's post-edit name, prefix-rewritten by a rename or
/// unchanged. A trailing slash (an explicit directory entry) is preserved.
/// Renames match a whole path component, like [`is_deleted`].
fn renamed_name(original_name: &str, renames: &[(String, String)]) -> String {
    let is_dir = original_name.ends_with('/');
    let logical = original_name.trim_end_matches('/');

    let mut new_logical = logical.to_string();
    for (from, to) in renames {
        let from = from.trim_matches('/');
        let to = to.trim_matches('/');
        if logical == from {
            new_logical = to.to_string();
            break;
        }
        if let Some(rest) = logical.strip_prefix(&format!("{from}/")) {
            new_logical = format!("{to}/{rest}");
            break;
        }
    }

    if is_dir { format!("{new_logical}/") } else { new_logical }
}

/// Whether `path` is `target` itself or a descendant of it (component-wise).
fn path_matches_or_under(path: &str, target: &str) -> bool {
    path == target || path.strip_prefix(target).is_some_and(|rest| rest.starts_with('/'))
}
