//! Post-hoc bundle-size cap. Trims log content from the head of the newest file (line
//! by line) instead of dropping whole entries, so a tight cap still ships the most
//! recent context.
//!
//! Used by Flow B (where the legacy builder doesn't enforce a cap inline) and as a
//! defense-in-depth pass on Flow A in case a future manifest grows large enough to
//! exceed the cap on its own.

use std::io::{Read, Seek, Write};
use zip::DateTime as ZipDateTime;
use zip::ZipArchive;
use zip::write::{SimpleFileOptions, ZipWriter};

/// Always preserve at least this many lines of the most recent file, even if the cap
/// would otherwise force it down to nothing. [`cap_bundle_to_mb`] may exceed the cap by
/// up to ~10% to honor this. Better to ship 1.1 MB of useful context than 0.
const MIN_TAIL_LINES_OF_NEWEST_FILE: usize = 50;

/// Cap the bundle to `cap_mb` megabytes, **trimming log content from the head**, not
/// dropping whole files.
///
/// Behavior:
/// 1. `manifest.json` is always preserved in full (verbatim, with its original mtime).
/// 2. `logs/*` entries are sorted newest-first by their stored mtime so the most recent context
///    wins the budget race.
/// 3. Each log entry's content is line-split. Lines are packed into the output zip starting from
///    the **end** of the file (the newest lines) until the compressed output approaches the cap.
///    Older lines get dropped.
/// 4. If a single entry won't fit even partially, we still preserve the last
///    [`MIN_TAIL_LINES_OF_NEWEST_FILE`] lines of the newest file, even if it pushes the output ~10%
///    over the cap. Shipping a 1.1 MB bundle with useful tail beats shipping a 542-byte bundle with
///    only the manifest (which is what the broken pre-fix-6 implementation did (see the bug
///    report).
///
/// If the input zip is already under the cap, returns it untouched.
pub fn cap_bundle_to_mb(zip_bytes: Vec<u8>, cap_mb: usize) -> Vec<u8> {
    let cap_bytes = cap_mb * 1024 * 1024;
    if zip_bytes.len() <= cap_bytes {
        return zip_bytes;
    }

    let Ok(mut archive) = ZipArchive::new(std::io::Cursor::new(&zip_bytes)) else {
        return zip_bytes;
    };

    // Pull the manifest (preserve verbatim with its mtime).
    let manifest_bytes_and_mtime = read_entry_with_mtime(&mut archive, "manifest.json");

    // Inventory log entries with their mtimes so we can sort newest-first.
    struct LogEntry {
        name: String,
        mtime: ZipDateTime,
        content: Vec<u8>,
    }
    let mut log_entries: Vec<LogEntry> = Vec::new();
    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else { continue };
        let name = entry.name().to_string();
        if !name.starts_with("logs/") {
            continue;
        }
        let mtime = entry.last_modified().unwrap_or_default();
        let mut content = Vec::new();
        if entry.read_to_end(&mut content).is_err() {
            continue;
        }
        log_entries.push(LogEntry { name, mtime, content });
    }
    // Newest first.
    log_entries.sort_by_key(|e| std::cmp::Reverse(e.mtime));

    // Headroom: leave 10% for the central directory plus per-entry overhead. Compressed
    // text is hard to predict from line count alone, but 10% has been reliable in the
    // 30 MB → 1 MB regression test.
    let target = (cap_bytes * 9) / 10;

    let mut out_buf: Vec<u8> = Vec::with_capacity(cap_bytes);
    let finish_result = {
        let cursor = std::io::Cursor::new(&mut out_buf);
        let mut writer = ZipWriter::new(cursor);

        // 1. Manifest, verbatim.
        if let Some((bytes, mtime)) = &manifest_bytes_and_mtime {
            let opts = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(*mtime);
            if writer.start_file("manifest.json", opts).is_err() || writer.write_all(bytes).is_err() {
                // Manifest write failed; bail to the original.
                return zip_bytes;
            }
        }

        // 2. Logs, newest-first. Pack lines from the end inward.
        //
        // We can't read `out_buf.len()` while the writer mutably borrows it, so we
        // budget against an *uncompressed* byte tally. Real log text deflates ~5–10×,
        // pathological pseudo-random text only ~1.1×; we pick a budget that lands the
        // compressed output near `target` in the worst case rather than the best.
        // Concretely: uncompressed_budget = target * 1.0. On real logs we'll be far
        // under the cap (acceptable: cap is a ceiling, not a quota). On worst-case
        // input we'll be just under the cap. The minimum-tail floor below covers the
        // pathological "every line still wouldn't fit" case.
        let uncompressed_budget = target;
        let mut uncompressed_used: usize = manifest_bytes_and_mtime.as_ref().map(|(b, _)| b.len()).unwrap_or(0);

        for (i, entry) in log_entries.iter().enumerate() {
            let lines: Vec<&[u8]> = split_into_lines(&entry.content);
            let remaining_budget = uncompressed_budget.saturating_sub(uncompressed_used);

            // Pick how many lines from the tail of this entry to keep.
            let kept_lines: Vec<&[u8]> = if remaining_budget == 0 {
                // No budget at all. Honor the minimum-tail floor for the newest entry only.
                if i == 0 {
                    take_tail(&lines, MIN_TAIL_LINES_OF_NEWEST_FILE)
                } else {
                    Vec::new()
                }
            } else {
                let mut kept = pick_tail_within_budget(&lines, remaining_budget);
                // Floor: ensure the newest file ships at least N lines (even if we'd
                // marginally exceed the cap (see the doc comment).
                if i == 0 && kept.len() < MIN_TAIL_LINES_OF_NEWEST_FILE.min(lines.len()) {
                    kept = take_tail(&lines, MIN_TAIL_LINES_OF_NEWEST_FILE);
                }
                kept
            };

            if kept_lines.is_empty() {
                continue;
            }

            let opts = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(entry.mtime);
            if writer.start_file(&entry.name, opts).is_err() {
                continue;
            }
            for line in &kept_lines {
                if writer.write_all(line).is_err() {
                    break;
                }
                if writer.write_all(b"\n").is_err() {
                    break;
                }
                uncompressed_used += line.len() + 1;
            }
        }

        writer.finish()
    };

    if finish_result.is_err() {
        return zip_bytes;
    }
    if out_buf.is_empty() {
        return zip_bytes;
    }
    out_buf
}

