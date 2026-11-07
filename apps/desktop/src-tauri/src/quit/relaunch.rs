//! Restarting Cmdr: the update toast's Restart and onboarding's "Restart Cmdr".
//!
//! On macOS a detached waiter outlives us, waits for this process to exit, and only then runs
//! `/usr/bin/open <bundle> --args …` WITHOUT `-n`. Tauri's own restart (2.12.2+) runs
//! `open -n` while we're still alive, and the Dock gives that second instance its own tile: the
//! pinned tile goes dark and an extra one carries the running dot until the next quit (#1037,
//! `docs/notes/dock-duplicate-icon-2026-11-07.md`). With one instance at a time, LaunchServices
//! binds the new process to the pinned tile. Same shape as Sparkle's relauncher.
//!
//! What Tauri's version fixed stays fixed: the new process starts through LaunchServices
//! (launchd owns it, stdio on `/dev/null`), with our args.
//!
//! The exit goes through the quit gate like ⌘Q: with a transfer in flight the person is asked
//! first, and "Keep working" calls the relaunch off ([`disarm`]). The waiter starts only in
//! `RunEvent::Exit` ([`on_exit`]), once the exit is certain and every flush before it has run.
//!
//! Outside a `.app` bundle (a dev binary) and off macOS this falls back to Tauri's
//! `request_restart`, which has nothing to double there.

#[cfg(target_os = "macos")]
use std::ffi::OsString;
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
use crate::ignore_poison::IgnorePoison as _;
#[cfg(target_os = "macos")]
use tauri::Manager as _;

/// The waiter to start when this process exits, set by [`relaunch`] and taken by [`on_exit`].
#[cfg(target_os = "macos")]
static ARMED: Mutex<Option<WaiterPlan>> = Mutex::new(None);

/// Calls a pending relaunch off: the person answered the quit prompt with "Keep working", so a
/// later ⌘Q must quit plainly.
pub(crate) fn disarm() {
    #[cfg(target_os = "macos")]
    if ARMED.lock_ignore_poison().take().is_some() {
        log::info!(target: "quit", "the relaunch was called off with the quit");
    }
}

/// How long the waiter waits for this process to exit before it opens the bundle anyway. The
/// exit itself takes well under a second (window state, search, and the security log flush in
/// `RunEvent::Exit`); this bound is for a process that hangs on the way out. An `open` while
/// the old one still runs only activates it, so the bound costs nothing but a log line.
#[cfg(target_os = "macos")]
const EXIT_WAIT: Duration = Duration::from_secs(30);

/// How often the waiter checks whether this process is gone.
#[cfg(target_os = "macos")]
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// The `logger` tag the waiter writes under, so its lines are findable after we're gone:
/// `log show --last 1h --predicate 'eventMessage CONTAINS "cmdr-relaunch"'`.
#[cfg(target_os = "macos")]
const LOG_TAG: &str = "cmdr-relaunch";

/// The waiter's script. Positional params, never interpolation, so no path or arg needs quoting:
/// `$1` pid, `$2` poll ticks before giving up, `$3` poll interval in seconds, `$4` the opener,
/// `$5` the bundle, the rest the app's args.
///
/// The outer `sh` forks the subshell into the background and returns at once, so we reap it
/// right away and the subshell, reparented to launchd, outlives us. `kill -0` asks "does this pid
/// exist" without signalling it.
#[cfg(target_os = "macos")]
const WAITER_SCRIPT: &str = r#"pid=$1; ticks=$2; interval=$3; opener=$4; bundle=$5; shift 5
(
  i=0
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$i" -ge "$ticks" ]; then
      /usr/bin/logger -t TAG "Cmdr (pid $pid) still running after the wait, opening $bundle anyway"
      break
    fi
    sleep "$interval"
    i=$((i + 1))
  done
  "$opener" "$bundle" --args "$@" || /usr/bin/logger -t TAG "Couldn't open $bundle (exit $?), start Cmdr by hand"
) </dev/null >/dev/null 2>&1 &
"#;

/// What the waiter needs: which process to outlive and what to open after.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
pub(crate) struct WaiterPlan {
    pub pid: u32,
    /// Poll ticks before the waiter stops waiting and opens anyway.
    pub ticks: u32,
    /// Seconds between polls, as `sleep` takes them (`"0.1"`).
    pub interval: String,
    /// `/usr/bin/open` in production; a stand-in in tests.
    pub opener: PathBuf,
    pub bundle: PathBuf,
    /// The app's own args, without argv[0].
    pub args: Vec<OsString>,
}

