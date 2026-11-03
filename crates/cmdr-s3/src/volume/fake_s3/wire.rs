//! The fake's wire side: reading one request off a connection, and the answer
//! the scripted [`World`] gives it. The knobs a cell turns live in the parent.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison as _;
use percent_encoding::percent_decode_str;
use tokio::io::AsyncReadExt;

use super::{BUCKET, GetBodies, Stored, Upload, World};

const NOT_FOUND: &str = "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";

fn ok_xml(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/xml\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn error(status: &str, code: &str) -> String {
    let body = format!("<Error><Code>{code}</Code><Message>m</Message></Error>");
    format!(
        "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// One query parameter's decoded value; a valueless one answers `""`.
fn param(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (key == name).then(|| percent_decode_str(value).decode_utf8_lossy().into_owned())
    })
}

/// A request header's value, by case-insensitive name.
fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn meta_lines(head: &str) -> Vec<String> {
    head.lines()
        .filter(|l| l.to_ascii_lowercase().starts_with("x-amz-meta-"))
        .map(str::to_string)
        .collect()
}

/// What the fake says to one request; `None` hangs up without an answer.
pub(super) async fn answer(
    world: &Mutex<World>,
    head: &str,
    body: &[u8],
    length: usize,
    answer_after: Duration,
) -> Option<String> {
    let body_len = body.len();
    let mut line = head.lines().next().unwrap_or_default().split(' ');
    let method = line.next().unwrap_or_default().to_string();
    let target = line.next().unwrap_or_default().to_string();
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let key = path
        .strip_prefix(&format!("/{BUCKET}/"))
        .map(|key| percent_decode_str(key).decode_utf8_lossy().into_owned());
    if let Some(key) = &key {
        let mut world = world.lock_ignore_poison();
        world.keys_seen.push(key.clone());
        if method == "PUT" && body_len == length && header(head, "x-amz-copy-source").is_none() {
            world.puts += 1;
        }
    }
    let upload_id = param(query, "uploadId");
    let response = match (method.as_str(), key, upload_id) {
        ("PUT", Some(key), Some(id)) if body_len == length => {
            let mut world = world.lock_ignore_poison();
            if world.taken_before_next_part.as_deref() == Some(key.as_str()) {
                world.taken_before_next_part = None;
                world.writes += 1;
                let etag = format!("\"theirs{}\"", world.writes);
                world.objects.insert(
                    key.clone(),
                    Stored {
                        len: 6,
                        etag,
                        meta: Vec::new(),
                    },
                );
                let ended: Vec<String> = world
                    .uploads
                    .iter()
                    .filter(|(_, upload)| upload.key == key)
                    .map(|(id, _)| id.clone())
                    .collect();
                for ended_id in ended {
                    world.uploads.remove(&ended_id);
                    world.ended_uploads.push(ended_id);
                }
            }
            if world.ended_uploads.contains(&id) {
                return None;
            }
            let number: u32 = param(query, "partNumber").and_then(|n| n.parse().ok()).unwrap_or(0);
            // `UploadPartCopy`: the part is the source range, not a body.
            let copied = header(head, "x-amz-copy-source").map(|_| {
                header(head, "x-amz-copy-source-range")
                    .and_then(|range| range.strip_prefix("bytes="))
                    .and_then(|range| range.split_once('-'))
                    .and_then(|(first, last)| Some(last.parse::<usize>().ok()? - first.parse::<usize>().ok()? + 1))
                    .unwrap_or(0)
            });
            match world.uploads.get_mut(&id) {
                Some(upload) => {
                    upload.parts.insert(number, copied.unwrap_or(body_len));
                    match copied {
                        Some(_) => ok_xml(&format!("<CopyPartResult><ETag>\"p{number}\"</ETag></CopyPartResult>")),
                        None => format!(
                            "HTTP/1.1 200 OK\r\netag: \"p{number}\"\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                        ),
                    }
                }
                None => error("404 Not Found", "NoSuchUpload"),
            }
        }
        ("PUT", Some(_), Some(_)) => return None,
        // `CopyObject`: the source's bytes at the key, with the request's
        // metadata under `REPLACE` and the source's under `COPY`.
        ("PUT", Some(key), None) if header(head, "x-amz-copy-source").is_some() => {
            let source = header(head, "x-amz-copy-source")
                .and_then(|source| source.strip_prefix(&format!("/{BUCKET}/")))
                .map(|key| percent_decode_str(key).decode_utf8_lossy().into_owned())
                .unwrap_or_default();
            let replace = header(head, "x-amz-metadata-directive") == Some("REPLACE");
            let (etag, hang) = {
                let mut world = world.lock_ignore_poison();
                let Some(from) = world.objects.get(&source).cloned() else {
                    return Some(error("404 Not Found", "NoSuchKey"));
                };
                let pin_fails = header(head, "x-amz-copy-source-if-match").is_some_and(|pin| pin != from.etag);
                let taken =
                    header(head, "cf-copy-destination-if-none-match") == Some("*") && world.objects.contains_key(&key);
                if pin_fails || taken {
                    return Some(error("412 Precondition Failed", "PreconditionFailed"));
                }
                world.writes += 1;
                let etag = format!("\"c{}\"", world.writes);
                let meta = if replace { meta_lines(head) } else { from.meta };
                world.objects.insert(
                    key,
                    Stored {
                        len: from.len,
                        etag: etag.clone(),
                        meta,
                    },
                );
                (etag, world.hang_up_after_commit)
            };
            if hang {
                return None;
            }
            ok_xml(&format!(
                "<CopyObjectResult><ETag>{etag}</ETag><LastModified>2026-10-02T10:00:00.000Z</LastModified></CopyObjectResult>"
            ))
        }
        ("PUT", Some(key), None) if body_len < length => {
            let mut world = world.lock_ignore_poison();
            if world.keep_cut_off_bodies {
                world.writes += 1;
                let etag = format!("\"v{}\"", world.writes);
                world.objects.insert(
                    key,
                    Stored {
                        len: body_len,
                        etag,
                        meta: meta_lines(head),
                    },
                );
            }
            // Cut off: a well-behaved S3 publishes nothing.
            return None;
        }
        ("PUT", Some(key), None) => {
            let (etag, hang) = {
                let mut world = world.lock_ignore_poison();
                if header(head, "if-none-match") == Some("*") && world.objects.contains_key(&key) {
                    return Some(error("412 Precondition Failed", "PreconditionFailed"));
                }
                let fault = world.fault_puts > 0;
                if fault {
                    world.fault_puts -= 1;
                    if !world.fault_puts_after_commit {
                        return Some(error("500 Internal Server Error", "InternalError"));
                    }
                    world.fault_puts_after_commit = false;
                }
                world.writes += 1;
                let etag = format!("\"v{}\"", world.writes);
                world.objects.insert(
                    key,
                    Stored {
                        len: body_len,
                        etag: etag.clone(),
                        meta: meta_lines(head),
                    },
                );
                if fault {
                    return Some(error("500 Internal Server Error", "InternalError"));
                }
                (etag, world.hang_up_after_commit)
            };
            if hang {
                return None;
            }
            tokio::time::sleep(answer_after).await;
            format!("HTTP/1.1 200 OK\r\netag: {etag}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
        }
        ("POST", Some(key), None) if param(query, "uploads").is_some() => {
            let mut world = world.lock_ignore_poison();
            world.writes += 1;
            let id = format!("up{}", world.writes);
            world.uploads.insert(
                id.clone(),
                Upload {
                    key: key.clone(),
                    meta: meta_lines(head),
                    parts: BTreeMap::new(),
                },
            );
            ok_xml(&format!(
                "<InitiateMultipartUploadResult><Bucket>{BUCKET}</Bucket><Key>{key}</Key><UploadId>{id}</UploadId></InitiateMultipartUploadResult>"
            ))
        }
        ("POST", Some(key), Some(id)) => {
            let mut world = world.lock_ignore_poison();
            if world.hang_up_before_completing {
                return None;
            }
            let Some(upload) = world.uploads.remove(&id) else {
                return Some(error("404 Not Found", "NoSuchUpload"));
            };
            let etag = format!("\"m{}-{}\"", world.writes, upload.parts.len());
            world.objects.insert(
                key.clone(),
                Stored {
                    len: upload.parts.values().sum(),
                    etag: etag.clone(),
                    meta: upload.meta,
                },
            );
            if world.hang_up_after_commit {
                return None;
            }
            ok_xml(&format!(
                "<CompleteMultipartUploadResult><Bucket>{BUCKET}</Bucket><Key>{key}</Key><ETag>{etag}</ETag></CompleteMultipartUploadResult>"
            ))
        }
        ("DELETE", Some(_), Some(id)) => match world.lock_ignore_poison().uploads.remove(&id) {
            Some(_) => "HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n".into(),
            None => error("404 Not Found", "NoSuchUpload"),
        },
        // `ListParts`, answered with no parts: only whether the upload exists.
        ("GET", Some(key), Some(id)) => match world.lock_ignore_poison().uploads.get(&id) {
            Some(_) => ok_xml(&format!(
                "<ListPartsResult><Bucket>{BUCKET}</Bucket><Key>{key}</Key><UploadId>{id}</UploadId><IsTruncated>false</IsTruncated></ListPartsResult>"
            )),
            None => error("404 Not Found", "NoSuchUpload"),
        },
        ("HEAD", Some(key), None) => match head_then_maybe_replace(world, &key) {
            Some(stored) => format!(
                "HTTP/1.1 200 OK\r\n{}etag: {}\r\nlast-modified: Fri, 02 Oct 2026 10:00:00 GMT\r\n{}connection: close\r\n\r\n",
                if world.lock_ignore_poison().heads_without_length {
                    String::new()
                } else {
                    format!("content-length: {}\r\n", stored.len)
                },
                stored.etag,
                stored.meta.iter().map(|m| format!("{m}\r\n")).collect::<String>()
            ),
            None => NOT_FOUND.into(),
        },
        ("GET", Some(key), None) => {
            let world = world.lock_ignore_poison();
            match (world.gets, world.objects.get(&key)) {
                (Some(gets), Some(stored)) => object_body(gets, stored.len, header(head, "range")),
                (Some(_), None) => NOT_FOUND.into(),
                (None, _) => "HTTP/1.1 501 Not Implemented\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".into(),
            }
        }
        ("DELETE", Some(key), None) => {
            world.lock_ignore_poison().objects.remove(&key);
            "HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n".into()
        }
        ("GET", None, None) if param(query, "uploads").is_some() => {
            let open = world.lock_ignore_poison().upload_listings_open.clone();
            if let Some(mut open) = open
                && !*open.borrow()
            {
                world.lock_ignore_poison().held_upload_listings += 1;
                let _ = open.wait_for(|open| *open).await;
                world.lock_ignore_poison().held_upload_listings -= 1;
            }
            let world = world.lock_ignore_poison();
            let uploads: String = world
                .uploads
                .iter()
                .map(|(id, upload)| format!("<Upload><Key>{}</Key><UploadId>{id}</UploadId></Upload>", upload.key))
                .collect();
            ok_xml(&format!(
                "<ListMultipartUploadsResult><Bucket>{BUCKET}</Bucket><IsTruncated>false</IsTruncated>{uploads}</ListMultipartUploadsResult>"
            ))
        }
        ("GET", None, None) if query.contains("list-type=2") => list(world, query),
        ("POST", None, None) if param(query, "delete").is_some() => batch_delete(world, body),
        _ => "HTTP/1.1 501 Not Implemented\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".into(),
    };
    Some(response)
}

/// An object GET of `len` bytes as [`GetBodies`] shapes it, from the start of
/// a `Range: bytes=<start>-` header when there is one.
fn object_body(gets: GetBodies, len: usize, range: Option<&str>) -> String {
    let start = range
        .and_then(|range| range.strip_prefix("bytes="))
        .and_then(|range| range.strip_suffix('-'))
        .and_then(|start| start.parse::<usize>().ok());
    let (status, content_range, from) = match start {
        Some(start) => (
            "206 Partial Content",
            format!("content-range: bytes {start}-{}/{len}\r\n", len.saturating_sub(1)),
            start,
        ),
        None => ("200 OK", String::new(), 0),
    };
    let sent = len.saturating_sub(from).min(gets.cut_after.unwrap_or(usize::MAX));
    format!(
        "HTTP/1.1 {status}\r\n{content_range}connection: close\r\n\r\n{}",
        "x".repeat(sent)
    )
}

/// The object at `key` as a HEAD sees it, replacing it afterwards when a cell
/// asked for that ([`super::FakeS3::replace_after_head`]).
fn head_then_maybe_replace(world: &Mutex<World>, key: &str) -> Option<Stored> {
    let mut world = world.lock_ignore_poison();
    let seen = world.objects.get(key).cloned()?;
    if world.replace_after_head.as_deref() == Some(key) {
        world.replace_after_head = None;
        world.writes += 1;
        let etag = format!("\"r{}\"", world.writes);
        world.objects.insert(
            key.to_string(),
            Stored {
                len: seen.len,
                etag,
                meta: Vec::new(),
            },
        );
    }
    Some(seen)
}

/// `ListObjectsV2`, every match as `Contents` (no delimiter folding), and ❗
/// B2's refusal of a prefix no key could carry.
fn list(world: &Mutex<World>, query: &str) -> String {
    let prefix = param(query, "prefix").unwrap_or_default();
    let mut world = world.lock_ignore_poison();
    world.listed.push(prefix.clone());
    if prefix.len() > 1024 {
        return error("400 Bad Request", "InvalidRequest");
    }
    let contents: String = world
        .objects
        .iter()
        .filter(|(key, _)| key.starts_with(&prefix))
        .map(|(key, stored)| {
            format!(
                "<Contents><Key>{key}</Key><Size>{}</Size><ETag>{}</ETag><LastModified>2026-10-02T10:00:00.000Z</LastModified></Contents>",
                stored.len, stored.etag
            )
        })
        .collect();
    ok_xml(&format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult><Name>{BUCKET}</Name><Prefix>{prefix}</Prefix><IsTruncated>false</IsTruncated>{contents}</ListBucketResult>"
    ))
}

/// `DeleteObjects`: every `<Key>` in the body removed, a quiet (empty)
/// `DeleteResult`, unless a throttle was scripted.
fn batch_delete(world: &Mutex<World>, body: &[u8]) -> String {
    let mut world = world.lock_ignore_poison();
    if world.throttle_batch_deletes > 0 {
        world.throttle_batch_deletes -= 1;
        return error("503 Service Unavailable", "SlowDown");
    }
    world.batch_deletes += 1;
    let text = String::from_utf8_lossy(body);
    for piece in text.split("<Key>").skip(1) {
        if let Some((key, _)) = piece.split_once("</Key>") {
            world.objects.remove(key);
        }
    }
    ok_xml("<DeleteResult></DeleteResult>")
}

/// The request head, the body bytes that arrived, and the `Content-Length`.
pub(super) async fn read_request(
    socket: &mut tokio::net::TcpStream,
    world: &Mutex<World>,
) -> Option<(String, Vec<u8>, usize)> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 65536];
    let head_end = loop {
        let read = socket.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let length: usize = head
        .lines()
        .skip(1)
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())?
        })
        .unwrap_or(0);
    // The body is kept for the small requests that carry one to read
    // (`DeleteObjects`); an upload's bytes are only counted.
    let mut body = buffer[head_end..].to_vec();
    let mut body_len = body.len();
    world.lock_ignore_poison().body_bytes += body_len;
    while body_len < length {
        let rate = world.lock_ignore_poison().read_rate;
        // A small read under a rate, so the pace holds piece by piece.
        let want = if rate.is_some() { 16 * 1024 } else { chunk.len() };
        match socket.read(&mut chunk[..want]).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                world.lock_ignore_poison().body_bytes += read;
                if let Some(rate) = rate {
                    // allowed-test-sleep: the pace IS this fake's slow uplink.
                    tokio::time::sleep(Duration::from_secs_f64(read as f64 / rate as f64)).await;
                }
                body_len += read;
                if body.len() < 1 << 20 {
                    body.extend_from_slice(&chunk[..read]);
                }
            }
        }
    }
    body.resize(body_len, 0);
    Some((head, body, length))
}
