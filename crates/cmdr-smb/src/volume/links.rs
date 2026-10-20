//! What a path reads as THROUGH a link, for the stats that decide how a
//! selected item is read: listed as a folder or streamed as a file.
//!
//! smb2's `stat` describes a link itself (`FILE_OPEN_REPARSE_POINT`), and Samba
//! 4.23 reports a Unix symlink as a non-directory sized by its link text, so a
//! copy streamed a selected folder symlink as a six-byte file and the READ
//! failed. A link's `is_symlink` still comes from the stat's tag, so
//! `entry_kind` keeps calling it a link (`DETAILS.md` § "Junctions and
//! symlinks").

use smb2::client::tree::FileInfo;
use smb2::client::{Connection, Tree};

/// `stat`, with a link the server calls a non-directory resolved to its
/// target's kind and size: a folder (size 0) when the path opens as a
/// directory, else the size a following open reports. Costs nothing for a
/// non-link, and two round trips for such a link only. A probe that can't open
/// the target (a dangling link, a Windows symlink the server won't follow)
/// leaves the stat's own answer.
pub(super) async fn stat_through_link(tree: &Tree, conn: &mut Connection, path: &str) -> Result<FileInfo, smb2::Error> {
    let mut info = tree.stat(conn, path).await?;
    if info.is_link() && !info.is_directory {
        follow_to_target(tree, conn, path, &mut info).await;
    }
    Ok(info)
}

/// Puts the target's kind and size on `info`, leaving it alone where the target
/// won't open. The dates stay the link's: no following call carries them.
async fn follow_to_target(tree: &Tree, conn: &mut Connection, path: &str, info: &mut FileInfo) {
    if let Ok(reader) = tree.open_directory_reader(conn.clone(), path).await {
        // The open already answered "folder"; a failed CLOSE changes nothing.
        let _ = reader.close().await;
        info.is_directory = true;
        info.size = 0;
        return;
    }
    if let Ok((file_id, size)) = tree.open_file(conn, path).await {
        let _ = tree.close_handle(conn, file_id).await;
        info.size = size;
    }
}
