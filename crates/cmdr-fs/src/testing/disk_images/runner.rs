//! The guarded runner: the closed list of `hdiutil`, `diskutil`, and `umount` calls
//! the harness may make, each under a SIGKILL deadline, with every call that changes
//! a disk gated on an ownership check.

use std::io::{self, Read, Seek};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::HarnessError;
use super::facts::{Refusal, Target};

/// How long one `hdiutil` or `diskutil` call gets before it's SIGKILLed. A healthy
/// call answers in well under 2 s; a wedged FSKit service or a slow DiskArbitration
/// refusal scan is killed at this point, never awaited.
pub(super) const TOOL_DEADLINE: Duration = Duration::from_secs(30);

/// How often [`run_program`] asks whether the tool exited.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// A filesystem `hdiutil create` formats an image with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateFs {
    /// APFS.
    Apfs,
    /// Mac OS Extended (Journaled).
    HfsPlus,
    /// FAT32. ❌ Only the hand-run `external_drive_fixture` may use it: an FSKit
    /// `msdos` unmount kernel-panicked a Mac (`crates/cmdr-index/src/indexing/tests/CLAUDE.md`).
    LegacyFat32,
    /// exFAT. ❌ Same restriction as [`Self::LegacyFat32`].
    LegacyExFat,
}

/// The partition scheme `hdiutil create` lays out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateLayout {
    /// One partition on a GUID partition table.
    Gpt,
    /// One partition on an MBR partition table.
    Mbr,
}

