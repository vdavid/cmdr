package main

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// runCommand executes a command and optionally captures its output.
func runCommand(cmd *exec.Cmd, captureOutput bool) (string, error) {
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

// commandExists checks if a command exists in PATH.
func commandExists(name string) bool {
	_, err := exec.LookPath(name)
	return err == nil
}

// getToolPath returns the full path to a Go tool, checking PATH and GOBIN.
// Returns the tool name unchanged if not found (will fail when executed).
func getToolPath(name string) string {
	// First check if it's in PATH
	if path, err := exec.LookPath(name); err == nil {
		return path
	}

	// Check in GOBIN
	gobin := getGoBin()
	if gobin != "" {
		toolPath := filepath.Join(gobin, name)
		if _, err := os.Stat(toolPath); err == nil {
			return toolPath
		}
	}

	// Return the name as-is, will fail when executed
	return name
}

// getGoPath returns the GOPATH environment variable.
func getGoPath() string {
	cmd := exec.Command("go", "env", "GOPATH")
	output, err := cmd.Output()
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(output))
}

// getGoBin returns the GOBIN or GOPATH/bin directory where go install puts binaries.
func getGoBin() string {
	// First check GOBIN
	cmd := exec.Command("go", "env", "GOBIN")
	output, err := cmd.Output()
	if err == nil && strings.TrimSpace(string(output)) != "" {
		return strings.TrimSpace(string(output))
	}
	// Fall back to GOPATH/bin
	gopath := getGoPath()
	if gopath != "" {
		return filepath.Join(gopath, "bin")
	}
	return ""
}

// ensureToolInstalled ensures a Go tool is installed, installing it if necessary.
// rootDir should be the project root directory.
func ensureToolInstalled(toolName, installCmd, rootDir string) error {
	gobin := getGoBin()
	expectedPath := filepath.Join(gobin, toolName)

	// Check if already installed
	if commandExists(toolName) {
		return nil
	}
	if gobin != "" {
		if _, err := os.Stat(expectedPath); err == nil {
			return nil
		}
	}

	fmt.Printf("%sInstalling %s...%s ", colorYellow, toolName, colorReset)

	parts := strings.Fields(installCmd)
	cmd := exec.Command(parts[0], parts[1:]...)
	// Run from backend directory for proper Go module context
	backendDir := filepath.Join(rootDir, "backend")
	cmd.Dir = backendDir
	cmd.Env = append(os.Environ(), "GOTOOLCHAIN=auto")

	// Capture output to display on error
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Printf("      Command: %s\n", installCmd)
		fmt.Printf("      Working dir: %s\n", backendDir)
		fmt.Printf("      GOBIN: %s\n", gobin)
		if output != "" {
			fmt.Printf("      Output:\n")
			fmt.Print(indentOutput(output, "        "))
		} else {
			fmt.Printf("      Output: (empty)\n")
		}
		return fmt.Errorf("failed to run '%s': %w", installCmd, err)
	}

	return nil
}

// addGoPathToPath adds GOPATH/bin to PATH if not already present.
func addGoPathToPath() {
	gopath := getGoPath()
	if gopath == "" {
		return
	}
	gopathBin := filepath.Join(gopath, "bin")
	path := os.Getenv("PATH")
	if !strings.Contains(path, gopathBin) {
		err := os.Setenv("PATH", gopathBin+string(os.PathListSeparator)+path)
		if err != nil {
			fmt.Printf("Warning: Failed to add %s to PATH: %v\n", gopathBin, err)
			return
		}
	}
}

// findRootDir finds the project root directory by looking for backend/go.mod and frontend/package.json.
func findRootDir() (string, error) {
	dir, err := os.Getwd()
	if err != nil {
		return "", err
	}

	for {
		// Check if this is the project root by looking for backend/go.mod and frontend/package.json
		backendGoMod := filepath.Join(dir, "backend", "go.mod")
		frontendPackageJson := filepath.Join(dir, "frontend", "package.json")
		if _, err := os.Stat(backendGoMod); err == nil {
			if _, err := os.Stat(frontendPackageJson); err == nil {
				return dir, nil
			}
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", fmt.Errorf("could not find project root (looking for backend/go.mod and frontend/package.json)")
		}
		dir = parent
	}
}
