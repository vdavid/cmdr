//! Direct SMB operations — uses the `smb` crate to talk SMB2/3 directly, bypassing the OS mount.

use crate::config::Target;
use futures_util::StreamExt;
use smb::{
    Client, ClientConfig, CreateDisposition, FileAccessMask, FileBothDirectoryInformation,
    FileDispositionInformation, GetLen, Resource, UncPath,
};
use std::io::Write;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Negotiated chunk sizes for direct SMB transfers.
pub struct ChunkSizes {
    pub read: usize,
    pub write: usize,
}

/// Default cap for negotiated sizes. Override per-target via `max_chunk_kb` in config.
const DEFAULT_MAX_CHUNK: u32 = 1024 * 1024; // 1 MB

/// Establish an SMB connection + session + tree for a target.
/// Returns the client, UNC path, and negotiated read/write chunk sizes.
/// Mirrors the desktop app's approach: connect_to_address with IP, then share_connect.
pub async fn connect(target: &Target) -> Result<(Client, UncPath, ChunkSizes), String> {
    let mut config = ClientConfig::default();
    if target.guest {
        config.connection.allow_unsigned_guest_access = true;
    }

    let client = Client::new(config);

    // Connect using IP:445 as the socket address, server_name = IP for UNC path consistency
    let server_addr = format!("{}:445", target.host);
    let socket_addr: std::net::SocketAddr = server_addr
        .parse()
        .map_err(|e| format!("Bad address '{server_addr}': {e}"))?;

    // Pre-establish TCP connection. The server_name here will be used for SPN.
    // Using the IP as server_name is fine for NTLM (SPN only matters for Kerberos).
    client
        .connect_to_address(&target.host, socket_addr)
        .await
        .map_err(|e| format!("connect_to_address: {e}"))?;

    // Build UNC path using the same server string (IP)
    let unc = format!(r"\\{}\{}", target.host, target.share);
    let unc_path = UncPath::from_str(&unc).map_err(|e| format!("Bad UNC path '{unc}': {e}"))?;

    let (username, password) = if target.guest {
        ("Guest".to_string(), String::new())
    } else {
        (
            target.username.clone().unwrap_or_default(),
            target.password.clone().unwrap_or_default(),
        )
    };

    client
        .share_connect(&unc_path, &username, password)
        .await
        .map_err(|e| format!("share_connect to {unc}: {e}"))?;

    // Query negotiated max read/write sizes from the connection
    let conn = client
        .get_connection(&target.host)
        .await
        .map_err(|e| format!("get_connection: {e}"))?;
    let conn_info = conn
        .conn_info()
        .ok_or("Connection not yet negotiated")?;
    let neg = &conn_info.negotiation;

    let cap = target
        .max_chunk_kb
        .map(|kb| kb * 1024)
        .unwrap_or(DEFAULT_MAX_CHUNK);
    let chunk_sizes = ChunkSizes {
        read: std::cmp::min(neg.max_read_size, cap) as usize,
        write: std::cmp::min(neg.max_write_size, cap) as usize,
    };
    println!(
        "  Negotiated sizes: max_read={}KB, max_write={}KB (capped to {}KB)",
        neg.max_read_size / 1024,
        neg.max_write_size / 1024,
        cap / 1024,
    );

    Ok((client, unc_path, chunk_sizes))
}

/// Create the test directory on the share via direct SMB.
pub async fn setup(client: &Client, unc_path: &UncPath, cycle_id: &str) -> Result<String, String> {
    let base = Target::direct_test_base();
    let test_dir = format!(r"{base}\{cycle_id}");

    // Try to delete existing test dir first (best-effort)
    let _ = delete_dir_recursive(client, unc_path, &test_dir).await;

    let tree = client
        .get_tree(unc_path)
        .await
        .map_err(|e| format!("get_tree: {e}"))?;

    let access = FileAccessMask::new()
        .with_generic_read(true)
        .with_generic_write(true);

    // Create base dir if needed
    let _ = tree
        .create_directory(base, CreateDisposition::OpenIf, access)
        .await;

    // Create cycle-specific subdir
    let resource = tree
        .create_directory(&test_dir, CreateDisposition::Create, access)
        .await
        .map_err(|e| format!("create_directory '{test_dir}': {e}"))?;

    drop(resource);
    Ok(test_dir)
}

/// Upload `count` files of `size` bytes each via direct SMB. Returns elapsed time.
pub async fn upload(
    client: &Client,
    unc_path: &UncPath,
    test_dir: &str,
    count: usize,
    size: usize,
    data: &[u8],
    write_chunk: usize,
) -> Duration {
    let tree = client.get_tree(unc_path).await.expect("get_tree");

    let start = Instant::now();
    for i in 0..count {
        let file_path = format!(r"{test_dir}\bench_{i:05}.dat");
        let access = FileAccessMask::new().with_generic_write(true);
        let resource = tree
            .create_file(&file_path, CreateDisposition::Create, access)
            .await
            .expect("create_file");

        let file = resource.unwrap_file();
        let mut offset = 0u64;
        while (offset as usize) < size {
            let end = std::cmp::min(offset as usize + write_chunk, size);
            let chunk = &data[offset as usize..end];
            file.write_block(chunk, offset, None)
                .await
                .expect("write_block");
            offset = end as u64;
        }
        drop(file);
    }
    start.elapsed()
}

