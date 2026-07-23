//! Next-launch rendering of raw instruction pointer addresses from signal crashes.

/// Format raw crash addresses for the report.
///
/// These are absolute virtual addresses from the crashed process. ASLR randomizes the base
/// on every launch, so they can't be resolved against the current process's address space,
/// and the relaunched process's own slide is useless for the job.
///
/// What makes them usable is the `imageBase` field on the report, recorded by the crashing
/// process itself (see [`super::CrashReport::image_base`]): `frame - image_base` is a stable
/// per-build offset, so identical crash sites group across installs, and
/// `atos -o <binary> -l <image_base> <frame…>` resolves them wherever the matching build's
/// symbols are available.
///
/// We deliberately don't annotate per-frame offsets here. Most frames land in system
/// libraries (WebKit, AppKit) rather than the main image, and without each image's load
/// address and size we can't tell which is which; guessing would label system frames as
/// ours. Plain addresses plus the one authoritative base keeps the data honest.
pub fn symbolicate_addresses(addresses: &[u64]) -> Vec<String> {
    addresses.iter().map(|addr| format!("0x{addr:016x}")).collect()
}
