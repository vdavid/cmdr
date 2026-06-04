//! The `cmdr://indexing` resource builder.
//!
//! Formats the drive-indexing debug status (current phase, timeline history, DB
//! stats) as human-readable plain text for the MCP resource.

use crate::search::format_size;

/// Format a duration in milliseconds as a human-readable string.
pub fn format_duration_human(ms: u64) -> String {
    if ms < 1_000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        let secs = ms as f64 / 1000.0;
        format!("{:.1}s", secs)
    } else if ms < 3_600_000 {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1000;
        if secs == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m {:02}s", mins, secs)
        }
    } else {
        let hours = ms / 3_600_000;
        let mins = (ms % 3_600_000) / 60_000;
        format!("{}h {:02}m", hours, mins)
    }
}

/// Build a plain-text summary of the indexing status for the MCP resource.
pub fn build_indexing_status_text() -> String {
    let status = match crate::indexing::get_debug_status() {
        Ok(s) => s,
        Err(e) => return format!("Couldn't read indexing status: {e}"),
    };

    let mut lines = Vec::new();

    // Current phase
    let duration_str = format_duration_human(status.phase_duration_ms);
    lines.push(format!("Phase: {} ({})", status.activity_phase, duration_str));

    // Trigger
    if let Some(last) = status.phase_history.last()
        && !last.trigger.is_empty()
    {
        lines.push(format!("Trigger: {}", last.trigger));
    }

    // Verifying
    lines.push(format!("Verifying: {}", if status.verifying { "yes" } else { "no" }));

    // Watcher + live events
    lines.push(format!(
        "Watcher: {}, {} live events",
        if status.watcher_active { "on" } else { "off" },
        status.live_event_count,
    ));

    // DB stats
    if let (Some(entries), Some(dirs)) = (status.live_entry_count, status.live_dir_count) {
        let total_size_str = status
            .base
            .db_file_size
            .map(|s| format!(", {}", format_size(s)))
            .unwrap_or_default();
        lines.push(format!(
            "DB: {} entries, {} dirs{}",
            format_number(entries),
            format_number(dirs),
            total_size_str
        ));

        // Breakdown: main + WAL + pages
        let mut breakdown = Vec::new();
        if let Some(main) = status.db_main_size {
            breakdown.push(format!("main: {}", format_size(main)));
        }
        if let Some(wal) = status.db_wal_size
            && wal > 0
        {
            breakdown.push(format!("WAL: {}", format_size(wal)));
        }
        if let (Some(pages), Some(free)) = (status.db_page_count, status.db_freelist_count)
            && free > 0
        {
            breakdown.push(format!("{} pages, {} free", format_number(pages), format_number(free)));
        }
        if !breakdown.is_empty() {
            lines.push(format!("    ({})", breakdown.join(", ")));
        }
    }

    // Phase history
    if status.phase_history.len() > 1
        || (status.phase_history.len() == 1 && status.phase_history[0].duration_ms.is_some())
    {
        lines.push(String::new());
        lines.push("History:".to_string());
        for (i, record) in status.phase_history.iter().enumerate() {
            let is_current = i == status.phase_history.len() - 1 && record.duration_ms.is_none();
            let duration_str = match record.duration_ms {
                Some(ms) => format!("{:>8}", format_duration_human(ms)),
                None => format!("{:>8}", format_duration_human(status.phase_duration_ms)),
            };
            let phase_name = format!("{:<14}", record.phase.to_string());
            let mut line = format!("  {}  {} {}", record.started_at, phase_name, duration_str);

            // Append stats summary
            if !record.stats.is_empty() {
                let stats_str: Vec<String> = record.stats.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                line.push_str(&format!("  {}", stats_str.join(", ")));
            }

            if is_current {
                line.push_str("  <- now");
            }

            lines.push(line);
        }
    }

    lines.join("\n")
}

/// Format a number with comma separators.
pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
