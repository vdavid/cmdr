//! Everything the sync service has no verb for: `mkdir`, `rm`, `mv`, `df`.
//!
//! Only `shell,v2,raw:` is spoken. It frames stdout, stderr, and the exit code
//! as `[id: u8][len: u32 LE][payload]` (ids: 0 stdin, 1 stdout, 2 stderr,
//! 3 exit with a one-byte payload). The legacy `shell:` service has no exit
//! code, and a device without `shell_v2` is refused at connect
//! (`AdbConnectError::DeviceTooOld`) rather than second-guessed from its
//! output. Wire reference: `adb/shell_protocol.h` (verified against
//! platform-tools 35, 2026-09).

use crate::errors::AdbError;
use crate::features::connect_as_transport;
use crate::server::AdbEndpoint;

/// Frame ids of the shell v2 protocol.
const ID_STDOUT: u8 = 1;
const ID_STDERR: u8 = 2;
const ID_EXIT: u8 = 3;

/// What one shell command produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellOutcome {
    /// The process's exit status (0 is success; 127 is "no such command").
    pub exit_code: u8,
    /// Everything it wrote to stdout.
    pub stdout: Vec<u8>,
    /// Everything it wrote to stderr. A log diagnostic; ❌ never classify on it.
    pub stderr: Vec<u8>,
}

impl ShellOutcome {
    /// Whether the command exited 0.
    pub fn succeeded(&self) -> bool {
        self.exit_code == 0
    }

    /// stdout as text, lossily.
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// stderr as text, lossily.
    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Single-quotes `arg` for a POSIX shell: `'` becomes `'\''`, everything else
/// is literal inside the quotes.
pub fn quote(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('\'');
    for c in arg.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Quotes every argument and joins them with spaces.
pub fn command_line(argv: &[&str]) -> String {
    argv.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
}

/// Runs `argv` on the device and collects its output and exit code.
pub async fn run(endpoint: &AdbEndpoint, serial: &str, argv: &[&str]) -> Result<ShellOutcome, AdbError> {
    let mut conn = endpoint.connect().await.map_err(connect_as_transport)?;
    conn.bind_device(serial).await?;
    conn.request(&format!("shell,v2,raw:{}", command_line(argv))).await?;
    let mut outcome = ShellOutcome {
        exit_code: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    loop {
        let mut id = [0u8; 1];
        conn.read_exact(&mut id).await?;
        let len = conn.read_u32_le().await? as usize;
        let mut payload = vec![0u8; len];
        conn.read_exact(&mut payload).await?;
        match id[0] {
            ID_STDOUT => outcome.stdout.extend_from_slice(&payload),
            ID_STDERR => outcome.stderr.extend_from_slice(&payload),
            ID_EXIT => {
                outcome.exit_code = payload.first().copied().unwrap_or(0);
                conn.shutdown().await;
                return Ok(outcome);
            }
            // Window-size and close-stdin frames flow the other way; anything
            // else is tolerated and skipped so a newer adbd can't wedge us.
            _ => {}
        }
    }
}

/// What `df -k` said about a filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpaceParts {
    /// Total size in bytes.
    pub total_bytes: u64,
    /// Bytes available to the caller.
    pub available_bytes: u64,
}

/// Parses `df -k <path>` output: the numbers on the last data line, in 1 KiB
/// blocks, as `total used available`.
///
/// Robust to toybox and busybox column layouts, and to busybox wrapping a long
/// device name onto its own line: every line after the header is tokenized and
/// the first three numeric tokens are read, so column widths and the `Use%`
/// token don't matter. `None` when there aren't three numbers.
pub fn parse_df_k(stdout: &str) -> Option<SpaceParts> {
    let numbers: Vec<u64> = stdout
        .lines()
        .skip(1)
        .flat_map(str::split_whitespace)
        .filter_map(|tok| tok.parse::<u64>().ok())
        .collect();
    let [total, _used, available, ..] = numbers[..] else {
        return None;
    };
    Some(SpaceParts {
        total_bytes: total.saturating_mul(1024),
        available_bytes: available.saturating_mul(1024),
    })
}

#[cfg(test)]
#[path = "shell_test.rs"]
mod shell_test;
