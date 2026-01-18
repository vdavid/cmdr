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

// Warning creates a warning result with the given message.
func Warning(message string) CheckResult {
	return CheckResult{Code: ResultWarning, Message: message}
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

// EnsureGoTool ensures a Go tool is installed and in PATH.
// If the tool isn't in PATH after installation, adds GOPATH/bin to PATH.
func EnsureGoTool(name, installPath string) error {
	if CommandExists(name) {
		return nil
	}

	installCmd := exec.Command("go", "install", installPath)
	if _, err := RunCommand(installCmd, true); err != nil {
		return fmt.Errorf("failed to install %s: %w", name, err)
	}

	// After installation, add GOPATH/bin to PATH if tool still not found
	if !CommandExists(name) {
		gopath := os.Getenv("GOPATH")
		if gopath == "" {
			// Default GOPATH is ~/go
			home, _ := os.UserHomeDir()
			gopath = filepath.Join(home, "go")
		}
		goBin := filepath.Join(gopath, "bin")
		os.Setenv("PATH", goBin+string(os.PathListSeparator)+os.Getenv("PATH"))
	}

	return nil
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
