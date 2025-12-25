package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// GofmtCheck checks Go code formatting.
type GofmtCheck struct{}

func (c *GofmtCheck) Name() string {
	return "gofmt"
}

func (c *GofmtCheck) Run(ctx *CheckContext) error {
	// Check both backend/ and scripts/
	paths := []string{
		filepath.Join(ctx.RootDir, "backend"),
		filepath.Join(ctx.RootDir, "scripts"),
	}

	var unformatted []string
	for _, path := range paths {
		cmd := exec.Command("gofmt", "-s", "-l", ".")
		cmd.Dir = path
		output, err := runCommand(cmd, true)
		if err != nil {
			// gofmt returns non-zero if files need formatting, so check output
			lines := strings.Split(strings.TrimSpace(output), "\n")
			for _, line := range lines {
				if strings.TrimSpace(line) != "" {
					unformatted = append(unformatted, filepath.Join(path, line))
				}
			}
		}
	}

	if len(unformatted) > 0 {
		// Always show output on failure (before auto-fix, so we see the original errors)
		fmt.Println()
		fmt.Println("    Files not formatted:")
		for _, file := range unformatted {
			fmt.Printf("      %s\n", file)
		}

		if !ctx.CI {
			// Auto-fix
			for _, path := range paths {
				cmd := exec.Command("gofmt", "-s", "-w", ".")
				cmd.Dir = path
				if err := cmd.Run(); err != nil {
					return fmt.Errorf("failed to run gofmt -w: %w", err)
				}
			}
			// Re-check
			unformattedAfterFix := []string{}
			for _, path := range paths {
				cmd := exec.Command("gofmt", "-s", "-l", ".")
				cmd.Dir = path
				output, _ := runCommand(cmd, true)
				lines := strings.Split(strings.TrimSpace(output), "\n")
				for _, line := range lines {
					if strings.TrimSpace(line) != "" {
						unformattedAfterFix = append(unformattedAfterFix, filepath.Join(path, line))
					}
				}
			}
			if len(unformattedAfterFix) == 0 {
				return nil // Fixed
			}
			// If still failing after auto-fix, show the remaining files
			fmt.Println()
			fmt.Println("    After auto-fix, still not formatted:")
			for _, file := range unformattedAfterFix {
				fmt.Printf("      %s\n", file)
			}
		}
		return fmt.Errorf("files need formatting")
	}
	return nil
}

// GovulncheckCheck checks for security vulnerabilities.
type GovulncheckCheck struct{}

func (c *GovulncheckCheck) Name() string {
	return "govulncheck"
}

func (c *GovulncheckCheck) Run(ctx *CheckContext) error {
	addGoPathToPath()
	if err := ensureToolInstalled("govulncheck", "go install golang.org/x/vuln/cmd/govulncheck@latest", ctx.RootDir); err != nil {
		return fmt.Errorf("failed to install govulncheck: %w", err)
	}

	// govulncheck requires a module context, so only run on backend/
	backendDir := filepath.Join(ctx.RootDir, "backend")
	cmd := exec.Command(getToolPath("govulncheck"), "./...")
	cmd.Dir = backendDir
	cmd.Env = append(os.Environ(), "GOTOOLCHAIN=auto")
	output, err := runCommand(cmd, true)

	// Only fail if the code is actually affected by vulnerabilities
	// The output will say "Your code is affected by 0 vulnerabilities" if safe
	if strings.Contains(output, "Your code is affected by") {
		// Check if it says "0 vulnerabilities" - if so, it's safe
		if strings.Contains(output, "Your code is affected by 0 vulnerabilities") {
			return nil // Code is not affected, pass
		}
		// Code is affected by vulnerabilities, fail
		fmt.Println()
		fmt.Println("    Vulnerabilities found in your code:")
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("vulnerabilities found in code")
	}

	// If we get here and there was an error, it might be a different issue
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("govulncheck failed: %w", err)
	}

	return nil
}

// GoVetCheck runs go vet for static analysis.
type GoVetCheck struct{}

func (c *GoVetCheck) Name() string {
	return "go vet"
}

func (c *GoVetCheck) Run(ctx *CheckContext) error {
	// go vet requires a module context, so only run on backend/
	backendDir := filepath.Join(ctx.RootDir, "backend")
	cmd := exec.Command("go", "vet", "./...")
	cmd.Dir = backendDir
	cmd.Env = append(os.Environ(), "GOTOOLCHAIN=auto")
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("go vet failed")
	}
	return nil
}

// StaticcheckCheck runs staticcheck for advanced static analysis.
type StaticcheckCheck struct{}

func (c *StaticcheckCheck) Name() string {
	return "staticcheck"
}

func (c *StaticcheckCheck) Run(ctx *CheckContext) error {
	addGoPathToPath()
	if err := ensureToolInstalled("staticcheck", "go install honnef.co/go/tools/cmd/staticcheck@latest", ctx.RootDir); err != nil {
		return fmt.Errorf("failed to install staticcheck: %w", err)
	}

	// staticcheck requires a module context, so only run on backend/
	backendDir := filepath.Join(ctx.RootDir, "backend")
	cmd := exec.Command(getToolPath("staticcheck"), "./...")
	cmd.Dir = backendDir
	cmd.Env = append(os.Environ(), "GOTOOLCHAIN=auto")
	output, err := runCommand(cmd, true)
	if err != nil {
		// Check if it's a module error (shouldn't happen for backend, but handle gracefully)
		if strings.Contains(output, "no main module") || strings.Contains(output, "does not contain main module") {
			return nil // Skip if no module context
		}
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("staticcheck failed")
	}
	return nil
}

