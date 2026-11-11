//! Prices the local listers against each other, and what Finder's "Last opened"
//! would cost per entry. Runs production's listers (`reading::list_directory_with`,
//! `testing`-gated, on for this dev target), so the numbers in
//! `docs/notes/getattrlistbulk-listing-benchmarks-2026-10.md` stay re-runnable
//! against shipped code. Dev-only; macOS only.
//!
//! ```sh
//! cargo run --release -p cmdr-local --example listing_bench -- gen <dir> 100000
//! cargo run --release -p cmdr-local --example listing_bench -- bench <dir> [runs] [methods]
//! cargo run --release -p cmdr-local --example listing_bench -- parity <dir>...
//! cargo run --release -p cmdr-local --example listing_bench -- volcaps <dir>...
//! cargo run --release -p cmdr-local --example listing_bench -- vanish <dir>
//! ```
//!
//! Methods (`bench`'s third argument, comma-separated; default: all four):
//! - `current`: `reading::list_directory_core`, what production runs (bulk on
//!   APFS and HFS+, `read_dir` elsewhere).
//! - `bulk`: the bulk lister forced.
//! - `readdir`: the `read_dir` lister forced, which on APFS and HFS+ adds one
//!   `getattrlist(ATTR_CMN_ADDEDTIME)` per entry (the fallback's cost).
//! - `bulk_xattr`: `bulk` plus one `getxattr(com.apple.lastuseddate#PS)` per file.
//!
//! `bench` interleaves methods round-robin (one warm-up round, then `runs`
//! rounds) so none of them gets a systematically warmer cache, and reports the
//! median wall and CPU (user + sys, `getrusage`) per method. `runs` = 0 times
//! one pass per method and nothing else: give each method its own fresh fixture
//! and process for the cold-ish numbers.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a measurement harness prints its measurements"
)]

#[cfg(target_os = "macos")]
mod bulk;

