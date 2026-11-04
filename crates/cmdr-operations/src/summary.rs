//! What the queue row says an operation is doing: the short `source →
//! destination` names, and the items it acts on ([`OperationItems`]).
//!
//! Built once, at registration, from what the starter already holds: ❌ no I/O
//! here. A source's kind (file or folder) would take a `stat` each, a network
//! round trip on a share, so it comes from the starter
//! ([`ItemKinds`]) or not at all.

use std::path::{Path, PathBuf};

use super::manager::{OperationItems, OperationSummaryText};
use super::types::ItemKinds;

/// How many top-level names the row's tooltip lists; past that it says how
/// many more there are.
pub(crate) const ITEM_NAMES_SHOWN: usize = 20;

/// Best-effort `source → destination` summary for a local operation: the
/// source items' display names joined, the destination's name, and the items
/// spelled out for the queue row. Cheap; no I/O.
pub(crate) fn path_summary(
    sources: &[PathBuf],
    destination: Option<&Path>,
    kinds: Option<ItemKinds>,
) -> OperationSummaryText {
    fn name(p: &Path) -> String {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.to_string_lossy().into_owned())
    }
    let source = match sources {
        [] => None,
        [one] => Some(name(one)),
        many => Some(format!("{} ({} items)", name(&many[0]), many.len())),
    };
    let from = sources.first().and_then(|first| first.parent()).map(local_location);
    OperationSummaryText {
        source,
        destination: destination.map(name),
        items: Some(operation_items(sources, kinds, from, destination.map(local_location))),
    }
}

/// `sources` as the queue row names them, coming `from` one folder and going
/// `to` another (both already in display form).
pub(crate) fn operation_items(
    sources: &[PathBuf],
    kinds: Option<ItemKinds>,
    from: Option<String>,
    to: Option<String>,
) -> OperationItems {
    let count = u32::try_from(sources.len()).unwrap_or(u32::MAX);
    OperationItems {
        count,
        kinds: kinds.filter(|kinds| u64::from(kinds.files) + u64::from(kinds.folders) == u64::from(count)),
        names: sources
            .iter()
            .take(ITEM_NAMES_SHOWN)
            .map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned())
            })
            .collect(),
        from,
        to,
    }
}

/// `sources` on `source_volume` as the queue row names them, going to
/// `destination` on its volume when there is one.
pub(crate) fn volume_items(
    source_volume: &dyn cmdr_fs::volume::Volume,
    sources: &[PathBuf],
    kinds: Option<ItemKinds>,
    destination: Option<(&dyn cmdr_fs::volume::Volume, &Path)>,
) -> OperationItems {
    let from = sources
        .first()
        .and_then(|first| first.parent())
        .map(|parent| volume_location(source_volume, parent));
    let to = destination.map(|(volume, path)| volume_location(volume, path));
    operation_items(sources, kinds, from, to)
}

/// A path on this Mac as a person reads it: the home folder as `~`.
pub(crate) fn local_location(path: &Path) -> String {
    tildify(path, crate::local_disk::home_dir().as_deref())
}

/// A path on `volume` as a person reads it: `<volume name>/<path inside it>`,
/// or [`local_location`] on the Mac's own disk, where the volume name
/// ("Macintosh HD") says nothing a path doesn't.
pub(crate) fn volume_location(volume: &dyn cmdr_fs::volume::Volume, path: &Path) -> String {
    if volume.local_path().is_some_and(|root| root == Path::new("/")) {
        return local_location(path);
    }
    location_on(volume.name(), volume.root(), path)
}

fn tildify(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.to_string_lossy()),
        None => path.to_string_lossy().into_owned(),
    }
}

/// `path` inside the volume named `volume_name`, by whole components: a path
/// the starter handed over relative to the root reads the same as an absolute one.
fn location_on(volume_name: &str, root: &Path, path: &Path) -> String {
    let inside = path.strip_prefix(root).unwrap_or(path);
    let inside = inside.to_string_lossy();
    let inside = inside.trim_matches('/');
    if inside.is_empty() {
        volume_name.to_string()
    } else {
        format!("{volume_name}/{inside}")
    }
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