// IneffassignCheck checks for ineffective assignments.
type IneffassignCheck struct{}

func (c *IneffassignCheck) Name() string {
	return "ineffassign"
}

func (c *IneffassignCheck) Run(ctx *CheckContext) error {
	addGoPathToPath()
	if err := ensureToolInstalled("ineffassign", "go install github.com/gordonklaus/ineffassign@latest", ctx.RootDir); err != nil {
		return fmt.Errorf("failed to install ineffassign: %w", err)
	}

	// ineffassign requires a module context, so only run on backend/
	backendDir := filepath.Join(ctx.RootDir, "backend")
	cmd := exec.Command(getToolPath("ineffassign"), "./...")
	cmd.Dir = backendDir
	output, err := runCommand(cmd, true)
	if err != nil {
		// Check if it's a module error (shouldn't happen for backend, but handle gracefully)
		if strings.Contains(output, "no main module") || strings.Contains(output, "does not contain main module") {
			return nil // Skip if no module context
		}
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("ineffassign failed")
	}
	return nil
}

// MisspellCheck checks for spelling errors.
type MisspellCheck struct{}

func (c *MisspellCheck) Name() string {
	return "misspell"
}

func (c *MisspellCheck) Run(ctx *CheckContext) error {
	addGoPathToPath()
	if err := ensureToolInstalled("misspell", "go install github.com/client9/misspell/cmd/misspell@latest", ctx.RootDir); err != nil {
		return fmt.Errorf("failed to install misspell: %w", err)
	}

	paths := []string{
		filepath.Join(ctx.RootDir, "backend"),
		filepath.Join(ctx.RootDir, "scripts"),
	}

	for _, path := range paths {
		cmd := exec.Command(getToolPath("misspell"), "-error", ".")
		cmd.Dir = path
		output, err := runCommand(cmd, true)
		if err != nil {
			fmt.Println()
			fmt.Print(indentOutput(output, "      "))
			return fmt.Errorf("misspell failed in %s", path)
		}
	}
	return nil
}

// GocycloCheck checks for cyclomatic complexity.
type GocycloCheck struct{}

func (c *GocycloCheck) Name() string {
	return "gocyclo (complexity > 15, excluding tests)"
}

func (c *GocycloCheck) Run(ctx *CheckContext) error {
	addGoPathToPath()
	if err := ensureToolInstalled("gocyclo", "go install github.com/fzipp/gocyclo/cmd/gocyclo@latest", ctx.RootDir); err != nil {
		return fmt.Errorf("failed to install gocyclo: %w", err)
	}

	paths := []string{
		filepath.Join(ctx.RootDir, "backend"),
		filepath.Join(ctx.RootDir, "scripts"),
	}

	var complexFuncs []string
	for _, path := range paths {
		// Find all Go files that are not test files
		err := filepath.Walk(path, func(filePath string, info os.FileInfo, err error) error {
			if err != nil {
				return nil
			}
			if info.IsDir() {
				return nil
			}
			if !strings.HasSuffix(filePath, ".go") {
				return nil
			}
			if strings.HasSuffix(filePath, "_test.go") {
				return nil
			}

			cmd := exec.Command(getToolPath("gocyclo"), "-over", "15", filePath)
			output, err := runCommand(cmd, true)
			if err == nil && strings.TrimSpace(output) != "" {
				lines := strings.Split(strings.TrimSpace(output), "\n")
				for _, line := range lines {
					if strings.TrimSpace(line) != "" {
						complexFuncs = append(complexFuncs, line)
					}
				}
			}
			return nil
		})
		if err != nil {
			return fmt.Errorf("failed to walk %s: %w", path, err)
		}
	}

	if len(complexFuncs) > 0 {
		fmt.Printf("%sWARN%s (%d functions)\n", colorYellow, colorReset, len(complexFuncs))
		fmt.Println()
		// Show first 5
		maxShow := 5
		if len(complexFuncs) < maxShow {
			maxShow = len(complexFuncs)
		}
		for i := 0; i < maxShow; i++ {
			fmt.Printf("      %s\n", complexFuncs[i])
		}
		return nil // Don't fail, just warn
	}
	return nil
}

// NilawayCheck checks for nil pointer issues.
type NilawayCheck struct{}

func (c *NilawayCheck) Name() string {
	return "nilaway"
}

func (c *NilawayCheck) Run(ctx *CheckContext) error {
	addGoPathToPath()
	if err := ensureToolInstalled("nilaway", "go install go.uber.org/nilaway/cmd/nilaway@latest", ctx.RootDir); err != nil {
		return fmt.Errorf("failed to install nilaway: %w", err)
	}

	// nilaway requires a module context, so only run on backend/
	backendDir := filepath.Join(ctx.RootDir, "backend")
	cmd := exec.Command(getToolPath("nilaway"), "./...")
	cmd.Dir = backendDir
	cmd.Env = append(os.Environ(), "GOTOOLCHAIN=auto")
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("nilaway failed")
	}
	return nil
}

// BackendTestsCheck runs backend tests.
type BackendTestsCheck struct{}

func (c *BackendTestsCheck) Name() string {
	return "tests"
}

func (c *BackendTestsCheck) Run(ctx *CheckContext) error {
	backendDir := filepath.Join(ctx.RootDir, "backend")
	cmd := exec.Command("go", "test", "./...")
	cmd.Dir = backendDir
	cmd.Env = append(os.Environ(), "GOTOOLCHAIN=auto")
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("tests failed")
	}
	return nil
}
