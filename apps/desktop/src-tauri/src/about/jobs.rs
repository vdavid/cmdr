//! One job per planned row, for an item on a local disk.
//!
//! Every job is blocking and runs on the about pool. Each reads raw inputs (a
//! file's first and last bytes, an `Info.plist`, an xattr) and hands them to
//! `cmdr-about`, which owns every rule about what they mean.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use cmdr_about::card::{AboutFact, AboutPlan};
use cmdr_about::fact::{AboutTarget, Fact, FactKind, NotApplicableWhy, UnknownWhy};
use cmdr_about::gather::Job;
use cmdr_about::payload::{AppRef, Bundle, Signature, Usage};
use cmdr_about::sniff::{HEAD_BYTES, RealType, TAIL_BYTES, real_type};
use cmdr_about::text::ItemText;

use cmdr_about::owner::OwnerQuestion;

use super::digest::{app_data_fact, folder_digest_fact};
use super::git::git_fact;
use super::history::{History, cmdr_operation};
use super::index::{IndexPlace, importance_fact, recursive_size_fact};
use super::owner::{AppSource, owner_fact};
use super::platform;
use super::probe::Probe;
use super::session::SubJobs;
use crate::file_system::framework_pool::note_activity;

/// A folder, an app, or a package: what git and the index see as a folder.
fn is_folder_like(target: AboutTarget) -> bool {
    matches!(
        target,
        AboutTarget::Folder | AboutTarget::AppBundle | AboutTarget::Package
    )
}

/// Unix seconds now, for the importance explanation's recency terms.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// The rows this host gathers: every kind in the vocabulary.
pub(crate) const GATHERED: [FactKind; 15] = [
    FactKind::Identity,
    FactKind::RealType,
    FactKind::Bundle,
    FactKind::Signature,
    FactKind::Download,
    FactKind::CmdrOperation,
    FactKind::LastOpened,
    FactKind::DefaultApp,
    FactKind::Importance,
    FactKind::Owner,
    FactKind::Git,
    FactKind::RecursiveSize,
    FactKind::FolderDigest,
    FactKind::AppData,
    FactKind::GitSnapshot,
];

/// Where a local card's stores and lookups come from, decided before the plan. A
/// store that isn't there leaves its row out of the plan (`route::local_gathered`),
/// since a row with no job would never settle.
#[derive(Clone)]
pub(crate) struct Sources {
    /// Cmdr's own operation log. `None`: no history row.
    pub(crate) history: Option<History>,
    /// Where the item sits in the drive index. `None`: no size, importance,
    /// digest, or app-data row.
    pub(crate) index: Option<IndexPlace>,
    /// The person's home folder, whose Library an app's data is looked for in.
    /// `None` (a sandboxed test runner): no app-data row.
    pub(crate) home: Option<String>,
    /// What the item's path says about its owner.
    pub(crate) owner: OwnerQuestion,
    /// Where the owner row's lookups come from.
    pub(crate) apps: AppSource,
    /// Where the rows that ask the OS's frameworks get their answers.
    pub(crate) os: OsReads,
}

/// Where the rows that ask an OS framework over XPC (Spotlight, LaunchServices,
/// code signing) get their answers: the OS, or a test's fixed ones. The download
/// record is plain `getxattr`, so it always reads the item.
///
/// Gotcha: the first of these calls in a process pays to connect to the
/// framework's daemon, 0.7–1.9 s alone and past 8 s with the whole Rust suite
/// starting a process per test (macOS 27.0, nextest, 2026-11-14), so a test of the
/// card's flow that asked the real OS timed out under load. Flow tests script
/// them (`Sources::none`); the real calls have their own tests in `platform/`.
#[derive(Clone, Copy)]
pub(crate) struct OsReads {
    /// Spotlight's use record.
    pub(crate) last_opened: fn(&str) -> Fact<Usage>,
    /// LaunchServices' default app.
    pub(crate) default_app: fn(&str) -> Fact<AppRef>,
    /// The code signature.
    pub(crate) signature: fn(&str, AboutTarget, Option<&str>) -> Fact<Signature>,
}

impl OsReads {
    /// The OS's own answers.
    pub(crate) fn of_the_os() -> Self {
        Self {
            last_opened: platform::last_opened,
            default_app: platform::default_app,
            signature: platform::signature,
        }
    }

    /// Nothing recorded, for every read: no framework is asked.
    #[cfg(test)]
    pub(crate) fn scripted() -> Self {
        Self {
            last_opened: |_| Fact::unknown(UnknownWhy::NoRecord),
            default_app: |_| Fact::unknown(UnknownWhy::NoRecord),
            signature: |_, _, _| Fact::unknown(UnknownWhy::NoRecord),
        }
    }
}

