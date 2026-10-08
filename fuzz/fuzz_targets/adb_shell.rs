//! The ADB shell v2 frame reader and the `df -k` parser against device-chosen bytes.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| cmdr_adb::fuzzing::shell_v2(data));