#[cfg(target_os = "macos")]
fn main() {
    macos::main();
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn main() {
    eprintln!("listing_bench is macOS only");
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::CString;
    use std::fs;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use cmdr_fs::attr_buf::{Attr, Request};
    use cmdr_fs::entry::FileEntry;
    use cmdr_fs::volume::host::listings::RecordingListings;
    use cmdr_local::reading::{self, Lister};

    use super::bulk;

    pub fn main() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let Some(cmd) = args.first() else {
            eprintln!("usage: listing_bench gen|bench|parity|volcaps|vanish ...");
            std::process::exit(2);
        };
        match cmd.as_str() {
            "gen" => generate(Path::new(&args[1]), args[2].parse().expect("file count")),
            "bench" => {
                let runs = args.get(2).map_or(7, |r| r.parse().expect("runs"));
                let methods = args.get(3).map_or("current,bulk,readdir,bulk_xattr", String::as_str);
                bench(Path::new(&args[1]), runs, methods);
            }
            "parity" => {
                for dir in &args[1..] {
                    parity(Path::new(dir));
                }
            }
            "volcaps" => {
                for dir in &args[1..] {
                    volcaps(Path::new(dir));
                }
            }
            "vanish" => vanish(Path::new(&args[1])),
            other => panic!("unknown command {other}"),
        }
    }

    fn run_method(method: &str, dir: &Path, host: &RecordingListings) -> Vec<FileEntry> {
        match method {
            "current" => reading::list_directory_core(dir, host).expect("current"),
            "bulk" => reading::list_directory_with(dir, false, host, Lister::Bulk).expect("bulk"),
            "readdir" => reading::list_directory_with(dir, false, host, Lister::ReadDir).expect("readdir"),
            "bulk_xattr" => {
                let entries = reading::list_directory_with(dir, false, host, Lister::Bulk).expect("bulk");
                for entry in &entries {
                    if !entry.is_directory && !entry.is_symlink {
                        std::hint::black_box(bulk::read_last_used(Path::new(&entry.path)));
                    }
                }
                entries
            }
            other => panic!("unknown method {other}"),
        }
    }

    fn cpu_time() -> Duration {
        // SAFETY: `getrusage` fills the zeroed struct we hand it.
        let usage = unsafe {
            let mut usage: libc::rusage = std::mem::zeroed();
            libc::getrusage(libc::RUSAGE_SELF, &mut usage);
            usage
        };
        let tv = |t: libc::timeval| Duration::new(t.tv_sec as u64, t.tv_usec as u32 * 1000);
        tv(usage.ru_utime) + tv(usage.ru_stime)
    }

    fn median(mut v: Vec<Duration>) -> Duration {
        v.sort();
        v[v.len() / 2]
    }

    fn bench(dir: &Path, runs: usize, methods: &str) {
        let host = RecordingListings::new();
        let methods: Vec<&str> = methods.split(',').collect();
        let mut wall: Vec<Vec<Duration>> = vec![Vec::new(); methods.len()];
        let mut cpu: Vec<Vec<Duration>> = vec![Vec::new(); methods.len()];
        let mut first: Vec<Option<(Duration, Duration)>> = vec![None; methods.len()];
        let mut count = 0;
        for round in 0..=runs {
            for (i, method) in methods.iter().enumerate() {
                let c0 = cpu_time();
                let t0 = Instant::now();
                let entries = run_method(method, dir, &host);
                let w = t0.elapsed();
                let c = cpu_time() - c0;
                count = entries.len();
                std::hint::black_box(&entries);
                drop(entries);
                if round == 0 {
                    first[i] = Some((w, c));
                } else {
                    wall[i].push(w);
                    cpu[i].push(c);
                }
            }
        }
        println!("dir={} entries={count} runs={runs}", dir.display());
        for (i, method) in methods.iter().enumerate() {
            let (fw, fc) = first[i].expect("warm-up ran");
            if runs == 0 {
                println!(
                    "  {method:<14} first pass only: wall {:>9.2} ms  cpu {:>9.2} ms  ({:.2} µs/entry)",
                    fw.as_secs_f64() * 1e3,
                    fc.as_secs_f64() * 1e3,
                    fw.as_secs_f64() * 1e6 / count.max(1) as f64,
                );
                continue;
            }
            let mw = median(wall[i].clone());
            let mc = median(cpu[i].clone());
            println!(
                "  {method:<14} median wall {:>9.2} ms  cpu {:>9.2} ms  ({:.2} µs/entry)  | first pass wall {:>9.2} ms cpu {:>9.2} ms",
                mw.as_secs_f64() * 1e3,
                mc.as_secs_f64() * 1e3,
                mw.as_secs_f64() * 1e6 / count.max(1) as f64,
                fw.as_secs_f64() * 1e3,
                fc.as_secs_f64() * 1e3,
            );
        }
    }

    /// Diffs production's bulk lister against its `read_dir` lister, every field
    /// (`added_at` included: both fill it on a bulk-eligible folder), and checks
    /// the added date against an independent per-path `getattrlist`. A sandbox
    /// `EPERM` entry (a minimal row from `read_dir`, the real entry from bulk) is
    /// the one intended difference and is counted apart.
    fn parity(dir: &Path) {
        let host = RecordingListings::new();
        let list = |lister| reading::list_directory_with(dir, false, &host, lister);
        let (read_dir, bulk) = match (list(Lister::ReadDir), list(Lister::Bulk)) {
            (Ok(r), Ok(b)) => (r, b),
            (r, b) => {
                println!(
                    "{}: read_dir {:?}, bulk {:?}",
                    dir.display(),
                    r.map(|v| v.len()),
                    b.map(|v| v.len())
                );
                return;
            }
        };
        let mut diffs = 0;
        let mut eperm = 0;
        let mut added_some = 0;
        let mut added_mismatch = 0;
        if read_dir.len() != bulk.len() {
            println!(
                "{}: COUNT read_dir={} bulk={}",
                dir.display(),
                read_dir.len(),
                bulk.len()
            );
        }
        let bulk_names: std::collections::HashMap<&str, &FileEntry> =
            bulk.iter().map(|e| (e.name.as_str(), e)).collect();
        for re in &read_dir {
            let Some(be) = bulk_names.get(re.name.as_str()) else {
                println!("  only in read_dir: {}", re.name);
                diffs += 1;
                continue;
            };
            if be.added_at.is_some() {
                added_some += 1;
            }
            if be.added_at != reading::added_at_by_path(Path::new(&re.path)) {
                added_mismatch += 1;
            }
            let (rs, bs) = (format!("{re:?}"), format!("{be:?}"));
            if rs == bs {
                continue;
            }
            let sandboxed = re.modified_at.is_none()
                && re.owner.is_empty()
                && fs::symlink_metadata(&re.path).err().and_then(|e| e.raw_os_error()) == Some(libc::EPERM);
            if sandboxed {
                eperm += 1;
                continue;
            }
            diffs += 1;
            if diffs <= 8 {
                let fields: Vec<String> = rs
                    .split(", ")
                    .zip(bs.split(", "))
                    .filter(|(a, b)| a != b)
                    .map(|(a, b)| format!("{a} vs {b}"))
                    .collect();
                println!("  DIFF {} (read_dir vs bulk): {}", re.name, fields.join("; "));
            }
        }
        println!(
            "{}: {} entries, {diffs} differing, {eperm} sandbox EPERM (expected), added_at set on {added_some} ({added_mismatch} disagree with per-path getattrlist)",
            dir.display(),
            read_dir.len(),
        );
    }

    fn volcaps(dir: &Path) {
        match bulk::volume_attr_support(dir) {
            Ok(s) => println!(
                "{}: attrlist native={}, ADDEDTIME valid={} native={}, FLAGS valid={}",
                dir.display(),
                s.attrlist_native,
                s.addedtime_valid,
                s.addedtime_native,
                s.flags_valid
            ),
            Err(e) => println!("{}: volcaps failed: {e}", dir.display()),
        }
    }

    /// Lists with a tiny buffer, deleting every not-yet-returned `vanish-*`
    /// file after the first batch, to see what `getattrlistbulk` does when
    /// entries disappear mid-listing.
    fn vanish(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
        fs::create_dir_all(dir).expect("mkdir");
        for i in 0..2000 {
            fs::write(dir.join(format!("vanish-{i:05}")), b"").expect("write");
        }
        let c_path = CString::new(dir.as_os_str().as_bytes()).expect("cstring");
        // SAFETY: valid NUL-terminated path.
        let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY) };
        assert!(fd >= 0, "open");
        let mut al = Request::new(&[Attr::Name, Attr::Error]).attrlist();
        let mut buf = vec![0u64; 512];
        let mut total = 0;
        let mut calls = 0;
        loop {
            // SAFETY: `al` valid, `buf` writable for 4 KiB.
            let n = unsafe {
                libc::getattrlistbulk(
                    fd,
                    (&raw mut al).cast(),
                    buf.as_mut_ptr().cast(),
                    4096,
                    u64::from(libc::FSOPT_PACK_INVAL_ATTRS),
                )
            };
            if n < 0 {
                println!("call {calls}: error {}", std::io::Error::last_os_error());
                break;
            }
            if n == 0 {
                break;
            }
            calls += 1;
            total += n;
            if calls == 1 {
                // Delete everything; the kernel already handed us the first batch.
                for i in 0..2000 {
                    let _ = fs::remove_file(dir.join(format!("vanish-{i:05}")));
                }
                println!("first batch {n}, then deleted all 2000");
            }
        }
        // SAFETY: closing the fd opened above.
        unsafe { libc::close(fd) };
        println!("vanish: {calls} calls, {total} entries returned in total");
    }

    /// Builds a fixture: `files` files (90% empty, 10% with 1–8 KiB), plus
    /// directories, symlinks (to files, to dirs, broken), hard links, `UF_HIDDEN`
    /// and dot files, and the last-used xattr on 30% of files (39 of 99 files
    /// carried it in a real Downloads folder).
    fn generate(dir: &Path, files: usize) {
        fs::create_dir_all(dir).expect("mkdir");
        let dirs = (files / 30).clamp(30, 300);
        let links = dirs;
        let payload = vec![b'x'; 8192];
        for i in 0..files {
            let name = if i % 100 == 7 {
                format!(".dot-{i:06}")
            } else {
                format!("file-{i:06}.txt")
            };
            let p = dir.join(&name);
            let len = if i % 10 == 3 { 1024 + (i % 8) * 896 } else { 0 };
            fs::write(&p, &payload[..len]).expect("write");
            if i % 100 == 11 {
                set_hidden_flag(&p);
            }
            if i % 10 < 3 {
                set_last_used(&p, 1_780_000_000 + i as i64);
            }
        }
        for i in 0..dirs {
            fs::create_dir(dir.join(format!("dir-{i:04}"))).expect("mkdir");
        }
        for i in 0..links {
            let target = match i % 3 {
                0 => PathBuf::from(format!("file-{:06}.txt", i * 3)),
                1 => PathBuf::from(format!("dir-{:04}", i % dirs)),
                _ => PathBuf::from(format!("missing-{i}")),
            };
            symlink(target, dir.join(format!("link-{i:04}"))).expect("symlink");
        }
        for i in 0..20.min(files) {
            let src = dir.join(format!("file-{:06}.txt", i * 10 + 1));
            let _ = fs::hard_link(&src, dir.join(format!("hardlink-{i:02}")));
        }
        println!(
            "{}: {files} files, {dirs} dirs, {links} symlinks, 20 hard links",
            dir.display()
        );
    }

    fn set_hidden_flag(p: &Path) {
        let c = CString::new(p.as_os_str().as_bytes()).expect("cstring");
        // SAFETY: valid NUL-terminated path.
        unsafe { libc::chflags(c.as_ptr(), libc::UF_HIDDEN) };
    }

    fn set_last_used(p: &Path, secs: i64) {
        let c = CString::new(p.as_os_str().as_bytes()).expect("cstring");
        let mut value = [0u8; 16];
        value[..8].copy_from_slice(&secs.to_le_bytes());
        // SAFETY: valid NUL-terminated strings; `value` readable for 16 bytes.
        unsafe {
            libc::setxattr(
                c.as_ptr(),
                bulk::LAST_USED_XATTR.as_ptr().cast(),
                value.as_ptr().cast(),
                16,
                0,
                0,
            )
        };
    }
}
