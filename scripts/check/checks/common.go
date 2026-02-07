package checks

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// App represents the application a check belongs to.
type App string

const (
	AppDesktop       App = "desktop"
	AppWebsite       App = "website"
	AppLicenseServer App = "license-server"
	AppScripts       App = "scripts"
	AppOther         App = "other"
)

// AppDisplayName returns a human-readable name for an app with icon.
func AppDisplayName(app App) string {
	switch app {
	case AppDesktop:
		return "🖥️  Desktop"
	case AppWebsite:
		return "🌐 Website"
	case AppLicenseServer:
		return "🔑 License server"
	case AppScripts:
		return "📜 Scripts"
	default:
		return string(app)
	}
}

// ResultCode indicates the outcome of a check.
type ResultCode int

const (
	ResultSuccess ResultCode = iota
	ResultWarning
	ResultSkipped
)

// CheckResult is returned by checks on success.
type CheckResult struct {
	Code        ResultCode
	Message     string
	MadeChanges bool // true if the check modified files (for example, formatted code)
}

// Success creates a success result with the given message (no changes made).
func Success(message string) CheckResult {
	return CheckResult{Code: ResultSuccess, Message: message, MadeChanges: false}
}

// SuccessWithChanges creates a success result indicating files were modified.
func SuccessWithChanges(message string) CheckResult {
	return CheckResult{Code: ResultSuccess, Message: message, MadeChanges: true}
}

// Skipped creates a skipped result with the given reason.
func Skipped(reason string) CheckResult {
	return CheckResult{Code: ResultSkipped, Message: reason}
}

// CheckContext holds the context for running checks.
type CheckContext struct {
	CI      bool
	Verbose bool
	RootDir string
}

// CheckFunc is the function signature for check implementations.
type CheckFunc func(ctx *CheckContext) (CheckResult, error)

// CheckDefinition defines a check's metadata and implementation.
type CheckDefinition struct {
	ID          string
	Nickname    string // Short alias shown in --help and accepted by --check (if empty, ID is used)
	DisplayName string
	App         App
	Tech        string
	IsSlow      bool
	DependsOn   []string
	Run         CheckFunc
}

// RunCommand executes a command and captures its output.
func RunCommand(cmd *exec.Cmd, captureOutput bool) (string, error) {
	var stdout, stderr bytes.Buffer
	if captureOutput {
		cmd.Stdout = &stdout
		cmd.Stderr = &stderr
	} else {
		cmd.Stdout = os.Stdout
		cmd.Stderr = os.Stderr
	}

	err := cmd.Run()
	output := stdout.String()
	if stderr.Len() > 0 {
		output += stderr.String()
	}
	return output, err
}

// CommandExists checks if a command exists in PATH.
func CommandExists(name string) bool {
	_, err := exec.LookPath(name)
	return err == nil
}

// EnsureGoTool ensures a Go tool is installed and returns the path to the binary.
// If the tool is already in PATH, returns just the name. Otherwise installs it
// and returns the full path to the installed binary.
func EnsureGoTool(name, installPath string) (string, error) {
	if CommandExists(name) {
		return name, nil
	}

	// Get Go's bin directory
	goBin := getGoBinDir()
	if goBin == "" {
		return "", fmt.Errorf("could not determine Go bin directory")
	}

	// Install the tool
	installCmd := exec.Command("go", "install", installPath)
	if _, err := RunCommand(installCmd, true); err != nil {
		return "", fmt.Errorf("failed to install %s: %w", name, err)
	}

	// Return full path to the binary
	return filepath.Join(goBin, name), nil
}

// getGoBinDir returns the directory where go install puts binaries.
func getGoBinDir() string {
	// First check GOBIN
	cmd := exec.Command("go", "env", "GOBIN")
	if output, err := RunCommand(cmd, true); err == nil {
		if bin := strings.TrimSpace(output); bin != "" {
			return bin
		}
	}

	// Fall back to GOPATH/bin
	cmd = exec.Command("go", "env", "GOPATH")
	if output, err := RunCommand(cmd, true); err == nil {
		if gopath := strings.TrimSpace(output); gopath != "" {
			return filepath.Join(gopath, "bin")
		}
	}

	// Last resort: ~/go/bin
	if home, err := os.UserHomeDir(); err == nil {
		return filepath.Join(home, "go", "bin")
	}

	return ""
}

// indentOutput indents each non-empty line of output.
func indentOutput(output string) string {
	lines := strings.Split(output, "\n")
	var result strings.Builder
	for _, line := range lines {
		if strings.TrimSpace(line) != "" {
			result.WriteString("      ")
			result.WriteString(line)
			result.WriteString("\n")
		}
	}
	return result.String()
}

// Pluralize returns singular if count is 1, plural otherwise.
// Example: Pluralize(1, "file", "files") returns "file"
// Example: Pluralize(5, "file", "files") returns "files"
func Pluralize(count int, singular, plural string) string {
	if count == 1 {
		return singular
	}
	return plural
}