impl Sources {
    /// No store at all, and a path that names no owner: the identity and OS rows
    /// only, with the framework reads scripted (`OsReads`).
    #[cfg(test)]
    pub(crate) fn none() -> Self {
        Self {
            history: None,
            index: None,
            home: None,
            owner: OwnerQuestion::Nothing,
            apps: AppSource::of_the_os(),
            os: OsReads::scripted(),
        }
    }
}

/// The largest `Info.plist` Cmdr parses. A real one is a few KB; anything this
/// size is no description worth reading, and the bytes are the bundle author's.
const MAX_INFO_PLIST_BYTES: u64 = 1024 * 1024;

/// How long the card waits for any one row before saying it timed out. The job
/// may still be running then; its answer is dropped. Every local row is a
/// metadata read or a bounded file read, well inside it on a healthy disk; the
/// signature's executable check has its own, shorter budget inside this one.
pub(crate) const ROW_BUDGET: Duration = Duration::from_secs(1);

/// The jobs for `plan`'s rows on a local item. Identity isn't one: the probe
/// already answered it. `sub_jobs`: where a row's own pool work goes (git's status
/// walk, the digest's searches), under the card's cancel.
pub(crate) fn local_jobs(
    plan: &AboutPlan,
    path: &str,
    probe: &Probe,
    sources: &Sources,
    sub_jobs: &SubJobs,
) -> Vec<(FactKind, Job<AboutFact>)> {
    plan.kinds()
        .into_iter()
        .filter(|kind| *kind != FactKind::Identity)
        .filter_map(|kind| job(kind, path.to_string(), probe, sources, sub_jobs.clone()).map(|job| (kind, job)))
        .collect()
}

fn job(kind: FactKind, path: String, probe: &Probe, sources: &Sources, sub_jobs: SubJobs) -> Option<Job<AboutFact>> {
    let target = probe.target;
    let extension = probe.extension.clone();
    let os = sources.os;
    // Each job names its call first, so a worker that never comes back says which
    // one it's stuck in (`pool::submit`). The kind, ❌ never the path.
    let job: Job<AboutFact> = match kind {
        FactKind::RealType => Box::new(move || {
            note_activity("read the first and last bytes", "realType");
            AboutFact::RealType(real_type_of(&path, extension.as_deref()))
        }),
        FactKind::Bundle => Box::new(move || {
            note_activity("read Info.plist", "bundle");
            AboutFact::Bundle(bundle_of(Path::new(&path), target))
        }),
        FactKind::Signature => Box::new(move || {
            note_activity("read the code signature", "signature");
            AboutFact::Signature((os.signature)(&path, target, extension.as_deref()))
        }),
        FactKind::Download => Box::new(move || {
            note_activity("getxattr", "download");
            AboutFact::Download(platform::download(&path))
        }),
        FactKind::LastOpened => Box::new(move || {
            note_activity("MDItemCopyAttribute", "lastOpened");
            AboutFact::LastOpened((os.last_opened)(&path))
        }),
        FactKind::DefaultApp => Box::new(move || {
            note_activity("URLForApplicationToOpenURL", "defaultApp");
            AboutFact::DefaultApp((os.default_app)(&path))
        }),
        FactKind::CmdrOperation => {
            let history = sources.history.clone()?;
            let node = probe.node;
            Box::new(move || {
                note_activity("read the operation log", "cmdrOperation");
                AboutFact::CmdrOperation(cmdr_operation(&history, &path, node))
            })
        }
        FactKind::Owner => {
            let question = sources.owner.clone();
            let apps = sources.apps;
            Box::new(move || AboutFact::Owner(owner_fact(&question, &path, apps)))
        }
        FactKind::Git => {
            let repo = probe.repo.clone();
            let is_folder = is_folder_like(target);
            Box::new(move || AboutFact::Git(git_fact(&repo, Path::new(&path), is_folder, &sub_jobs)))
        }
        FactKind::RecursiveSize => {
            let place = sources.index.clone()?;
            Box::new(move || {
                note_activity("read the index's folder size", "recursiveSize");
                AboutFact::RecursiveSize(recursive_size_fact(&place))
            })
        }
        FactKind::Importance => {
            let place = sources.index.clone().filter(IndexPlace::scores)?;
            Box::new(move || {
                note_activity("read the importance store", "importance");
                AboutFact::Importance(importance_fact(&place, now_secs()))
            })
        }
        FactKind::FolderDigest => {
            let place = sources.index.clone()?;
            Box::new(move || {
                note_activity("read the index's folder listing", "folderDigest");
                AboutFact::FolderDigest(folder_digest_fact(&place, now_secs(), &sub_jobs))
            })
        }
        FactKind::AppData => {
            let home = sources.home.clone()?;
            Box::new(move || {
                note_activity("read the index's Library listings", "appData");
                // The app's own id, from its `Info.plist`: the bundle row reads the
                // same small file, and the two jobs don't wait on each other. A
                // description Cmdr couldn't read says why, ❌ never "no record".
                let bundle_id = match bundle_of(Path::new(&path), target) {
                    Fact::Known(bundle) => bundle.bundle_id,
                    Fact::Unknown(unknown) => return AboutFact::AppData(Fact::unknown(unknown.why())),
                    Fact::NotApplicable(_) | Fact::Pending => None,
                };
                let index_id = crate::index_host::index().volume_id_for_path(&home);
                AboutFact::AppData(app_data_fact(
                    bundle_id.as_ref().map(ItemText::as_str),
                    &home,
                    &index_id,
                ))
            })
        }
        // The probe answered identity; a snapshot from git's history is never a
        // local item (the route sends it through the portal).
        FactKind::Identity | FactKind::GitSnapshot => return None,
    };
    Some(job)
}

