//! The download record's decoders (`com.apple.quarantine`, `kMDItemWhereFroms`,
//! `user.xdg.origin.url`) against bytes a download wrote.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| cmdr_about::fuzzing::provenance(data));
