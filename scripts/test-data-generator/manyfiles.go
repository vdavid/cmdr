// Many-files folder sync for test data.
// Creates folders with thousands of small markdown files.
package main

import (
	"fmt"
	"math/rand"
	"os"
	"path/filepath"
	"strings"
)

// ============================================================================
// Configuration - adjust these values as needed
// ============================================================================

// ManyFilesTargets defines folder names and their target file counts.
// Folder names are relative to the test data base directory.
var ManyFilesTargets = map[string]int{
	"folder with 1000 files":   1000,
	"folder with 5000 files":   5000,
	"folder with 20000 files":  20000,
	"folder with 50000 files":  50000,
	"folder with 100000 files": 100000,
	"folder with 200000 files": 200000,
}

// ============================================================================
// Implementation
// ============================================================================

// deleteFilesToTarget deletes random files from a folder to reach the target count.
func deleteFilesToTarget(folderPath string, existingFiles []string, deleteCount int) error {
	fmt.Printf("  Deleting %d files", deleteCount)

	// Shuffle and pick first N to delete
	rand.Shuffle(len(existingFiles), func(i, j int) {
		existingFiles[i], existingFiles[j] = existingFiles[j], existingFiles[i]
	})

	for i := range deleteCount {
		filePath := filepath.Join(folderPath, existingFiles[i])
		if err := os.Remove(filePath); err != nil {
			return fmt.Errorf("failed to delete %s: %w", filePath, err)
		}
		if (i+1)%5000 == 0 {
			fmt.Print(".")
		}
	}
	fmt.Println(" done")
	return nil
}

// createFilesToTarget creates files with random timestamps to reach the target count.
func createFilesToTarget(folderPath string, usedTimestamps map[string]bool, createCount int) error {
	fmt.Printf("  Creating %d files", createCount)

	created := 0
	for created < createCount {
		ts := generateTimestamp()
		filename := ts.Format("2006-01-02 15-04-05") + ".md"

		if usedTimestamps[filename] {
			continue // Try another timestamp
		}
		usedTimestamps[filename] = true

		filePath := filepath.Join(folderPath, filename)
		content := generateSentence()

		if err := os.WriteFile(filePath, []byte(content), 0644); err != nil {
			return fmt.Errorf("failed to write %s: %w", filePath, err)
		}

		created++
		if created%5000 == 0 {
			fmt.Print(".")
		}
	}
	fmt.Println(" done")
	return nil
}

// SyncManyFilesFolder ensures a folder has exactly targetCount files, creating or deleting as needed.
func SyncManyFilesFolder(folderPath string, targetCount int) error {
	if err := os.MkdirAll(folderPath, 0755); err != nil {
		return fmt.Errorf("failed to create folder %s: %w", folderPath, err)
	}

	entries, err := os.ReadDir(folderPath)
	if err != nil {
		return fmt.Errorf("failed to read folder %s: %w", folderPath, err)
	}

	existingFiles := make([]string, 0, len(entries))
	for _, entry := range entries {
		if !entry.IsDir() && strings.HasSuffix(entry.Name(), ".md") {
			existingFiles = append(existingFiles, entry.Name())
		}
	}

	currentCount := len(existingFiles)
	fmt.Printf("  %s: %d files exist, target is %d\n", filepath.Base(folderPath), currentCount, targetCount)

	switch {
	case currentCount > targetCount:
		return deleteFilesToTarget(folderPath, existingFiles, currentCount-targetCount)
	case currentCount < targetCount:
		usedTimestamps := make(map[string]bool)
		for _, name := range existingFiles {
			usedTimestamps[name] = true
		}
		return createFilesToTarget(folderPath, usedTimestamps, targetCount-currentCount)
	default:
		fmt.Println("  Already at target, no changes needed")
		return nil
	}
}
