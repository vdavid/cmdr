package checks

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// transportResetPatterns are mtp-rs's three transport-reset entry points. All of
// them send the Still Image Class `DEVICE_RESET` control request; the selector
// only decides which device receives it.
var transportResetPatterns = []string{
	"reset_by_serial(",
	"reset_by_location(",
	"reset_first(",
}

type transportResetSite struct {
	relPath string
	line    int
	text    string
}

// RunMtpNoTransportReset fails the build if anything under `src/mtp/` sends a
// USB transport reset.
//
// On Android the reset is a kill switch, not a recovery step: MtpServer answers
// it by tearing its FunctionFS endpoints down and never re-arms them, while the
// USB device controller stays `configured`. The phone keeps enumerating and
// keeps appearing in a device list while answering nothing, until the user
// physically unplugs it. Recovery from a `SessionReset` is drop-the-session plus
// spaced reopens, which self-heals; adding a reset converts that into a replug.
// See `apps/desktop/src-tauri/src/mtp/connection/DETAILS.md` § "No transport
// reset in recovery" for the logcat evidence. Deliberately has no opt-out
// directive: reintroducing a reset means deleting this check, which is the
// informed act we want it to force.
func RunMtpNoTransportReset(ctx *CheckContext) (CheckResult, error) {
	mtpDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "src", "mtp")

	violations, scanned, err := scanForTransportResets(ctx.RootDir, mtpDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan MTP Rust files: %w", err)
	}

	if len(violations) > 0 {
		sort.Slice(violations, func(i, j int) bool {
			if violations[i].relPath == violations[j].relPath {
				return violations[i].line < violations[j].line
			}
			return violations[i].relPath < violations[j].relPath
		})
		var sb strings.Builder
		for _, v := range violations {
			sb.WriteString(fmt.Sprintf("  %s:%d: %s\n", v.relPath, v.line, v.text))
		}
		return CheckResult{}, fmt.Errorf(
			"found %d USB transport reset %s under `src/mtp/` "+
				"(on Android the reset kills MTP for good: MtpServer drops its FunctionFS endpoints and never "+
				"re-arms them, so the phone keeps enumerating while answering nothing until it's replugged — "+
				"recovery is drop-the-session plus spaced reopens, which self-heals; see "+
				"`apps/desktop/src-tauri/src/mtp/connection/DETAILS.md` § \"No transport reset in recovery\"):\n%s",
			len(violations), Pluralize(len(violations), "call", "calls"),
			strings.TrimRight(sb.String(), "\n"),
		)
	}

	return Success(fmt.Sprintf(
		"%d MTP Rust %s scanned, no USB transport reset",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanForTransportResets(rootDir, srcDir string) ([]transportResetSite, int, error) {
	var violations []transportResetSite
	scanned := 0

	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}
		scanned++

		relPath, relErr := filepath.Rel(rootDir, path)
		if relErr != nil {
			relPath = path
		}

		fileViolations, scanErr := scanRustFileForTransportResets(path, relPath)
		if scanErr != nil {
			return scanErr
		}
		violations = append(violations, fileViolations...)
		return nil
	})

	return violations, scanned, err
}

// scanRustFileForTransportResets scans one file. Comment lines are skipped, so a
// doc comment explaining why the reset is gone doesn't flag itself.
func scanRustFileForTransportResets(path, relPath string) ([]transportResetSite, error) {
	f, openErr := os.Open(path)
	if openErr != nil {
		return nil, openErr
	}
	defer f.Close()

	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 64*1024), 1024*1024)

	var violations []transportResetSite
	lineNum := 0

	for scanner.Scan() {
		lineNum++
		line := scanner.Text()

		if strings.HasPrefix(strings.TrimLeft(line, " \t"), "//") {
			continue
		}
		if !lineHasTransportReset(line) {
			continue
		}
		violations = append(violations, transportResetSite{relPath: relPath, line: lineNum, text: strings.TrimSpace(line)})
	}
	return violations, scanner.Err()
}

func lineHasTransportReset(line string) bool {
	for _, pattern := range transportResetPatterns {
		if strings.Contains(line, pattern) {
			return true
		}
	}
	return false
}
