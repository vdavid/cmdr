package checks

import (
	"path/filepath"
	"strings"
	"testing"
)

// logPrivateFixtureRoot builds a fixture repo with the given files under the app
// tree and returns the root.
func logPrivateFixtureRoot(t *testing.T, files map[string]string) string {
	t.Helper()
	root := t.TempDir()
	seedAppFixtureWorkspace(t, root)
	srcDir := filepath.Join(root, "apps", "desktop", "src-tauri", "src")
	for rel, body := range files {
		mustWrite(t, filepath.Join(srcDir, rel), body)
	}
	return root
}

func runLogPrivateOn(t *testing.T, files map[string]string) (CheckResult, error) {
	t.Helper()
	return RunLogPrivateValues(&CheckContext{RootDir: logPrivateFixtureRoot(t, files)})
}

// expectLogPrivateFlags runs the check over one file and asserts it fails naming
// every given `file.rs:line`, and only as many sites as listed.
func expectLogPrivateFlags(t *testing.T, body string, lines ...string) {
	t.Helper()
	_, err := runLogPrivateOn(t, map[string]string{"site.rs": body})
	if err == nil {
		t.Fatalf("expected unmarked values at %v, got success", lines)
	}
	for _, l := range lines {
		if !strings.Contains(err.Error(), "site.rs:"+l+" ") {
			t.Errorf("expected a finding at site.rs:%s, got:\n%s", l, err.Error())
		}
	}
	if got := strings.Count(err.Error(), "  apps/desktop/src-tauri/src/site.rs:"); got != len(lines) {
		t.Errorf("expected %d findings, got %d:\n%s", len(lines), got, err.Error())
	}
}

func expectLogPrivateClean(t *testing.T, files map[string]string) {
	t.Helper()
	res, err := runLogPrivateOn(t, files)
	if err != nil {
		t.Fatalf("expected success, got:\n%v", err)
	}
	if res.Code != ResultSuccess {
		t.Fatalf("expected ResultSuccess, got %v: %s", res.Code, res.Message)
	}
}

func TestLogPrivateValues_FlagsDisplayAndLossyConversions(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(p: &Path, q: &OsStr) {
    log::info!("Listing {}", p.display());
    log::debug!("Got {:?}", q.to_string_lossy());
}
`, "3", "4")
}

func TestLogPrivateValues_FlagsArgumentsNamedLikeAPathOrIdentity(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(src_path: &Path, entry: &Entry, volume_name: &str, hosts: &[String]) {
    log::debug!("copy {:?}", src_path);
    log::debug!("entry {}", entry.name);
    log::debug!("entry {:?}", entry.path());
    log::warn!("volume {} gone", volume_name);
    log::info!("hosts {:?}", hosts);
    log::info!("name {}", file_name_str);
}
`, "3", "4", "5", "6", "7", "8")
}

func TestLogPrivateValues_FlagsInlineCaptures(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(path: &Path, share: &str, count: usize) {
    log::debug!("listed {path:?} on {share} ({count} entries)");
    log::debug!("state lives in {STATE_FILE_PATH}");
}
`, "3", "3")
}

func TestLogPrivateValues_ReportsTheArgumentLineOfAMultiLineMacro(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(dir: &Path, n: usize) {
    log::info!(
        target: "listing",
        "listed {} ({} entries)",
        dir.display(),
        n,
    );
}
`, "6")
}

func TestLogPrivateValues_CoversEveryLogMacroSpelling(t *testing.T) {
	expectLogPrivateFlags(t, `
use log::{debug, error};
fn f(path: &Path, level: log::Level) {
    debug!("a {}", path.display());
    error!("b {}", path.display());
    trace!("c {}", path.display());
    log::log!(level, "d {}", path.display());
    log_error!("e {}", path.display());
    crate::log_error!("f {}", path.display());
    log::warn!["g {}", path.display()];
}
`, "4", "5", "6", "7", "8", "9", "10")
}

