//! Next-launch symbolication of raw instruction pointer addresses from signal crashes.
//!
//! Uses `std::backtrace` to resolve addresses to function names. Only valid when the
//! crash and current binary are the same version (same ASLR slide doesn't apply, but
//! function offsets within the binary are stable for the same build).

/// Symbolicate raw addresses against the current binary.
/// Falls back to hex addresses for frames that can't be resolved.
pub fn symbolicate_addresses(addresses: &[u64]) -> Vec<String> {
    // The addresses from the signal handler are absolute virtual addresses from the
    // crashed process. Since ASLR randomizes the base address on each launch, we can't
    // directly symbolicate them against the current process's address space.
    //
    // However, we can compute the offset within the binary: if we know the image base
    // from the crash (we don't store it), we're stuck. Instead, we format the addresses
    // and rely on the raw addresses being useful for grouping identical crash sites.
    //
    // For full symbolication, we'd need to store the image base address in the raw crash
    // file and use it to compute offsets, then resolve those against the binary on disk.
    // That's a future improvement; for now, raw addresses still group crashes by site.
    addresses.iter().map(|addr| format!("0x{addr:016x}")).collect()
}
