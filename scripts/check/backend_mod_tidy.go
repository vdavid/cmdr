package main

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// GoModTidyCheck checks if go.mod and go.sum are tidy.
type GoModTidyCheck struct{}

func (c *GoModTidyCheck) Name() string {
	return "go mod tidy"
}

func (c *GoModTidyCheck) Run(ctx *CheckContext) error {
	backendDir := filepath.Join(ctx.RootDir, "backend")
	goModPath := filepath.Join(backendDir, "go.mod")
	if _, err := os.Stat(goModPath); os.IsNotExist(err) {
		return nil // No go.mod, skip
	}

	// Create temporary copies
	goModBak := goModPath + ".bak"
	goSumPath := filepath.Join(backendDir, "go.sum")
	goSumBak := goSumPath + ".bak"

	// Backup files
	if err := copyFile(goModPath, goModBak); err != nil {
		return fmt.Errorf("failed to backup go.mod: %w", err)
	}
	if _, err := os.Stat(goSumPath); err == nil {
		if err := copyFile(goSumPath, goSumBak); err != nil {
			err := os.Remove(goModBak)
			if err != nil {
				return err
			}
			return fmt.Errorf("failed to backup go.sum: %w", err)
		}
	}

	// Run go mod tidy
	cmd := exec.Command("go", "mod", "tidy")
	cmd.Dir = backendDir
	cmd.Env = append(os.Environ(), "GOTOOLCHAIN=auto")
	if err := cmd.Run(); err != nil {
		restoreFiles(goModPath, goModBak, goSumPath, goSumBak)
		return fmt.Errorf("go mod tidy failed: %w", err)
	}

	// Check if files changed
	modChanged := filesDiffer(goModPath, goModBak)
	sumChanged := false
	if _, err := os.Stat(goSumPath); err == nil {
		sumChanged = filesDiffer(goSumPath, goSumBak)
	}

	// Restore files
	restoreFiles(goModPath, goModBak, goSumPath, goSumBak)

	if modChanged || sumChanged {
		fmt.Println()
		fmt.Println("    Module files need tidying. Run: go mod tidy")
		return fmt.Errorf("go.mod or go.sum needs tidying")
	}

	return nil
}

// copyFile copies a file from src to dst.
func copyFile(src, dst string) error {
	data, err := os.ReadFile(src)
	if err != nil {
		return err
	}
	return os.WriteFile(dst, data, 0644)
}

// filesDiffer checks if two files differ.
func filesDiffer(file1, file2 string) bool {
	data1, err1 := os.ReadFile(file1)
	data2, err2 := os.ReadFile(file2)
	if err1 != nil || err2 != nil {
		return true
	}
	return !bytes.Equal(data1, data2)
}

// restoreFiles restores backup files.
func restoreFiles(goModPath, goModBak, goSumPath, goSumBak string) {
	if _, err := os.Stat(goModBak); err == nil {
		err := os.Rename(goModBak, goModPath)
		if err != nil {
			fmt.Printf("failed to restore go.mod: %v\n", err)
			return
		}
	}
	if _, err := os.Stat(goSumBak); err == nil {
		err := os.Rename(goSumBak, goSumPath)
		if err != nil {
			fmt.Printf("failed to restore go.sum: %v\n", err)
			return
		}
	}
}
