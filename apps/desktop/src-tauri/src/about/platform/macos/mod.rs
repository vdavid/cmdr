//! macOS: the quarantine and `kMDItemWhereFroms` xattrs, Spotlight's use history,
//! LaunchServices' default app, and the Security framework's code signature.

mod apps;
mod signature;
mod spotlight;

use std::ffi::CString;
use std::path::{Path, PathBuf};

use cmdr_about::card::OsCaps;
use cmdr_about::fact::{Fact, UnknownWhy};
use cmdr_about::payload::AppRef;
use cmdr_about::provenance::{Download, MAX_XATTR_BYTES, download_fact};
use cmdr_about::text::ItemText;

use super::StatExtras;
use super::unix::{XattrSyscalls, errno, executable, hard_links, physical_size, read_capped};

pub(crate) use apps::{app_with_id, installed_apps, receipts};
pub(crate) use signature::signature;
pub(crate) use spotlight::last_opened;

pub(crate) fn os_caps() -> OsCaps {
    OsCaps {
        use_history: true,
        code_signing: true,
        default_app: true,
        install_receipts: true,
    }
}

/// `SF_DATALESS` from `<sys/stat.h>`: what a File Provider sets on an evicted file.
const SF_DATALESS: u32 = 0x4000_0000;

pub(crate) fn stat_extras(metadata: &std::fs::Metadata) -> StatExtras {
    use std::os::darwin::fs::MetadataExt;
    StatExtras {
        executable: executable(metadata),
        online_only: metadata.st_flags() & SF_DATALESS != 0,
        physical_size: physical_size(metadata),
        hard_links: hard_links(metadata),
    }
}

/// `getxattr` with `XATTR_NOFOLLOW`: the link itself, never what it points at.
struct Darwin;

impl XattrSyscalls for Darwin {
    const NO_ATTR: i32 = libc::ENOATTR;

    fn size(path: &CString, name: &CString) -> Result<usize, i32> {
        // SAFETY: both pointers are live NUL-terminated strings for the call; a null
        // buffer with size 0 asks only for the value's size.
        let size = unsafe {
            libc::getxattr(
                path.as_ptr(),
                name.as_ptr(),
                std::ptr::null_mut(),
                0,
                0,
                libc::XATTR_NOFOLLOW,
            )
        };
        usize::try_from(size).map_err(|_| errno())
    }

    fn read(path: &CString, name: &CString, buf: &mut [u8]) -> Result<usize, i32> {
        // SAFETY: `buf` is a live, writable slice of exactly `buf.len()` bytes, and the
        // kernel writes at most that many.
        let read = unsafe {
            libc::getxattr(
                path.as_ptr(),
                name.as_ptr(),
                buf.as_mut_ptr().cast(),
                buf.len(),
                0,
                libc::XATTR_NOFOLLOW,
            )
        };
        usize::try_from(read).map_err(|_| errno())
    }
}

pub(crate) fn download(path: &str) -> Fact<Download> {
    download_record(&mut |name| read_capped::<Darwin>(path, name, MAX_XATTR_BYTES).for_decoder(MAX_XATTR_BYTES))
}

pub(crate) fn download_record(read: super::ReadXattr<'_>) -> Fact<Download> {
    let where_froms = read("com.apple.metadata:kMDItemWhereFroms");
    let quarantine = read("com.apple.quarantine");
    download_fact(where_froms.as_deref(), quarantine.as_deref())
}

pub(crate) fn default_app(path: &str) -> Fact<AppRef> {
    let Some(app) = default_app_path(path) else {
        // LaunchServices names no app for it.
        return Fact::unknown(UnknownWhy::NoRecord);
    };
    let name = crate::file_system::open_with::read_app_display_name(&app);
    let id = objc2::rc::autoreleasepool(|_| crate::file_system::open_with::read_bundle_identifier(&app));
    Fact::Known(AppRef {
        name: ItemText::new(&name),
        id: id.as_deref().map(ItemText::new),
    })
}

/// `URLForApplicationToOpenURL:` (macOS 10.6): the one app a double-click opens it with.
fn default_app_path(path: &str) -> Option<PathBuf> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};
    objc2::rc::autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let app = NSWorkspace::sharedWorkspace().URLForApplicationToOpenURL(&url)?;
        app.path().map(|path| Path::new(&path.to_string()).to_path_buf())
    })
}
