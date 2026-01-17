package checks

import (
	"bytes"
	"os"
	"os/exec"
	"strings"
)

// App represents the application a check belongs to.
type App string

const (
	AppDesktop       App = "desktop"
	AppWebsite       App = "website"
	AppLicenseServer App = "license-server"
	AppOther         App = "other"
)

// ResultCode indicates the outcome of a check.
type ResultCode int

const (
	ResultSuccess ResultCode = iota
	ResultWarning
	ResultSkipped
)

// CheckResult is returned by checks on success.
type CheckResult struct {
	Code    ResultCode
	Message string
}

// Success creates a success result with the given message.
func Success(message string) CheckResult {
	return CheckResult{Code: ResultSuccess, Message: message}
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