/// The file's first [`HEAD_BYTES`] and, when it's longer, its last
/// [`TAIL_BYTES`] (a disk image is told by its trailer), against its extension.
pub(crate) fn real_type_of(path: &str, extension: Option<&str>) -> Fact<RealType> {
    match read_head_and_tail(Path::new(path)) {
        Ok((head, tail)) => Fact::Known(real_type(&head, tail.as_deref(), extension)),
        Err(error) => Fact::unknown(why_of_io(&error)),
    }
}

/// Why a read of the item's own bytes failed: gone since the probe, refused, or the
/// disk didn't deliver them. ❌ Never `Unparseable`: nothing was there to parse.
pub(crate) fn why_of_io(error: &std::io::Error) -> UnknownWhy {
    match error.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory => UnknownWhy::NotFound,
        std::io::ErrorKind::PermissionDenied => UnknownWhy::PermissionDenied,
        _ => UnknownWhy::Unreachable,
    }
}

fn read_head_and_tail(path: &Path) -> std::io::Result<(Vec<u8>, Option<Vec<u8>>)> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let mut head = Vec::with_capacity(HEAD_BYTES.min(usize::try_from(len).unwrap_or(HEAD_BYTES)));
    (&mut file).take(HEAD_BYTES as u64).read_to_end(&mut head)?;
    // A file no longer than the head has no tail the head doesn't hold.
    if len <= HEAD_BYTES as u64 {
        return Ok((head, None));
    }
    file.seek(SeekFrom::End(-(TAIL_BYTES as i64)))?;
    let mut tail = Vec::with_capacity(TAIL_BYTES);
    file.take(TAIL_BYTES as u64).read_to_end(&mut tail)?;
    Ok((head, Some(tail)))
}

/// What the top-level bundle's `Info.plist` says, never a nested one's. An app's
/// is `Contents/Info.plist`; a framework or plug-in may keep it at the root.
pub(crate) fn bundle_of(path: &Path, target: AboutTarget) -> Fact<Bundle> {
    let candidates = [path.join("Contents/Info.plist"), path.join("Info.plist")];
    let Some((plist, size)) = candidates.iter().find_map(|candidate| {
        let metadata = std::fs::symlink_metadata(candidate).ok()?;
        metadata.is_file().then(|| (candidate.clone(), metadata.len()))
    }) else {
        return match target {
            // An app with no description is still an app: it names nothing.
            AboutTarget::AppBundle => Fact::Known(Bundle {
                bundle_id: None,
                display_name: None,
                version: None,
            }),
            _ => Fact::NotApplicable(NotApplicableWhy::NotABundle),
        };
    };
    if size > MAX_INFO_PLIST_BYTES {
        return Fact::unknown(UnknownWhy::Unparseable);
    }
    // Read here, then parsed, so a file Cmdr couldn't read says why instead of
    // reading as one it couldn't make sense of.
    let mut bytes = Vec::new();
    let read = std::fs::File::open(&plist).and_then(|file| file.take(MAX_INFO_PLIST_BYTES).read_to_end(&mut bytes));
    if let Err(error) = read {
        return Fact::unknown(why_of_io(&error));
    }
    let Ok(value) = plist::Value::from_reader(std::io::Cursor::new(bytes)) else {
        return Fact::unknown(UnknownWhy::Unparseable);
    };
    let Some(dictionary) = value.as_dictionary() else {
        return Fact::unknown(UnknownWhy::Unparseable);
    };
    let text = |key: &str| dictionary.get(key).and_then(plist::Value::as_string).map(ItemText::new);
    Fact::Known(Bundle {
        bundle_id: text("CFBundleIdentifier"),
        display_name: text("CFBundleDisplayName").or_else(|| text("CFBundleName")),
        version: text("CFBundleShortVersionString"),
    })
}
