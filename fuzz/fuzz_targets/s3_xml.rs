//! Every S3 response-body parser against server-chosen bytes.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| cmdr_s3::fuzzing::response_body(data));
