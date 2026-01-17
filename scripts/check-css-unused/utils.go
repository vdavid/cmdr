package main

import (
	"fmt"
	"os"
	"path/filepath"
)

// findRootDir finds the project root directory by looking for the monorepo marker.
func findRootDir() (string, error) {
	dir, err := os.Getwd()
	if err != nil {
		return "", err
	}

	for {
		monorepoMarker := filepath.Join(dir, "apps", "desktop", "src-tauri", "Cargo.toml")
		if _, err := os.Stat(monorepoMarker); err == nil {
			return dir, nil
		}

		parent := filepath.Dir(dir)
		if parent == dir {
			return "", fmt.Errorf("could not find project root (looking for apps/desktop/src-tauri/Cargo.toml)")
		}
		dir = parent
	}
}