// runPrettierCheck runs Prettier formatting check/fix for a given directory.
// extensions are the file extensions to count (like []string{"*.ts", "*.svelte", "*.css", "*.js"}).
func runPrettierCheck(ctx *CheckContext, dir string, extensions []string) (CheckResult, error) {
	// Count files that prettier would check
	findArgs := buildFindArgs("src", extensions)
	findCmd := exec.Command("find", findArgs...)
	findCmd.Dir = dir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	// Check which files need formatting (--list-different lists them)
	// Note: prettier exits with code 1 if files differ, so we ignore the error
	// Prettier's default behavior respects .gitignore files in current dir and parents
	checkCmd := exec.Command("pnpm", "exec", "prettier", "--list-different", ".")
	checkCmd.Dir = dir
	checkOutput, _ := RunCommand(checkCmd, true)

	// Parse files that need formatting
	var needsFormat []string
	if strings.TrimSpace(checkOutput) != "" {
		needsFormat = strings.Split(strings.TrimSpace(checkOutput), "\n")
	}

	if ctx.CI {
		if len(needsFormat) > 0 {
			return CheckResult{}, fmt.Errorf("code is not formatted, run pnpm format locally\n%s", indentOutput(checkOutput))
		}
		return Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}

	// Non-CI mode: format if needed
	if len(needsFormat) > 0 {
		fmtCmd := exec.Command("pnpm", "format")
		fmtCmd.Dir = dir
		output, err := RunCommand(fmtCmd, true)
		if err != nil {
			return CheckResult{}, fmt.Errorf("prettier formatting failed\n%s", indentOutput(output))
		}
		return SuccessWithChanges(fmt.Sprintf("Formatted %d of %d %s", len(needsFormat), fileCount, Pluralize(fileCount, "file", "files"))), nil
	}

	return Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files"))), nil
}

// runESLintCheck runs ESLint check/fix for a given directory.
// extensions are the file extensions to count (like []string{"*.ts", "*.svelte", "*.js"}).
// If requireConfig is true, skips when eslint.config.js is missing.
func runESLintCheck(ctx *CheckContext, dir string, extensions []string, requireConfig bool) (CheckResult, error) {
	if requireConfig {
		if _, err := os.Stat(filepath.Join(dir, "eslint.config.js")); os.IsNotExist(err) {
			return Skipped("no eslint.config.js"), nil
		}
	}

	// Count lintable files
	findArgs := buildFindArgs("src", extensions)
	findCmd := exec.Command("find", findArgs...)
	findCmd.Dir = dir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	var cmd *exec.Cmd
	if ctx.CI {
		cmd = exec.Command("pnpm", "lint")
	} else {
		cmd = exec.Command("pnpm", "lint:fix")
	}
	cmd.Dir = dir
	output, err := RunCommand(cmd, true)
	if err != nil {
		if ctx.CI {
			return CheckResult{}, fmt.Errorf("lint errors found, run pnpm lint:fix locally\n%s", indentOutput(output))
		}
		return CheckResult{}, fmt.Errorf("eslint found unfixable errors\n%s", indentOutput(output))
	}

	if fileCount > 0 {
		return Success(fmt.Sprintf("%d %s passed", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}
	return Success("All files passed"), nil
}

// buildFindArgs constructs arguments for a find command to locate files with given extensions.
func buildFindArgs(searchDir string, extensions []string) []string {
	args := []string{searchDir, "-type", "f", "("}
	for i, ext := range extensions {
		if i > 0 {
			args = append(args, "-o")
		}
		args = append(args, "-name", ext)
	}
	args = append(args, ")")
	return args
}

// GetGoDirectories returns all directories in the repo that contain Go code.
// Each returned path is relative to rootDir.
func GetGoDirectories() []string {
	return []string{
		"scripts",
		"apps/desktop/scripts",
	}
}

// FindGoModules finds all go.mod files in the given directory and returns
// the directories containing them.
func FindGoModules(rootDir string) ([]string, error) {
	findCmd := exec.Command("find", ".", "-name", "go.mod", "-type", "f")
	findCmd.Dir = rootDir
	output, err := RunCommand(findCmd, true)
	if err != nil {
		return nil, err
	}

	var modules []string
	for _, line := range strings.Split(strings.TrimSpace(output), "\n") {
		if line != "" {
			// Get directory containing go.mod
			dir := strings.TrimSuffix(line, "/go.mod")
			dir = strings.TrimPrefix(dir, "./")
			if dir == "go.mod" {
				dir = "."
			}
			modules = append(modules, dir)
		}
	}
	return modules, nil
}

// FindAllGoModules finds Go modules across all Go directories in the repo.
// Returns a map of base directory to list of module subdirectories.
func FindAllGoModules(rootDir string) (map[string][]string, error) {
	result := make(map[string][]string)
	for _, goDir := range GetGoDirectories() {
		fullPath := filepath.Join(rootDir, goDir)
		modules, err := FindGoModules(fullPath)
		if err != nil {
			return nil, fmt.Errorf("failed to find modules in %s: %w", goDir, err)
		}
		result[goDir] = modules
	}
	return result, nil
}
