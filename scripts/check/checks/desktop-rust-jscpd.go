package checks

// rustJscpdLane detects copy-paste across every first-party Rust tree.
// Duplication doesn't stop being duplication at a crate boundary — and copy-paste
// ACROSS the boundary is the specific thing an extraction invites. The vendored
// fork is out of jurisdiction.
var rustJscpdLane = jscpdLane{
	checkID:       "desktop-rust-jscpd",
	allowlistName: "jscpd-rust",
	what:          "Rust",
	roots: func(rootDir string) ([]string, error) {
		return ScannerRoots(rootDir, "desktop-rust-jscpd")
	},
	formats:  "rust",
	minLines: 5,
	// 100 tokens is roughly 25 lines of Rust: a block somebody copied on purpose.
	// Measured on this repo, the floor is the difference between a list and a
	// wall — 91 clones at 100, 200 at 75, 555 at 50, where the extra ones are
	// mostly short match arms and builder chains that read as idiom, not as
	// copy-paste. Raise this only with a measurement.
	minTokens: 100,
	// Exclude test code: this lane guards duplication in production Rust, not
	// tests (which are intentionally repetitive). The globs cover every test
	// convention in this repo without over-matching production names like
	// `latest.rs`: `test*.rs` (prefix), `*_test.rs` / `*_tests.rs` (suffix),
	// `*_test_*.rs` (shared fixture modules), and `tests/` module directories,
	// which is where most of this repo's unit tests actually live.
	ignore: "**/test*.rs,**/*_test.rs,**/*_tests.rs,**/*_test_*.rs,**/tests/**",
}

// RunJscpdRust reports copy-paste between Rust files: which two files say the same
// thing, at which lines. Warn-only, gated by `jscpd-rust-allowlist.json`.
func RunJscpdRust(ctx *CheckContext) (CheckResult, error) {
	return runJscpdLane(ctx, rustJscpdLane)
}
