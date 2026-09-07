//! Styling for the target column of a terminal log line.
//!
//! A dev scanning `pnpm dev` output looks for the subsystem first, so the head of the
//! target (everything before the first `::`) gets a fixed-width column and a stable
//! color, and the module path trailing it fades to gray. Terminal only: the file chain
//! stays plain text so `cat` and the error-report bundles stay readable.

/// Width the head is padded to. `crash_reporter` is the longest head in the tree at
/// exactly 14, and a longer one just overflows and pushes its own message right, which
/// beats widening the column on every other line.
pub(super) const HEAD_WIDTH: usize = 14;

/// Closes any color opened earlier in the line.
pub(super) const RESET: &str = "\x1b[0m";

/// The module path after the head. A 256-color gray rather than SGR 2 (`dim`), which
/// several terminals ignore outright.
pub(super) const TAIL_COLOR: &str = "\x1b[38;5;245m";

/// Head colors as 256-color SGR sequences.
///
/// Mid-tones only: nothing dark enough to vanish on a dark background or light enough to
/// vanish on a light one. No reds, yellows, or greens, so a head never reads as the
/// level sitting in the column to its left (`ERROR` red, `WARN` yellow, `INFO` green).
const PALETTE: [&str; 16] = [
    "\x1b[38;5;33m",  // azure
    "\x1b[38;5;37m",  // teal
    "\x1b[38;5;63m",  // indigo
    "\x1b[38;5;67m",  // steel blue
    "\x1b[38;5;72m",  // sea green
    "\x1b[38;5;73m",  // cadet
    "\x1b[38;5;74m",  // sky
    "\x1b[38;5;98m",  // violet
    "\x1b[38;5;103m", // slate
    "\x1b[38;5;108m", // sage
    "\x1b[38;5;110m", // powder blue
    "\x1b[38;5;132m", // plum
    "\x1b[38;5;139m", // mauve
    "\x1b[38;5;141m", // lavender
    "\x1b[38;5;168m", // rose
    "\x1b[38;5;176m", // orchid
];

/// Splits a target into head and tail: `cmdr_index::indexing::watch` becomes
/// (`cmdr_index`, `::indexing::watch`). A target with no `::` is all head.
pub(super) fn split(target: &str) -> (&str, &str) {
    match target.find("::") {
        Some(separator) => target.split_at(separator),
        None => (target, ""),
    }
}

/// The color for a head, stable across runs and machines.
///
/// FNV-1a rather than `DefaultHasher`: std promises nothing about the latter's output
/// across releases, and a Rust upgrade silently repainting every subsystem would defeat
/// the point of a deterministic color.
pub(super) fn head_color(head: &str) -> &'static str {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET_BASIS;
    for byte in head.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    let bucket = hash % PALETTE.len() as u64;
    PALETTE[usize::try_from(bucket).unwrap_or(0)]
}

/// Whether the terminal chain should emit ANSI at all.
///
/// ❌ `is_terminal()` can't answer this alone under `pnpm dev`: the Tauri CLI spawns the
/// app with a piped stderr and forwards the bytes to its own, so from in here a real
/// terminal and `2> log.txt` look identical (verified 2026-09-07 against `@tauri-apps/cli`
/// 2.9.1, with `tauri dev --runner` on a probe reporting `isatty(2)`). `tauri-wrapper.ts`
/// still sees the real stderr and sets `CLICOLOR_FORCE`, the same move the Tauri CLI makes
/// when it hands cargo `--color always`.
pub(super) fn color_enabled() -> bool {
    use std::ffi::OsStr;
    use std::io::IsTerminal;

    let no_color = std::env::var_os("NO_COLOR");
    let no_color = no_color.as_deref().map(OsStr::to_string_lossy);
    let force = std::env::var_os("CLICOLOR_FORCE").or_else(|| std::env::var_os("FORCE_COLOR"));
    let force = force.as_deref().map(OsStr::to_string_lossy);
    resolve_color(no_color.as_deref(), force.as_deref(), std::io::stderr().is_terminal())
}

