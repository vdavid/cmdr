//! The two reads the app runs on any image a user opens, with no panic containment:
//! the header-only dimensions (`file_viewer::media::read_image_dimensions`) and the
//! EXIF block, shaped through `display_value` (`agent::tools::read::inspect::exif`).
//! Mirrored here because fuzzing the app crate itself would instrument all of Tauri;
//! keep the calls in step with those two functions.
#![no_main]

use std::io::Cursor;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(reader) = image::ImageReader::new(Cursor::new(data)).with_guessed_format() {
        let _ = reader.into_dimensions();
    }
    if let Ok(parsed) = exif::Reader::new().read_from_container(&mut Cursor::new(data)) {
        for field in parsed.fields() {
            let _ = field.display_value().with_unit(&parsed).to_string();
            let _ = field.value.get_uint(0);
        }
    }
});