/// Split a log file's content into lines (without trailing newlines) so we can pack
/// them tail-first into the capped zip. Empty trailing slice is dropped; we add the
/// `\n` separator on write-out anyway.
fn split_into_lines(content: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> = content.split(|b| *b == b'\n').collect();
    if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines
}

/// Take the last `n` lines (or all of them if there are fewer).
fn take_tail<'a>(lines: &[&'a [u8]], n: usize) -> Vec<&'a [u8]> {
    let start = lines.len().saturating_sub(n);
    lines[start..].to_vec()
}

/// Pick the newest tail of `lines` whose **uncompressed** byte total fits within
/// `budget` bytes.
///
/// Uses a simple back-to-front scan rather than a binary search: cap-trimming runs once
/// per dispatch and the line count is bounded by the rotation cap. Each iteration is a
/// `len()` lookup. The result is the longest tail of `lines` whose summed lengths
/// (plus per-line `\n`) stay under `budget`.
///
/// Heuristic: assume worst-case 1:1 deflate ratio on the line bytes (log text deflates
/// to ~10–20% of source, so this is conservative). Headroom for the central directory
/// is the caller's concern via `target = cap * 9 / 10`.
fn pick_tail_within_budget<'a>(lines: &[&'a [u8]], budget: usize) -> Vec<&'a [u8]> {
    let mut total: usize = 0;
    let mut start = lines.len();
    for (i, line) in lines.iter().enumerate().rev() {
        let cost = line.len() + 1; // +1 for newline
        if total + cost > budget {
            break;
        }
        total += cost;
        start = i;
    }
    lines[start..].to_vec()
}

/// Reads an entry's bytes plus its stored mtime. `None` if the entry doesn't exist or
/// can't be read.
fn read_entry_with_mtime<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Option<(Vec<u8>, ZipDateTime)> {
    let mut entry = archive.by_name(name).ok()?;
    let mtime = entry.last_modified().unwrap_or_default();
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    Some((bytes, mtime))
}
