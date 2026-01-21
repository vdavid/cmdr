// Test data generator for Cmdr.
// Creates folders with files for testing file manager performance and features.
// Run with: go run scripts/test-data-generator/main.go
//
// Configuration is done by editing the constants in each module:
//   - manyfiles.go: ManyFilesTargets (folder names and file counts)
//   - bigfiles.go:  BigFilesScenarios (size, file count, directory count)
//   - icons.go:     File types and folder icons for icon testing
package main

import (
	"fmt"
	"os"
	"path/filepath"
)

// Base directories for test data
const (
	baseDir     = "_ignored/test-data"
	bigFilesDir = "_ignored/test-data/big-files"
)

func main() {
	fmt.Printf("Syncing test data folders in %s/\n\n", baseDir)

	// Create icon test data first
	if err := CreateIconTestData(baseDir); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error creating icon test data: %v\n", err)
		os.Exit(1)
	}
	fmt.Println()

	// Sync large file count folders
	for folderName, target := range ManyFilesTargets {
		folderPath := filepath.Join(baseDir, folderName)
		if err := SyncManyFilesFolder(folderPath, target); err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		fmt.Println()
	}

	// Create big-files test data
	fmt.Printf("Syncing big-files test data in %s/\n", bigFilesDir)
	if err := os.MkdirAll(bigFilesDir, 0755); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error creating big-files directory: %v\n", err)
		os.Exit(1)
	}

	for _, scenario := range BigFilesScenarios {
		if err := SyncBigFilesScenario(bigFilesDir, scenario); err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "Error creating big-files scenario %s: %v\n", scenario.Name, err)
			os.Exit(1)
		}
	}
	fmt.Println()

	fmt.Println("All folders synced successfully!")
}