/// The waiter's program and argv. Pure, so the shape is pinned by tests: `open` gets the bundle,
/// then `--args` and our args, and never `-n`.
#[cfg(target_os = "macos")]
pub(crate) fn waiter_command(plan: &WaiterPlan) -> (PathBuf, Vec<OsString>) {
    let mut argv: Vec<OsString> = vec![
        "-c".into(),
        WAITER_SCRIPT.replace("TAG", LOG_TAG).into(),
        // `$0` of the script, shown in `ps`.
        LOG_TAG.into(),
        plan.pid.to_string().into(),
        plan.ticks.to_string().into(),
        plan.interval.clone().into(),
        plan.opener.clone().into(),
        plan.bundle.clone().into(),
    ];
    argv.extend(plan.args.iter().cloned());
    (PathBuf::from("/bin/sh"), argv)
}

/// The `.app` bundle a binary at `Contents/MacOS/<name>` lives in, or `None` outside one.
#[cfg(target_os = "macos")]
pub(crate) fn bundle_root(binary: &Path) -> Option<PathBuf> {
    let macos_dir = binary.parent()?;
    if macos_dir.file_name()? != "MacOS" {
        return None;
    }
    let contents = macos_dir.parent()?;
    if contents.file_name()? != "Contents" {
        return None;
    }
    let bundle = contents.parent()?;
    bundle
        .extension()
        .filter(|ext| *ext == "app")
        .map(|_| bundle.to_path_buf())
}

/// Starts the waiter and returns once it's detached. The outer `sh` exits immediately, so this
/// waits on it (reaping it) and reads its status.
#[cfg(target_os = "macos")]
pub(crate) fn spawn_waiter(plan: &WaiterPlan) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let (program, argv) = waiter_command(plan);
    let status = Command::new(program)
        .args(argv)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Its own process group: a terminal's Ctrl-C on a dev run must not reach the waiter.
        .process_group(0)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "the waiter's shell exited with {status}"
        )))
    }
}

/// Arms a relaunch without an app, for the gate's tests.
#[cfg(all(test, target_os = "macos"))]
pub(crate) fn arm_for_test(plan: WaiterPlan) {
    *ARMED.lock_ignore_poison() = Some(plan);
}

/// Whether a relaunch is waiting on the exit, for the gate's tests.
#[cfg(all(test, target_os = "macos"))]
pub(crate) fn is_armed() -> bool {
    ARMED.lock_ignore_poison().is_some()
}

/// Exits Cmdr (through the quit gate) and starts it again. See the module docs for why it isn't
/// Tauri's `restart`.
pub(crate) fn relaunch<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    #[cfg(target_os = "macos")]
    {
        let env = app.env();
        match tauri::process::current_binary(&env)
            .ok()
            .as_deref()
            .and_then(bundle_root)
        {
            Some(bundle) => {
                let plan = WaiterPlan {
                    pid: std::process::id(),
                    ticks: u32::try_from(EXIT_WAIT.as_millis() / POLL_INTERVAL.as_millis()).unwrap_or(u32::MAX),
                    interval: POLL_INTERVAL.as_secs_f64().to_string(),
                    opener: PathBuf::from("/usr/bin/open"),
                    bundle,
                    args: env.args_os.iter().skip(1).cloned().collect(),
                };
                log::info!(target: "quit", "relaunch requested: quitting, then reopening {}", cmdr_fs::log_private::LogPrivate::dir_path(&plan.bundle));
                *ARMED.lock_ignore_poison() = Some(plan);
                // Comes back as `RunEvent::ExitRequested`, where the gate may hold it; `on_exit`
                // starts the waiter only once the exit really happens.
                app.exit(0);
                return;
            }
            None => log::info!(target: "quit", "not running from an .app bundle; relaunching through Tauri's restart"),
        }
    }
    app.request_restart();
}

/// The last step of `RunEvent::Exit`: starts the waiter if a relaunch is armed. If it can't
/// start, Tauri's restart still brings Cmdr back (with the doubled Dock tile) rather than
/// leaving the person with no Cmdr.
pub(crate) fn on_exit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    #[cfg(target_os = "macos")]
    {
        let Some(plan) = ARMED.lock_ignore_poison().take() else {
            return;
        };
        match spawn_waiter(&plan) {
            Ok(()) => log::info!(target: "quit", "relaunch waiter started; it opens Cmdr once this process is gone"),
            Err(e) => {
                crate::log_error!(target: "quit", "couldn't start the relaunch waiter ({e}); falling back to Tauri's restart");
                tauri::process::restart(&app.env());
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[cfg(all(test, target_os = "macos"))]
#[path = "relaunch_tests.rs"]
mod tests;