/// Every call the harness makes. The list is closed on purpose: a verb that isn't
/// here can't run.
#[derive(Debug, Clone, Copy)]
pub(super) enum Call<'a> {
    /// `hdiutil create`, into a file the harness just made room for.
    Create {
        image: &'a Path,
        size: &'a str,
        fs: CreateFs,
        layout: CreateLayout,
        volume_name: &'a str,
        sparse: bool,
    },
    /// `hdiutil attach -plist -nobrowse`.
    Attach { image: &'a Path },
    /// `diskutil image attach --mountOptions nobrowse`: the DiskImages2 path Finder takes,
    /// so a `diskimagesiod` serves the image rather than a same-uid helper.
    AttachThroughDaemon { image: &'a Path },
    /// `hdiutil info -plist`.
    Info,
    /// `diskutil info -plist`.
    DiskInfo { target: &'a str },
    /// `hdiutil detach`, optionally `-force`, of an image's physical whole disk.
    Detach { whole: &'a str, force: bool },
    /// `diskutil apfs addVolume <container> APFS <name> -nomount`.
    AddApfsVolume { container: &'a str, name: &'a str },
    /// `diskutil partitionDisk <whole> GPT JHFS+ <first> 60M JHFS+ <second> R`.
    PartitionTwoJhfs {
        whole: &'a str,
        first: &'a str,
        second: &'a str,
    },
    /// `diskutil mount -mountOptions nobrowse`.
    MountNobrowse { node: &'a str },
    /// `diskutil mount -mountOptions nobrowse -mountPoint <dir>`: a mount inside an
    /// ordinary folder, the shape `/Volumes` never has.
    MountNobrowseAt { node: &'a str, mount_point: &'a Path },
    /// `diskutil unmount` of a node.
    Unmount { node: &'a str },
    /// `diskutil unmountDisk` of a whole disk: every volume on it, one request per volume.
    UnmountDisk { whole: &'a str },
    /// `diskutil renameVolume <node> <name>`.
    RenameVolume { node: &'a str, name: &'a str },
    /// `diskutil eject` of a mount point.
    Eject { mount_point: &'a Path },
    /// `/sbin/umount` of a mount point: the raw syscall wrapper, which does NOT go
    /// through DiskArbitration. The only way to make a volume vanish with no approval
    /// asked, short of pulling a real cable. Ownership-checked like every other change.
    RawUmount { mount_point: &'a Path },
}

impl Call<'_> {
    /// What the call changes, or `None` for a call that changes no existing disk.
    pub(super) fn target(&self) -> Option<Target<'_>> {
        match *self {
            Call::Create { .. }
            | Call::Attach { .. }
            | Call::AttachThroughDaemon { .. }
            | Call::Info
            | Call::DiskInfo { .. } => None,
            Call::Detach { whole, .. } => Some(Target::Node(whole)),
            Call::AddApfsVolume { container, .. } => Some(Target::Node(container)),
            Call::PartitionTwoJhfs { whole, .. } => Some(Target::Node(whole)),
            Call::MountNobrowse { node }
            | Call::MountNobrowseAt { node, .. }
            | Call::Unmount { node }
            | Call::RenameVolume { node, .. } => Some(Target::Node(node)),
            Call::UnmountDisk { whole } => Some(Target::Node(whole)),
            Call::Eject { mount_point } | Call::RawUmount { mount_point } => Some(Target::MountPoint(mount_point)),
        }
    }

    /// The program and its arguments.
    fn command(&self) -> (&'static str, Vec<String>) {
        let owned = |args: &[&str]| args.iter().map(|arg| (*arg).to_string()).collect::<Vec<_>>();
        match *self {
            Call::Create {
                image,
                size,
                fs,
                layout,
                volume_name,
                sparse,
            } => {
                let fs = match fs {
                    CreateFs::Apfs => "APFS",
                    CreateFs::HfsPlus => "HFS+",
                    CreateFs::LegacyFat32 => "MS-DOS FAT32",
                    CreateFs::LegacyExFat => "ExFAT",
                };
                let layout = match layout {
                    CreateLayout::Gpt => "GPTSPUD",
                    CreateLayout::Mbr => "MBRSPUD",
                };
                let mut args = owned(&[
                    "create",
                    "-size",
                    size,
                    "-fs",
                    fs,
                    "-volname",
                    volume_name,
                    "-layout",
                    layout,
                ]);
                if sparse {
                    args.extend(owned(&["-type", "SPARSE"]));
                }
                args.push(image.to_string_lossy().into_owned());
                ("hdiutil", args)
            }
            Call::Attach { image } => {
                let mut args = owned(&["attach", "-plist", "-nobrowse"]);
                args.push(image.to_string_lossy().into_owned());
                ("hdiutil", args)
            }
            Call::AttachThroughDaemon { image } => {
                let mut args = owned(&["image", "attach", "--mountOptions", "nobrowse"]);
                args.push(image.to_string_lossy().into_owned());
                ("diskutil", args)
            }
            Call::Info => ("hdiutil", owned(&["info", "-plist"])),
            Call::DiskInfo { target } => ("diskutil", owned(&["info", "-plist", target])),
            Call::Detach { whole, force: false } => ("hdiutil", owned(&["detach", &format!("/dev/{whole}")])),
            Call::Detach { whole, force: true } => ("hdiutil", owned(&["detach", "-force", &format!("/dev/{whole}")])),
            Call::AddApfsVolume { container, name } => (
                "diskutil",
                owned(&["apfs", "addVolume", container, "APFS", name, "-nomount"]),
            ),
            Call::PartitionTwoJhfs { whole, first, second } => (
                "diskutil",
                owned(&[
                    "partitionDisk",
                    whole,
                    "GPT",
                    "JHFS+",
                    first,
                    "60M",
                    "JHFS+",
                    second,
                    "R",
                ]),
            ),
            Call::MountNobrowse { node } => ("diskutil", owned(&["mount", "-mountOptions", "nobrowse", node])),
            Call::MountNobrowseAt { node, mount_point } => {
                let mut args = owned(&["mount", "-mountOptions", "nobrowse", "-mountPoint"]);
                args.push(mount_point.to_string_lossy().into_owned());
                args.push(node.to_string());
                ("diskutil", args)
            }
            Call::Unmount { node } => ("diskutil", owned(&["unmount", node])),
            Call::UnmountDisk { whole } => ("diskutil", owned(&["unmountDisk", whole])),
            Call::RenameVolume { node, name } => ("diskutil", owned(&["renameVolume", node, name])),
            Call::Eject { mount_point } => {
                let mut args = owned(&["eject"]);
                args.push(mount_point.to_string_lossy().into_owned());
                ("diskutil", args)
            }
            // The absolute path, so no `PATH` entry can decide what "umount" means.
            Call::RawUmount { mount_point } => ("/sbin/umount", vec![mount_point.to_string_lossy().into_owned()]),
        }
    }

    /// For error messages and panics: the command line.
    pub(super) fn describe(&self) -> String {
        let (program, args) = self.command();
        format!("{program} {}", args.join(" "))
    }
}

/// A call that ran to an exit status.
#[derive(Debug)]
pub struct Finished {
    /// The exit code, or `None` when a signal ended the tool.
    pub code: Option<i32>,
    /// Everything the tool wrote to stdout.
    pub stdout: Vec<u8>,
    /// Everything the tool wrote to stderr.
    pub stderr: Vec<u8>,
}

/// Runs `call` if `check` lets it: a call with a [`Call::target`] runs only after
/// `check` answers `Ok` for that target, and a refusal never reaches `spawn`.
pub(super) fn gated<T>(
    call: &Call<'_>,
    check: impl FnOnce(Target<'_>) -> Result<(), Refusal>,
    spawn: impl FnOnce() -> Result<T, HarnessError>,
) -> Result<T, HarnessError> {
    if let Some(target) = call.target() {
        check(target).map_err(|refusal| HarnessError::Refused {
            call: call.describe(),
            refusal,
        })?;
    }
    spawn()
}

/// Runs `call` under `deadline`, SIGKILLing it past that. A nonzero exit is
/// [`HarnessError::Failed`], carrying the code and stderr.
///
/// ❌ Don't call this for a call with a [`Call::target`] without the ownership gate;
/// `DiskImageSession` is the one caller, and it always gates.
pub(super) fn run(call: &Call<'_>, deadline: Duration) -> Result<Finished, HarnessError> {
    let (program, args) = call.command();
    let finished = run_program(program, &args, deadline).map_err(|error| match error {
        RunError::CouldNotStart(error) => HarnessError::CouldNotStart {
            call: call.describe(),
            error,
        },
        RunError::TimedOut => HarnessError::TimedOut { call: call.describe() },
    })?;
    if finished.code == Some(0) {
        Ok(finished)
    } else {
        Err(HarnessError::Failed {
            call: call.describe(),
            code: finished.code,
            stderr: String::from_utf8_lossy(&finished.stderr).trim().to_string(),
        })
    }
}

#[derive(Debug)]
enum RunError {
    /// The tool couldn't be started or waited on, or its output couldn't be read.
    CouldNotStart(io::Error),
    TimedOut,
}

/// The process half of [`run`], separate so a test can hand it a program that
/// outlives the deadline.
///
/// Output goes to anonymous temp files rather than pipes: a large plist can't fill a
/// pipe and stall the tool, and a helper the tool spawned can't hold a pipe open and
/// keep a read blocked past the kill.
fn run_program(program: &str, args: &[String], deadline: Duration) -> Result<Finished, RunError> {
    let mut stdout = tempfile::tempfile().map_err(RunError::CouldNotStart)?;
    let mut stderr = tempfile::tempfile().map_err(RunError::CouldNotStart)?;
    let mut child = Command::new(program)
        .args(args)
        // The tools' messages stay English, which `HarnessError::says_resource_busy` reads.
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout.try_clone().map_err(RunError::CouldNotStart)?))
        .stderr(Stdio::from(stderr.try_clone().map_err(RunError::CouldNotStart)?))
        .spawn()
        .map_err(RunError::CouldNotStart)?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < deadline => {
                // allowed-test-sleep: the poll interval of a hard deadline on a real `hdiutil`/`diskutil` child
                std::thread::sleep(POLL_INTERVAL);
            }
            outcome => {
                // Past the deadline, or the wait itself failed: SIGKILL (what `Child::kill`
                // sends on Unix), then reap, so nothing is left running.
                let _ = child.kill();
                let _ = child.wait();
                return Err(match outcome {
                    Err(error) => RunError::CouldNotStart(error),
                    _ => RunError::TimedOut,
                });
            }
        }
    };
    Ok(Finished {
        code: status.code(),
        stdout: read_all(&mut stdout).map_err(RunError::CouldNotStart)?,
        stderr: read_all(&mut stderr).map_err(RunError::CouldNotStart)?,
    })
}

fn read_all(file: &mut std::fs::File) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.rewind()?;
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn a_tool_past_its_deadline_is_killed_and_reported_as_timed_out() {
        let dir = crate::testing::TestDir::new("runner_deadline");
        let pid_file = dir.join("pid");
        let script = format!("echo $$ > '{}'; exec /bin/sleep 20", pid_file.display());
        let started = Instant::now();
        let result = run_program("/bin/sh", &["-c".to_string(), script], Duration::from_secs(1));
        assert!(matches!(result, Err(RunError::TimedOut)), "got {result:?}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the runner waited out the tool instead of killing it: {:?}",
            started.elapsed()
        );
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .expect("the tool started and wrote its pid")
            .trim()
            .parse()
            .expect("a pid");
        // SAFETY: signal 0 delivers nothing; it only asks whether `pid` exists.
        let still_there = unsafe { libc::kill(pid, 0) } == 0;
        assert!(
            !still_there,
            "the tool (pid {pid}) must be killed and reaped, not left running"
        );
    }

    #[test]
    fn a_tool_that_answers_in_time_hands_back_its_code_and_output() {
        let finished = run_program(
            "/bin/sh",
            &["-c".to_string(), "echo out; echo err >&2; exit 3".to_string()],
            Duration::from_secs(5),
        )
        .expect("sh runs");
        assert_eq!(finished.code, Some(3));
        assert_eq!(finished.stdout, b"out\n");
        assert_eq!(finished.stderr, b"err\n");
    }

    /// `says_resource_busy` reads English, so a localized Mac must still get English.
    #[test]
    fn a_tool_runs_in_the_c_locale() {
        let finished = run_program(
            "/bin/sh",
            &["-c".to_string(), "printf %s \"$LC_ALL\"".to_string()],
            Duration::from_secs(5),
        )
        .expect("sh runs");
        assert_eq!(finished.stdout, b"C");
    }

    #[test]
    fn a_refused_call_never_reaches_the_tool() {
        let call = Call::Eject {
            mount_point: Path::new("/Volumes/Backup"),
        };
        let result: Result<(), HarnessError> = gated(
            &call,
            |_| Err(Refusal::ImageNotAttached),
            || panic!("a refused call must not spawn the tool"),
        );
        assert!(
            matches!(result, Err(HarnessError::Refused { ref refusal, .. }) if *refusal == Refusal::ImageNotAttached),
            "got {result:?}"
        );
    }

    #[test]
    fn a_call_that_changes_a_disk_is_always_checked_against_its_target() {
        let checked = Cell::new(false);
        let call = Call::Detach {
            whole: "disk5",
            force: false,
        };
        let result = gated(
            &call,
            |target| {
                assert!(matches!(target, Target::Node("disk5")), "got {target:?}");
                checked.set(true);
                Ok(())
            },
            || Ok(()),
        );
        assert!(result.is_ok());
        assert!(checked.get(), "a detach must be ownership-checked");
    }

    /// A rename changes a mounted volume, so it's proven ours like any other change.
    #[test]
    fn a_rename_is_checked_against_the_node_it_renames() {
        let checked = Cell::new(false);
        let call = Call::RenameVolume {
            node: "disk5s1",
            name: "CMDR1",
        };
        let result = gated(
            &call,
            |target| {
                assert!(matches!(target, Target::Node("disk5s1")), "got {target:?}");
                checked.set(true);
                Ok(())
            },
            || Ok(()),
        );
        assert!(result.is_ok());
        assert!(checked.get(), "a rename must be ownership-checked");
        assert_eq!(call.describe(), "diskutil renameVolume disk5s1 CMDR1");
    }

    #[test]
    fn a_read_runs_without_an_ownership_check() {
        let result = gated(&Call::Info, |_| panic!("a read has nothing to own"), || Ok(()));
        assert!(result.is_ok());
    }
}
