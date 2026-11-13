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
use cmdr_about::payload::Bundle;
use cmdr_about::sniff::{HEAD_BYTES, RealType, TAIL_BYTES, real_type};
use cmdr_about::text::ItemText;

use super::platform;
use super::probe::Probe;
use crate::file_system::framework_pool::note_activity;

/// The rows this host gathers. The rest of the vocabulary (Cmdr's own history,
/// git, importance, sizes, the owner, a folder's digest, an app's data) comes from
/// other stores, and the plan leaves those rows out until their jobs exist.
pub(crate) const GATHERED: [FactKind; 7] = [
    FactKind::Identity,
    FactKind::RealType,
    FactKind::Bundle,
    FactKind::Signature,
    FactKind::Download,
    FactKind::LastOpened,
    FactKind::DefaultApp,
];

/// The largest `Info.plist` Cmdr parses. A real one is a few KB; anything this
/// size is no description worth reading, and the bytes are the bundle author's.
const MAX_INFO_PLIST_BYTES: u64 = 1024 * 1024;

/// How long the card waits for any one row before saying it timed out. The job
/// may still be running then; its answer is dropped. Every local row is a
/// metadata read or a bounded file read, well inside it on a healthy disk; the
/// signature's executable check has its own, shorter budget inside this one.
pub(crate) const ROW_BUDGET: Duration = Duration::from_secs(1);

/// The jobs for `plan`'s rows on a local item. Identity isn't one: the probe
/// already answered it.
pub(crate) fn local_jobs(plan: &AboutPlan, path: &str, probe: &Probe) -> Vec<(FactKind, Job<AboutFact>)> {
    plan.kinds()
        .into_iter()
        .filter(|kind| *kind != FactKind::Identity)
        .filter_map(|kind| job(kind, path.to_string(), probe).map(|job| (kind, job)))
        .collect()
}

fn job(kind: FactKind, path: String, probe: &Probe) -> Option<Job<AboutFact>> {
    let target = probe.target;
    let extension = probe.extension.clone();
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
            AboutFact::Signature(platform::signature(&path, target, extension.as_deref()))
        }),
        FactKind::Download => Box::new(move || {
            note_activity("getxattr", "download");
            AboutFact::Download(platform::download(&path))
        }),
        FactKind::LastOpened => Box::new(move || {
            note_activity("MDItemCopyAttribute", "lastOpened");
            AboutFact::LastOpened(platform::last_opened(&path))
        }),
        FactKind::DefaultApp => Box::new(move || {
            note_activity("URLForApplicationToOpenURL", "defaultApp");
            AboutFact::DefaultApp(platform::default_app(&path))
        }),
        FactKind::Identity
        | FactKind::CmdrOperation
        | FactKind::Importance
        | FactKind::Owner
        | FactKind::Git
        | FactKind::RecursiveSize
        | FactKind::FolderDigest
        | FactKind::AppData => return None,
    };
    Some(job)
}

/// The file's first [`HEAD_BYTES`] and, when it's longer, its last
/// [`TAIL_BYTES`] (a disk image is told by its trailer), against its extension.
pub(crate) fn real_type_of(path: &str, extension: Option<&str>) -> Fact<RealType> {
    match read_head_and_tail(Path::new(path)) {
        Ok((head, tail)) => Fact::Known(real_type(&head, tail.as_deref(), extension)),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            Fact::unknown(UnknownWhy::PermissionDenied)
        }
        Err(_) => Fact::unknown(UnknownWhy::Unparseable),
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
    let Ok(value) = plist::Value::from_file(&plist) else {
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
