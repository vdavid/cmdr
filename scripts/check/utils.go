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
		cargoToml := filepath.Join(dir, "apps", "desktop", "src-tauri", "Cargo.toml")
		if _, err := os.Stat(cargoToml); err == nil {
			return dir, nil
		}

		parent := filepath.Dir(dir)
		if parent == dir {
			return "", fmt.Errorf("could not find project root (looking for apps/desktop/src-tauri/Cargo.toml)")
		}
		dir = parent
	}
}