func TestLogPrivateValues_AcceptsWrappedValues(t *testing.T) {
	expectLogPrivateClean(t, map[string]string{"site.rs": `
use cmdr_fs::log_private::LogPrivate;
fn f(path: &Path, err: &io::Error, message: &str, dir: Option<&Path>) {
    log::debug!("listed {:?}", LogPrivate::dir_path(path));
    log::debug!("listed {}", &LogPrivate::file_path(&path));
    log::debug!("listed {:?}", cmdr_fs::log_private::LogPrivate::path(&path.display().to_string()));
    log::warn!("detail={:?}", LogDetail(&err.to_string()).marking(&[LogPrivate::path(path)]));
    log::warn!("detail={:?}", cmdr_fs::LogDetail(message));
    log::warn!("note {:?}", LoggedText(message));
    log::debug!("maybe {:?}", dir.map(LogPrivate::dir_path));
    log::debug!("named {p}", p = LogPrivate::path(path));
}
`})
}

func TestLogPrivateValues_LeavesNonIdentityValuesAlone(t *testing.T) {
	expectLogPrivateClean(t, map[string]string{"site.rs": `
fn f(paths: &[PathBuf], path: &Path, entry: &Entry, elapsed: Duration, file_count: usize, volume_id: &str) {
    log::debug!("{} paths in {:?}", paths.len(), elapsed);
    log::debug!("exists={} dir={}", path.exists(), path.is_dir());
    log::debug!("name is {} long", entry.name.len());
    log::debug!("ext {:?}", path.extension());
    log::debug!("{file_count} files on {volume_id}");
    log::debug!("{{path}} is a literal brace pair, {} too", file_count);
    log::debug!("a (paren, comma) and \"quote\" {}", file_count);
    log::debug!(r#"raw "(path)" {}"#, file_count);
    log::debug!("{:?}", paths.iter().collect::<HashMap<_, _>>().len());
    log::debug!("{}", Path::new("/tmp").exists());
    log::debug!("{}", count_of(file_count));
    println!("{}", path.display());
    let s = format!("{}", path.display());
    // log::debug!("{}", path.display());
    /* log::debug!("{}", path.display()); */
}
`})
}

func TestLogPrivateValues_FlagsTheWiderIdentityNames(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(remote: &str, key: &str, root_url: Url, addr: String, device: &str, to: &Path, target: &Path) {
    log::debug!("s3 copied to {remote} under {key}");
    log::debug!("webdav probe: PROPFIND {}", root_url);
    log::debug!("addr={}", addr);
    log::debug!("device {device:?} staging {:?}", staging);
    log::debug!("rename {from:?} -> {to:?}");
    log::debug!("link points at {:?}", target);
    log::debug!("sidecar {}", self.key);
}
`, "3", "3", "4", "5", "6", "6", "7", "7", "8", "9")
}

func TestLogPrivateValues_AScalarFieldOfAnIdentityIsAFactAboutIt(t *testing.T) {
	expectLogPrivateClean(t, map[string]string{"site.rs": `
fn f(target: &Target, entry: &Entry, old_entry: &Entry, n: usize) {
    log::debug!("search on {}", target.volume_id);
    log::debug!("model {:?}", target.model.adapter_kind);
    log::debug!("size {}", entry.size.unwrap_or(0));
    log::debug!("server {:?} on {}", entry.id, entry.port);
    log::debug!("parent {}", old_entry.parent_id);
    log::debug!("{}", n.to_string());
    log::debug!("{}", self.log_target);
}
`})
}

func TestLogPrivateValues_SeesThroughStringsWithParensAndChars(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(path: &Path, c: char) {
    log::debug!("open ( paren \" ) {} {}", c == '(', path.display());
    log::debug!("next {}", path.display());
}
`, "3", "4")
}

func TestLogPrivateValues_JudgesANestedFormatOnce(t *testing.T) {
	expectLogPrivateFlags(t, `
fn f(path: &Path) {
    log::debug!("{}", format!("at {}", path.display()));
}
`, "3")
}

func TestLogPrivateValues_SkipsTestCode(t *testing.T) {
	expectLogPrivateClean(t, map[string]string{
		"site_test.rs":     `fn t(p: &Path) { log::debug!("{}", p.display()); }`,
		"tests/fixture.rs": `fn t(p: &Path) { log::debug!("{}", p.display()); }`,
		"site.rs": `
#[cfg(test)]
mod split_tests;

fn real() {}

#[cfg(test)]
mod tests {
    fn t(p: &Path) {
        log::debug!("{}", p.display());
    }
}
`,
	})
}

