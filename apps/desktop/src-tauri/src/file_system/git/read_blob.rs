//! `VolumeReadStream` for git blobs.
//!
//! ## Honest streaming
//!
//! gix's `Object::data` is `Vec<u8>` for the whole blob — there's no
//! chunked loose-object reader exposed at the public surface in 0.81. So
//! this stream owns the full `Vec<u8>` and yields slices in 256 KB chunks.
//! Memory cost equals blob size; chunked yield is for the consumer API
//! shape, not memory streaming. We refuse blobs larger than `MAX_BLOB_BYTES`
//! up-front via `tree::read_blob`.
//!
//! Future work: revisit when gix exposes a chunked reader (see the
//! gitoxide tracking issues).

use std::future::Future;
use std::pin::Pin;

use crate::file_system::volume::{VolumeError, VolumeReadStream};

/// 256 KB — matches the consumer-friendly chunk size used by network read streams.
const CHUNK_SIZE: usize = 256 * 1024;

/// Streaming reader over an in-memory `Vec<u8>` blob.
pub struct GitBlobReadStream {
    data: Vec<u8>,
    total_size: u64,
    pos: usize,
}

impl GitBlobReadStream {
    pub fn new(data: Vec<u8>) -> Self {
        let total_size = data.len() as u64;
        Self {
            data,
            total_size,
            pos: 0,
        }
    }
}

impl VolumeReadStream for GitBlobReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.pos >= self.data.len() {
                return None;
            }
            let end = std::cmp::min(self.pos + CHUNK_SIZE, self.data.len());
            let chunk = self.data[self.pos..end].to_vec();
            self.pos = end;
            Some(Ok(chunk))
        })
    }

    fn total_size(&self) -> u64 {
        self.total_size
    }

    fn bytes_read(&self) -> u64 {
        self.pos as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn drain(stream: &mut GitBlobReadStream) -> Vec<u8> {
        let mut out = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            out.extend_from_slice(&chunk.unwrap());
        }
        out
    }

    #[tokio::test]
    async fn yields_whole_blob_in_chunks() {
        let data: Vec<u8> = (0..(CHUNK_SIZE * 2 + 17)).map(|i| (i % 256) as u8).collect();
        let mut stream = GitBlobReadStream::new(data.clone());
        assert_eq!(stream.total_size(), data.len() as u64);
        let drained = drain(&mut stream).await;
        assert_eq!(drained, data);
        assert_eq!(stream.bytes_read(), data.len() as u64);
    }

    #[tokio::test]
    async fn empty_blob_yields_no_chunks() {
        let mut stream = GitBlobReadStream::new(Vec::new());
        assert!(stream.next_chunk().await.is_none());
    }

    #[tokio::test]
    async fn small_blob_single_chunk() {
        let data = b"hello, world!".to_vec();
        let mut stream = GitBlobReadStream::new(data.clone());
        let chunk = stream.next_chunk().await.unwrap().unwrap();
        assert_eq!(chunk, data);
        assert!(stream.next_chunk().await.is_none());
    }
}
