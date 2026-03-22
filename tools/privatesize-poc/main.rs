//! Proof-of-concept: compare st_blocks*512 vs ATTR_CMNEXT_PRIVATESIZE on macOS.
//!
//! Usage: privatesize-poc <path>
//!   Walks the directory tree, sums both metrics, and prints the difference.

use std::ffi::{CString, c_char, c_void};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

// --- macOS getattrlist FFI ---

#[repr(C)]
#[derive(Default)]
struct AttrList {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

// CMNEXT attributes go in forkattr field, require FSOPT_ATTR_CMN_EXTENDED option
const ATTR_CMNEXT_PRIVATESIZE: u32 = 0x0000_0008;
const FSOPT_NOFOLLOW: u64 = 0x0000_0001;
const FSOPT_ATTR_CMN_EXTENDED: u64 = 0x0000_0020;

unsafe extern "C" {
    fn getattrlist(
        path: *const c_char,
        attrlist: *mut c_void,
        attrbuf: *mut c_void,
        attrbufsize: usize,
        options: u64,
    ) -> i32;
}

/// Returns the private size (bytes freed on deletion) for a file.
fn get_private_size(path: &Path) -> Option<u64> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;

    let mut attrlist = AttrList {
        bitmapcount: 5,
        forkattr: ATTR_CMNEXT_PRIVATESIZE,
        ..Default::default()
    };

    // Buffer: u32 length + off_t privatesize (i64)
    let mut buf = [0u8; 12];

    let ret = unsafe {
        getattrlist(
            c_path.as_ptr(),
            &mut attrlist as *mut AttrList as *mut c_void,
            buf.as_mut_ptr() as *mut c_void,
            buf.len(),
            FSOPT_NOFOLLOW | FSOPT_ATTR_CMN_EXTENDED,
        )
    };

    if ret != 0 {
        return None;
    }

    let length = u32::from_ne_bytes(buf[0..4].try_into().unwrap());
    if length >= 12 {
        let val = i64::from_ne_bytes(buf[4..12].try_into().unwrap());
        Some(val as u64)
    } else {
        None
    }
}

fn human(bytes: u64) -> String {
    const GB: f64 = 1_000_000_000.0;
    const MB: f64 = 1_000_000.0;
    if bytes as f64 >= GB {
        format!("{:.2} GB", bytes as f64 / GB)
    } else {
        format!("{:.1} MB", bytes as f64 / MB)
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: privatesize-poc <path>");
        std::process::exit(1);
    });
    let root = Path::new(&path);

    let mut total_blocks: u64 = 0;
    let mut total_private: u64 = 0;
    let mut total_logical: u64 = 0;
    let mut file_count: u64 = 0;
    let mut errors: u64 = 0;
    let mut no_private: u64 = 0;

    fn walk(
        dir: &Path,
        total_blocks: &mut u64,
        total_private: &mut u64,
        total_logical: &mut u64,
        file_count: &mut u64,
        errors: &mut u64,
        no_private: &mut u64,
        seen_inodes: &mut std::collections::HashSet<u64>,
    ) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => {
                *errors += 1;
                return;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                *errors += 1;
                continue;
            };
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                *errors += 1;
                continue;
            };
            if meta.is_symlink() {
                continue;
            }
            if meta.is_dir() {
                walk(
                    &path,
                    total_blocks,
                    total_private,
                    total_logical,
                    file_count,
                    errors,
                    no_private,
                    seen_inodes,
                );
                continue;
            }
            if !meta.is_file() {
                continue;
            }

            // Hardlink dedup
            let nlink = meta.nlink();
            let ino = meta.ino();
            if nlink > 1 && !seen_inodes.insert(ino) {
                continue;
            }

            *file_count += 1;
            *total_blocks += meta.blocks() * 512;
            *total_logical += meta.len();

            if let Some(ps) = get_private_size(&path) {
                *total_private += ps;
            } else {
                *total_private += meta.blocks() * 512;
                *no_private += 1;
            }
        }
    }

    let mut seen_inodes = std::collections::HashSet::new();
    walk(
        root,
        &mut total_blocks,
        &mut total_private,
        &mut total_logical,
        &mut file_count,
        &mut errors,
        &mut no_private,
        &mut seen_inodes,
    );

    println!("Directory:        {}", root.display());
    println!("Files scanned:    {file_count}");
    println!("Errors:           {errors}");
    println!("No privatesize:   {no_private}");
    println!();
    println!(
        "Logical (len):    {} ({total_logical} bytes)",
        human(total_logical)
    );
    println!(
        "Physical (blocks):{} ({total_blocks} bytes)",
        human(total_blocks)
    );
    println!(
        "Private size:     {} ({total_private} bytes)",
        human(total_private)
    );
    println!();
    let savings = total_blocks.saturating_sub(total_private);
    println!(
        "Savings (blocks - private): {} ({:.1}%)",
        human(savings),
        if total_blocks > 0 {
            savings as f64 / total_blocks as f64 * 100.0
        } else {
            0.0
        }
    );
}