func TestLogPrivateValues_AnOutOfLineTestModDoesNotHideTheNextItem(t *testing.T) {
	expectLogPrivateFlags(t, `
#[cfg(test)]
mod tests;

fn real(p: &Path) {
    log::debug!("{}", p.display());
}
`, "6")
}

func TestLogPrivateValues_HonorsTheOptOut(t *testing.T) {
	expectLogPrivateClean(t, map[string]string{"site.rs": `
fn f(name: &str, thread_name: &str) {
    // allowed-log-private: the setting's name, not the user's
    log::debug!("setting {} changed", name);
    log::debug!("thread {}", thread_name); // allowed-log-private: a thread's name
}
`})
}

func TestLogPrivateValues_FlagsAnOrphanedOptOut(t *testing.T) {
	_, err := runLogPrivateOn(t, map[string]string{"site.rs": `
fn f(n: usize) {
    // allowed-log-private: nothing to excuse here
    log::debug!("{} entries", n);
}
`})
	if err == nil || !strings.Contains(err.Error(), "site.rs:3") {
		t.Fatalf("expected the orphaned opt-out at site.rs:3, got: %v", err)
	}
}

func TestLogPrivateValues_FlagsAMarkedValueTurnedIntoAString(t *testing.T) {
	_, err := runLogPrivateOn(t, map[string]string{"site.rs": `
fn f(path: &Path, text: &str) -> (String, String, String) {
    let a = LogPrivate::path(path).to_string();
    let b = format!("{}", LogPrivate::dir_path(path));
    let c = format!("{:?}", LogDetail(text));
    log::debug!("{}", format!("{}", LogPrivate::path(path)));
    let note = format!("detail={:?}", LoggedText(text));
    (a, b, c)
}
`})
	if err == nil {
		t.Fatal("expected marked strings outside a log macro to fail")
	}
	for _, line := range []string{"site.rs:3", "site.rs:4", "site.rs:5"} {
		if !strings.Contains(err.Error(), line) {
			t.Errorf("expected %s, got:\n%s", line, err.Error())
		}
	}
	for _, line := range []string{"site.rs:6", "site.rs:7"} {
		if strings.Contains(err.Error(), line) {
			t.Errorf("did not expect %s (inside a log macro, or re-quoting our own log text), got:\n%s", line, err.Error())
		}
	}
}

func TestReadsAsIdentity(t *testing.T) {
	for _, name := range []string{
		"path", "src_path", "dest", "dst", "volume_name", "path_str", "paths", "filename", "hostname",
		"username", "server", "share", "label", "root", "roots", "dir", "directories", "parent_dir",
		"name_lower", "_path", "folder", "source", "host", "user", "file", "subdir", "mount_point",
		// A remote side, a link or copy target, a device, a staging file, an
		// object key, a URL or address, a bucket, an account: each names the
		// user's data or the place it lives.
		"remote", "to_remote", "target", "link_target", "device", "staging", "key", "object_key",
		"bucket_key", "root_url", "url_string", "addr", "address", "bucket", "prefix", "account",
		"email", "query", "query_text", "endpoint", "entry", "old_entry", "uri",
		// `from` / `to` name the two sides of a copy or rename, but only alone.
		"from", "to",
	} {
		if !readsAsIdentity(name) {
			t.Errorf("expected %q to read as a path or identity", name)
		}
	}
	for _, name := range []string{
		"file_count", "path_len", "profile", "rename", "volume_id", "entries", "user_id", "server_id",
		"count", "elapsed", "nickname", "e", "err", "self", "kind",
		// Counts: a bare plural of a file-system noun is a tally in a log line,
		// and so is a name that leads with a quantity word.
		"files", "dirs", "folders", "kept_files", "total_dirs", "num_paths", "n_files",
		// Flags: a name carrying a predicate word holds a bool.
		"is_dir", "is_directory", "source_is_dir", "in_subdir", "use_concurrent_path", "has_parent",
		"requires_api_key",
		// `to` / `from` inside a longer name are a conversion, not a side.
		"to_string", "to_owned", "from_utf8", "copied_to",
		// Our own logging's target, and a count of entries.
		"log_target", "entries",
	} {
		if readsAsIdentity(name) {
			t.Errorf("did not expect %q to read as a path or identity", name)
		}
	}
}

