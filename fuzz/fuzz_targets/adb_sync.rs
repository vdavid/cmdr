//! The ADB `sync:` reader (stat, listing, pull) against device-chosen bytes.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| cmdr_adb::fuzzing::sync_session(data));
