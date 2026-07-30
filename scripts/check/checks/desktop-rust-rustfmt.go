package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunRustfmt formats Rust code across the whole workspace.
func RunRustfmt(ctx *CheckContext) (CheckResult, error) {
	// `--all` covers every workspace member. `cmdr-fsevent-stream` needs no
	// exclusion here even though it's macOS-only: rustfmt parses, it doesn't
	// compile. It pins its own `rustfmt.toml` to keep upstream's defaults, which
	// rustfmt honors per source file.
	fileCount, err := countWorkspaceRustFiles(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't enumerate Rust sources: %w", err)
	}

	// Check which files need formatting (--files-with-diff lists them)
	checkCmd := exec.Command("cargo", "fmt", "--all", "--", "--check", "--files-with-diff")
	checkCmd.Dir = ctx.RootDir
	checkOutput, checkErr := RunCommand(checkCmd, true)

	// Parse files that need formatting
	var needsFormat []string
	if strings.TrimSpace(checkOutput) != "" {
		for line := range strings.SplitSeq(strings.TrimSpace(checkOutput), "\n") {
			// Only count lines that look like file paths (end with .rs)
			if strings.HasSuffix(line, ".rs") {
				needsFormat = append(needsFormat, line)
			}
		}
	}

	if ctx.CI {
		if checkErr != nil || len(needsFormat) > 0 {
			return CheckResult{}, fmt.Errorf("code is not formatted, run cargo fmt locally\n%s", indentOutput(checkOutput))
		}
		result := Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files")))
		result.Total = fileCount
		result.Issues = 0
		result.Changes = 0
		return result, nil
	}

	// Non-CI mode: format if needed
	if len(needsFormat) > 0 {
		fmtCmd := exec.Command("cargo", "fmt", "--all")
		fmtCmd.Dir = ctx.RootDir
		output, err := RunCommand(fmtCmd, true)
		if err != nil {
			return CheckResult{}, fmt.Errorf("rust formatting failed\n%s", indentOutput(output))
		}
		result := SuccessWithChanges(fmt.Sprintf("Formatted %d of %d %s", len(needsFormat), fileCount, Pluralize(fileCount, "file", "files")))
		result.Total = fileCount
		result.Issues = len(needsFormat)
		result.Changes = len(needsFormat)
		return result, nil
	}

	result := Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files")))
	result.Total = fileCount
	result.Issues = 0
	result.Changes = 0
	return result, nil
}

// countWorkspaceRustFiles counts the `.rs` files under every workspace member, so
// the reported total describes the same trees `cargo fmt --all` rewrites. Counting
// one tree while formatting another is how a lane starts reporting confidently
// about files it never looked at.
func countWorkspaceRustFiles(rootDir string) (int, error) {
	members, err := WorkspaceMembers(rootDir)
	if err != nil {
		return 0, err
	}
	count := 0
	for _, m := range members {
		walkErr := filepath.WalkDir(m.Dir, func(_ string, d os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if d.IsDir() {
				// A per-member `target/` only appears when someone built from inside
				// the member dir, but its generated sources would inflate the count
				// by thousands.
				if d.Name() == "target" {
					return filepath.SkipDir
				}
				return nil
			}
			if strings.HasSuffix(d.Name(), ".rs") {
				count++
			}
			return nil
		})
		if walkErr != nil {
			return 0, walkErr
		}
	}
	return count, nil
}