/// The decision behind [`color_enabled`], with the environment passed in so it's testable.
///
/// `no_color` is `NO_COLOR`, `force` the first of `CLICOLOR_FORCE` / `FORCE_COLOR` that's
/// set. An unset or empty force var means "no opinion", `0` means off, anything else on;
/// `NO_COLOR` outranks both.
fn resolve_color(no_color: Option<&str>, force: Option<&str>, stderr_is_terminal: bool) -> bool {
    if no_color.is_some_and(|value| !value.is_empty()) {
        return false;
    }
    match force {
        Some(value) if !value.is_empty() => value != "0",
        _ => stderr_is_terminal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_at_the_first_separator() {
        assert_eq!(
            split("cmdr_index::indexing::watch"),
            ("cmdr_index", "::indexing::watch")
        );
        assert_eq!(split("downloads::watcher"), ("downloads", "::watcher"));
    }

    #[test]
    fn a_target_without_a_separator_is_all_head() {
        assert_eq!(split("logging"), ("logging", ""));
        assert_eq!(split(""), ("", ""));
    }

    /// The whole promise of the feature: `logging` is the same color today, tomorrow, and
    /// on someone else's machine. Pinning two literal sequences also catches an
    /// accidental hash or palette-order change, which would silently repaint every
    /// subsystem a dev has learned to recognize.
    #[test]
    fn head_color_is_stable() {
        assert_eq!(head_color("logging"), "\x1b[38;5;168m");
        assert_eq!(head_color("cmdr_index"), "\x1b[38;5;103m");
        assert_eq!(head_color("logging"), head_color("logging"));
    }

    /// With nobody forcing anything, our own stderr decides.
    #[test]
    fn a_plain_environment_follows_stderr() {
        assert!(resolve_color(None, None, true));
        assert!(!resolve_color(None, None, false));
    }

    /// The `pnpm dev` case this exists for: the wrapper saw a real terminal and said so,
    /// even though the Tauri CLI handed us a pipe.
    #[test]
    fn a_force_var_wins_over_a_piped_stderr() {
        assert!(resolve_color(None, Some("1"), false));
    }

    /// `0` is the conventional way to say "no color" through the same variable, and it
    /// holds even on a real terminal.
    #[test]
    fn a_force_var_set_to_zero_turns_color_off() {
        assert!(!resolve_color(None, Some("0"), true));
    }

    /// An empty value carries no opinion, so stderr still decides.
    #[test]
    fn an_empty_force_var_is_no_opinion() {
        assert!(resolve_color(None, Some(""), true));
        assert!(!resolve_color(None, Some(""), false));
    }

    /// `NO_COLOR` outranks a force var and a terminal both: someone who set it wants no
    /// escape sequences, whatever else the environment says.
    #[test]
    fn no_color_outranks_everything() {
        assert!(!resolve_color(Some("1"), Some("1"), true));
        assert!(!resolve_color(Some("anything"), None, true));
    }

    /// An empty `NO_COLOR` isn't set, per the convention.
    #[test]
    fn an_empty_no_color_is_not_set() {
        assert!(resolve_color(Some(""), None, true));
    }

    /// Different heads generally land on different colors. Collisions are inevitable with
    /// 16 buckets, but the two most-logged subsystems sharing one would be a bad draw.
    #[test]
    fn busy_heads_do_not_all_collide() {
        let heads = [
            "logging",
            "cmdr_index",
            "media_index",
            "cmdr_smb",
            "file_system",
            "config",
        ];
        let mut colors: Vec<&str> = heads.iter().map(|h| head_color(h)).collect();
        colors.sort_unstable();
        colors.dedup();
        assert!(
            colors.len() >= 5,
            "expected the busy heads to spread across the palette"
        );
    }
}