// pathErrorTypes declares two path-carrying errors the way the tree does, one
// through `ErrorLogText` and one through `log_detail`, plus the struct that
// holds the second, in a file of their own: the logging file never names them.
const pathErrorTypes = `
pub enum VolumeError { NotFound(String) }
impl ErrorLogText for VolumeError {
    fn write_log_text(&self, out: &mut ErrorText<'_>) -> fmt::Result { Ok(()) }
}
impl<T: ErrorLogText + ?Sized> ErrorLogText for &T {
    fn write_log_text(&self, out: &mut ErrorText<'_>) -> fmt::Result { Ok(()) }
}
pub enum WriteOperationError { PermissionDenied { path: String } }
impl WriteOperationError {
    pub fn log_detail(&self) -> WriteErrorDetail { todo!() }
}
pub struct WriteFailure {
    pub error: WriteOperationError,
    pub attempt: u32,
}
`

// The shape that shipped a file name holding ", " (cmdr-reports#103): a derived
// `{:?}` of a `WriteOperationError` field, declared in another file.
func TestLogPrivateValues_FlagsAPathCarryingErrorPrintedPlainly(t *testing.T) {
	_, err := runLogPrivateOn(t, map[string]string{
		"types.rs": pathErrorTypes,
		"site.rs": `
fn f(op_id: &str, failure: WriteFailure, e: &VolumeError) {
    log::warn!(target: "move", "move operation {} failed: {:?}", op_id, failure.error);
    log::warn!("couldn't delete: {}", e);
    log::warn!("couldn't delete: {e}");
    log::warn!("couldn't delete: {:?}", e.clone());
}
`,
	})
	if err == nil {
		t.Fatal("expected the plainly printed errors to fail")
	}
	for _, want := range []string{
		"site.rs:3 [path-carrying error: error]", "site.rs:4 [path-carrying error: e]",
		"site.rs:5 [path-carrying error: e]", "site.rs:6 [path-carrying error: e]",
	} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("expected %q, got:\n%s", want, err.Error())
		}
	}
	if got := strings.Count(err.Error(), "  apps/desktop/src-tauri/src/site.rs:"); got != 4 {
		t.Errorf("expected 4 findings, got %d:\n%s", got, err.Error())
	}
}

// Marked, a scalar beside the error, a method (whose return type the scanner
// can't see), and a generic named like the blanket impl's `T` all pass.
func TestLogPrivateValues_PassesAPathCarryingErrorLoggedMarked(t *testing.T) {
	expectLogPrivateClean(t, map[string]string{
		"types.rs": pathErrorTypes,
		"site.rs": `
fn f(op_id: &str, failure: WriteFailure, e: &VolumeError, report: &Report, data: T) {
    log::warn!("move operation {} failed: detail={:?}", op_id, failure.error.log_detail());
    log::warn!("couldn't delete: {}", LogError(e));
    log::warn!("couldn't delete: {e}", e = LogError(&e));
    log::warn!("attempt {}", failure.attempt);
    log::warn!("{}", report.error());
    log::warn!("{:?}", data);
}
`,
	})
}

func TestOuterTypeName(t *testing.T) {
	for ty, want := range map[string]string{
		"WriteOperationError":                               "WriteOperationError",
		"&'a VolumeError":                                   "VolumeError",
		"Option<cmdr_fs::volume::VolumeError>":              "VolumeError",
		"Arc<Mutex<VolumeError>>":                           "Mutex",
		"Result<PathBuf, VolumeError>":                      "PathBuf",
		"VolumeError::NotSupported":                         "NotSupported",
		"&mut Vec<Box<cmdr_fs::error_detail::ErrorDetail>>": "ErrorDetail",
	} {
		if got := outerTypeName(ty); got != want {
			t.Errorf("outerTypeName(%q) = %q, want %q", ty, got, want)
		}
	}
}
