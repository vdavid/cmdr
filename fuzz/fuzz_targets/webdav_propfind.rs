//! The WebDAV PROPFIND `multistatus` parser against server-chosen bytes.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| cmdr_webdav::fuzzing::propfind(data));
