package main

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// PrettierCheck checks code formatting with Prettier.
type PrettierCheck struct{}

func (c *PrettierCheck) Name() string {
	return "Prettier"
}

func (c *PrettierCheck) Run(ctx *CheckContext) error {
	frontendDir := filepath.Join(ctx.RootDir, "frontend")

	// Check frontend
	checkFrontendCmd := exec.Command("pnpm", "format:check")
	checkFrontendCmd.Dir = frontendDir
	frontendOutput, frontendErr := runCommand(checkFrontendCmd, true)

	// Check e2e (now in frontend/e2e)
	checkE2ECmd := exec.Command("pnpm", "exec", "prettier", "--check", "e2e/**/*.ts")
	checkE2ECmd.Dir = frontendDir
	e2eOutput, e2eErr := runCommand(checkE2ECmd, true)

	if frontendErr != nil || e2eErr != nil {
		// Always show output on failure (before auto-fix, so we see the original errors)
		fmt.Println()
		if frontendErr != nil {
			fmt.Print(indentOutput(frontendOutput, "      "))
		}
		if e2eErr != nil {
			fmt.Print(indentOutput(e2eOutput, "      "))
		}

		if !ctx.CI {
			// Auto-fix frontend
			if frontendErr != nil {
				formatCmd := exec.Command("pnpm", "format")
				formatCmd.Dir = frontendDir
				formatCmd.Stdout = nil
				formatCmd.Stderr = nil
				if err := formatCmd.Run(); err != nil {
					return fmt.Errorf("failed to run pnpm format: %w", err)
				}
			}
			// Auto-fix e2e
			if e2eErr != nil {
				e2eFormatCmd := exec.Command("pnpm", "exec", "prettier", "--write", "e2e/**/*.ts")
				e2eFormatCmd.Dir = frontendDir
				e2eFormatCmd.Stdout = nil
				e2eFormatCmd.Stderr = nil
				if err := e2eFormatCmd.Run(); err != nil {
					return fmt.Errorf("failed to format e2e: %w", err)
				}
			}
			// Re-check after auto-fix
			recheckFrontendCmd := exec.Command("pnpm", "format:check")
			recheckFrontendCmd.Dir = frontendDir
			recheckFrontendOutput, recheckFrontendErr := runCommand(recheckFrontendCmd, true)
			recheckE2ECmd := exec.Command("pnpm", "exec", "prettier", "--check", "e2e/**/*.ts")
			recheckE2ECmd.Dir = frontendDir
			recheckE2EOutput, recheckE2EErr := runCommand(recheckE2ECmd, true)
			if recheckFrontendErr == nil && recheckE2EErr == nil {
				return nil // Fixed
			}
			// If still failing after auto-fix, show the new output
			if recheckFrontendErr != nil {
				fmt.Println()
				fmt.Println("    After auto-fix, still has errors:")
				fmt.Print(indentOutput(recheckFrontendOutput, "      "))
				if recheckE2EErr != nil {
					fmt.Print(indentOutput(recheckE2EOutput, "      "))
				}
			}
		}
		return fmt.Errorf("prettier check failed")
	}
	return nil
}

// ESLintCheck checks code with ESLint.
type ESLintCheck struct{}

func (c *ESLintCheck) Name() string {
	return "ESLint"
}

func (c *ESLintCheck) Run(ctx *CheckContext) error {
	frontendDir := filepath.Join(ctx.RootDir, "frontend")

	// Check frontend and e2e (e2e is now in frontend/e2e, so ESLint will pick it up)
	checkFrontendCmd := exec.Command("pnpm", "lint")
	checkFrontendCmd.Dir = frontendDir
	frontendOutput, frontendErr := runCommand(checkFrontendCmd, true)

	if frontendErr != nil {
		// Always show output on failure (before auto-fix, so we see the original errors)
		if strings.TrimSpace(frontendOutput) != "" {
			fmt.Println()
			fmt.Print(indentOutput(frontendOutput, "      "))
		} else {
			// If output is empty, show a helpful message
			fmt.Println()
			fmt.Println("    ESLint found errors. Run: pnpm lint")
		}

		if !ctx.CI {
			// Auto-fix frontend
			fixCmd := exec.Command("pnpm", "lint:fix")
			fixCmd.Dir = frontendDir
			// Suppress output from lint:fix
			fixCmd.Stdout = nil
			fixCmd.Stderr = nil
			if err := fixCmd.Run(); err != nil {
				return fmt.Errorf("failed to run pnpm lint:fix: %w", err)
			}
			// Re-check after auto-fix
			recheckCmd := exec.Command("pnpm", "lint")
			recheckCmd.Dir = frontendDir
			recheckOutput, recheckErr := runCommand(recheckCmd, true)
			if recheckErr == nil {
				return nil // Fixed
			}
			// If still failing after auto-fix, show the new output
			if strings.TrimSpace(recheckOutput) != "" {
				fmt.Println()
				fmt.Println("    After auto-fix, still has errors:")
				fmt.Print(indentOutput(recheckOutput, "      "))
			}
		}
		return fmt.Errorf("eslint check failed")
	}
	return nil
}

// FrontendTestsCheck runs frontend unit tests.
type FrontendTestsCheck struct{}

func (c *FrontendTestsCheck) Name() string {
	return "tests"
}

func (c *FrontendTestsCheck) Run(ctx *CheckContext) error {
	frontendDir := filepath.Join(ctx.RootDir, "frontend")
	cmd := exec.Command("pnpm", "test", "--run")
	cmd.Dir = frontendDir
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("tests failed")
	}
	return nil
}

// E2ETestsCheck runs end-to-end tests.
type E2ETestsCheck struct{}

func (c *E2ETestsCheck) Name() string {
	return "E2E tests"
}

func (c *E2ETestsCheck) Run(ctx *CheckContext) error {
	frontendDir := filepath.Join(ctx.RootDir, "frontend")
	cmd := exec.Command("pnpm", "test:e2e")
	cmd.Dir = frontendDir
	// Capture output on the first run
	output, err := runCommand(cmd, true)
	if err != nil {
		// Display captured output
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("e2e tests failed")
	}
	return nil
}
