//! A volume token served in one go, the way a host with no blocking threads serves it: the
//! same answer and the same `Done` the app's stall-watched read gives.

use std::sync::Arc;

use super::ViewerError;
use super::media_serve::{MediaAnswer, ResolvedRange, serve_volume_inline};
use super::media_volume::{MediaLoad, MediaLoadStage, VolumeMedia};
use super::source::ByteSource;

struct Bytes(Vec<u8>);

impl ByteSource for Bytes {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, ViewerError> {
        let start = (offset as usize).min(self.0.len());
        let n = buf.len().min(self.0.len() - start);
        buf[..n].copy_from_slice(&self.0[start..start + n]);
        Ok(n)
    }
}

fn media(bytes: &[u8]) -> VolumeMedia {
    VolumeMedia::new(Arc::new(Bytes(bytes.to_vec())), bytes.len() as u64, "image/png".into())
}

#[test]
fn a_whole_file_answers_its_bytes_and_says_done() {
    let body: Vec<u8> = (0..=255u8).collect();
    let mut heard: Vec<MediaLoad> = Vec::new();
    let answer = serve_volume_inline(&media(&body), "tok", None, |load| heard.push(load));
    assert_eq!(
        answer,
        MediaAnswer::Body {
            mime: "image/png".into(),
            range: ResolvedRange {
                start: 0,
                end: 255,
                total: 256,
                is_partial: false,
            },
            body,
        }
    );
    assert_eq!(
        heard,
        vec![MediaLoad {
            token: "tok".into(),
            stage: MediaLoadStage::Done,
            bytes_done: 256,
            bytes_total: 256,
        }]
    );
}

#[test]
fn a_range_answers_its_slice_and_a_closed_token_answers_nothing() {
    let body: Vec<u8> = (0..=255u8).collect();
    let served = media(&body);
    let answer = serve_volume_inline(&served, "tok", Some("bytes=10-19"), |_| {});
    let MediaAnswer::Body { body: slice, range, .. } = answer else {
        panic!("a range is served");
    };
    assert_eq!(slice, &body[10..=19]);
    assert!(range.is_partial);

    served.close();
    let mut heard = Vec::new();
    assert_eq!(
        serve_volume_inline(&served, "tok", None, |load| heard.push(load)),
        MediaAnswer::NotFound
    );
    assert!(heard.is_empty(), "a closed session tells nobody");
}
