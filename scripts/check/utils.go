package main

import (
	"fmt"
	"os"
	"path/filepath"
)

// findRootDir finds the project root directory.
// For monorepo structure, it looks for apps/desktop/src-tauri/Cargo.toml.
func findRootDir() (string, error) {
	dir, err := os.Getwd()
	if err != nil {
		return "", err
	}

	for {
		// Check for monorepo structure: apps/desktop/src-tauri/Cargo.toml
		monorepoCargoToml := filepath.Join(dir, "apps", "desktop", "src-tauri", "Cargo.toml")
		if _, err := os.Stat(monorepoCargoToml); err == nil {
			return dir, nil
		}

		// Fallback: old structure with src-tauri at root
		tauriCargoToml := filepath.Join(dir, "src-tauri", "Cargo.toml")
		packageJson := filepath.Join(dir, "package.json")
		if _, err := os.Stat(tauriCargoToml); err == nil {
			if _, err := os.Stat(packageJson); err == nil {
				return dir, nil
			}
		}

		parent := filepath.Dir(dir)
		if parent == dir {
			return "", fmt.Errorf("could not find project root (looking for apps/desktop/src-tauri/Cargo.toml or src-tauri/Cargo.toml)")
		}
		dir = parent
	}
}