/// List the test directory via direct SMB. Returns (count, elapsed).
pub async fn list(client: &Client, unc_path: &UncPath, test_dir: &str) -> (usize, Duration) {
    let tree = client.get_tree(unc_path).await.expect("get_tree");

    let access = FileAccessMask::new().with_generic_read(true);
    let resource = tree
        .open_existing(test_dir, access)
        .await
        .expect("open test dir");
    let dir = Arc::new(resource.unwrap_dir());

    let start = Instant::now();
    let entries = collect_dir_entries(&dir, "*").await;
    let count = entries
        .iter()
        .filter(|e| {
            let name = e.file_name.to_string();
            name != "." && name != ".."
        })
        .count();
    let elapsed = start.elapsed();

    drop(dir);
    (count, elapsed)
}

/// Download all files to a local temp dir via direct SMB. Returns (bytes, elapsed).
pub async fn download(
    client: &Client,
    unc_path: &UncPath,
    test_dir: &str,
    local_dest: &std::path::PathBuf,
    read_chunk: usize,
) -> (u64, Duration) {
    std::fs::create_dir_all(local_dest).expect("create local dest");
    let tree = client.get_tree(unc_path).await.expect("get_tree");

    // First, list files to know what to download
    let access = FileAccessMask::new().with_generic_read(true);
    let resource = tree.open_existing(test_dir, access).await.expect("open test dir");
    let dir = Arc::new(resource.unwrap_dir());
    let entries = collect_dir_entries(&dir, "*").await;
    let file_names: Vec<String> = entries
        .iter()
        .filter(|e| {
            let name = e.file_name.to_string();
            name != "." && name != ".."
        })
        .map(|e| e.file_name.to_string())
        .collect();
    drop(dir);

    let start = Instant::now();
    let mut total_bytes = 0u64;
    for name in &file_names {
        let file_path = format!(r"{test_dir}\{name}");
        let access = FileAccessMask::new().with_generic_read(true);
        let resource = tree
            .open_existing(&file_path, access)
            .await
            .expect("open file");
        let file = resource.unwrap_file();

        let file_size = file.get_len().await.expect("get file length");
        let local_path = local_dest.join(name);
        let mut local_file = std::fs::File::create(&local_path).expect("create local file");

        let mut offset = 0u64;
        while offset < file_size {
            let to_read = std::cmp::min(read_chunk, (file_size - offset) as usize);
            let mut buf = vec![0u8; to_read];
            let n = file
                .read_block(&mut buf, offset, None, false)
                .await
                .expect("read_block");
            local_file.write_all(&buf[..n]).expect("write local");
            offset += n as u64;
            total_bytes += n as u64;
        }
        drop(file);
    }
    (total_bytes, start.elapsed())
}

/// Delete all files in the test directory and then the directory itself via direct SMB.
pub async fn delete(client: &Client, unc_path: &UncPath, test_dir: &str) -> Duration {
    let start = Instant::now();
    delete_dir_recursive(client, unc_path, test_dir)
        .await
        .expect("delete_dir_recursive");
    start.elapsed()
}

/// Recursively delete a directory and its contents via SMB.
async fn delete_dir_recursive(
    client: &Client,
    unc_path: &UncPath,
    dir_name: &str,
) -> Result<(), String> {
    let tree = client
        .get_tree(unc_path)
        .await
        .map_err(|e| format!("get_tree: {e}"))?;

    // List directory contents
    let access = FileAccessMask::new().with_generic_read(true);
    let resource = tree
        .open_existing(dir_name, access)
        .await
        .map_err(|e| format!("open '{dir_name}': {e}"))?;
    let dir = Arc::new(resource.unwrap_dir());
    let entries = collect_dir_entries(&dir, "*").await;
    drop(dir);

    // Delete each file: open with DELETE access, set disposition, close
    for entry in &entries {
        let name = entry.file_name.to_string();
        if name == "." || name == ".." {
            continue;
        }
        let path = format!(r"{dir_name}\{name}");
        let access = FileAccessMask::new().with_delete(true);
        let resource = tree
            .open_existing(&path, access)
            .await
            .map_err(|e| format!("open '{path}' for delete: {e}"))?;

        set_delete_disposition(&resource).await?;
        drop(resource);
    }

    // Delete the directory itself
    let access = FileAccessMask::new().with_delete(true);
    let resource = tree
        .open_existing(dir_name, access)
        .await
        .map_err(|e| format!("open dir '{dir_name}' for delete: {e}"))?;
    set_delete_disposition(&resource).await?;
    drop(resource);

    Ok(())
}

/// Set delete-on-close disposition on a resource handle.
async fn set_delete_disposition(resource: &Resource) -> Result<(), String> {
    let handle = match resource {
        Resource::File(f) => f.handle(),
        Resource::Directory(d) => d.handle(),
        Resource::Pipe(p) => p.handle(),
    };
    handle
        .set_info(FileDispositionInformation::default())
        .await
        .map_err(|e| format!("set_info(FileDisposition): {e}"))
}

/// Collect all directory entries from the async stream into a Vec.
async fn collect_dir_entries(
    dir: &Arc<smb::Directory>,
    pattern: &str,
) -> Vec<FileBothDirectoryInformation> {
    let stream = smb::Directory::query::<FileBothDirectoryInformation>(dir, pattern)
        .await
        .expect("query directory");
    let results: Vec<Result<FileBothDirectoryInformation, _>> = stream.collect().await;
    results
        .into_iter()
        .map(|r| r.expect("dir entry"))
        .collect()
}

/// Remove test base directory if it exists (for --cleanup-only).
pub async fn cleanup(target: &Target) {
    match connect(target).await {
        Ok((client, unc_path, _chunks)) => {
            let base = Target::direct_test_base();
            match delete_dir_recursive(&client, &unc_path, base).await {
                Ok(()) => println!("  Removed {base} (direct)"),
                Err(e) => println!("  Direct cleanup: {e} (may not exist)"),
            }
        }
        Err(e) => println!("  Can't connect for direct cleanup: {e}"),
    }
}
