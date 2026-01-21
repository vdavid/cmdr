// Big files scenario generator for test data.
// Creates folders with large files or many files totaling significant disk space.
package main

import (
	cryptoRand "crypto/rand"
	"fmt"
	"os"
	"path/filepath"
)

// ============================================================================
// Configuration - adjust these values as needed
// ============================================================================

// Size constants for convenience
const (
	KB = 1024
	MB = 1024 * KB
	GB = 1024 * MB
)

// BigFilesScenario defines a test scenario with large files.
type BigFilesScenario struct {
	Name        string // Folder name
	TotalSize   int64  // Total size in bytes
	FileCount   int    // Number of files to create
	DirCount    int    // Number of directories (0 = flat structure)
	Description string // Human-readable description
}

// BigFilesScenarios defines the scenarios to generate.
// Modify this slice to add, remove, or adjust scenarios.
var BigFilesScenarios = []BigFilesScenario{
	{
		Name:        "big-files-100-files-total-1GB",
		TotalSize:   1 * GB,
		FileCount:   100,
		DirCount:    0,
		Description: "100 files totaling 1GB (~10MB each)",
	},
	{
		Name:        "big-files-one-file-5GB",
		TotalSize:   5 * GB,
		FileCount:   1,
		DirCount:    0,
		Description: "Single 5GB file",
	},
	{
		Name:        "big-files-100k-files-and-dirs-total-2GB",
		TotalSize:   2 * GB,
		FileCount:   100000,
		DirCount:    1000, // 1000 directories with ~100 files each
		Description: "100k files in 1000 directories totaling 2GB",
	},
}

// ============================================================================
// Implementation
// ============================================================================

// createBigFile creates a file with random data of the specified size.
func createBigFile(path string, size int64) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer func(f *os.File) {
		_ = f.Close()
	}(f)

	// Write random data in chunks for better performance
	const chunkSize = 64 * MB
	chunk := make([]byte, chunkSize)

	written := int64(0)
	for written < size {
		toWrite := chunkSize
		if size-written < int64(chunkSize) {
			toWrite = int(size - written)
		}

		// Fill chunk with random data
		_, _ = cryptoRand.Read(chunk[:toWrite])

		n, err := f.Write(chunk[:toWrite])
		if err != nil {
			return err
		}
		written += int64(n)
	}

	return nil
}

// bigFilesFolderNeedsRecreation checks if the folder needs to be recreated.
// Returns true if the folder doesn't exist or has incorrect size.
func bigFilesFolderNeedsRecreation(folderPath string, scenario BigFilesScenario) bool {
	info, err := os.Stat(folderPath)
	if err != nil || !info.IsDir() {
		return true
	}

	var currentSize int64
	var fileCount int
	err = filepath.Walk(folderPath, func(_ string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		if !info.IsDir() {
			currentSize += info.Size()
			fileCount++
		}
		return nil
	})

	if err != nil {
		return true
	}

	withinRange := currentSize >= scenario.TotalSize*95/100 && currentSize <= scenario.TotalSize*105/100
	if withinRange {
		fmt.Printf("    Already exists with ~%d files, %.2f GB - skipping\n",
			fileCount, float64(currentSize)/float64(GB))
		return false
	}
	return true
}

// createHierarchicalBigFiles creates files organized in directories.
func createHierarchicalBigFiles(folderPath string, scenario BigFilesScenario) error {
	filesPerDir := scenario.FileCount / scenario.DirCount
	fileSize := scenario.TotalSize / int64(scenario.FileCount)

	fmt.Printf("    Creating %d directories with ~%d files each (~%d KB per file)...\n",
		scenario.DirCount, filesPerDir, fileSize/KB)

	fileIndex := 0
	for d := range scenario.DirCount {
		dirPath := filepath.Join(folderPath, fmt.Sprintf("dir-%05d", d))
		if err := os.MkdirAll(dirPath, 0755); err != nil {
			return fmt.Errorf("failed to create directory %d: %w", d, err)
		}

		for f := 0; f < filesPerDir && fileIndex < scenario.FileCount; f++ {
			filePath := filepath.Join(dirPath, fmt.Sprintf("file-%06d.dat", fileIndex))
			if err := createBigFile(filePath, fileSize); err != nil {
				return fmt.Errorf("failed to create file %d: %w", fileIndex, err)
			}
			fileIndex++
			if fileIndex%10000 == 0 {
				fmt.Printf("    Created %d files...\n", fileIndex)
			}
		}

		if (d+1)%100 == 0 {
			fmt.Printf("    Created %d directories...\n", d+1)
		}
	}
	fmt.Printf("    Created %d files in %d directories\n", fileIndex, scenario.DirCount)
	return nil
}

// createFlatBigFiles creates files in a single directory.
func createFlatBigFiles(folderPath string, scenario BigFilesScenario) error {
	fileSize := scenario.TotalSize / int64(scenario.FileCount)
	fmt.Printf("    Creating %d files (~%.2f MB each)...\n",
		scenario.FileCount, float64(fileSize)/float64(MB))

	for i := range scenario.FileCount {
		filePath := filepath.Join(folderPath, fmt.Sprintf("file-%06d.dat", i))
		if err := createBigFile(filePath, fileSize); err != nil {
			return fmt.Errorf("failed to create file %d: %w", i, err)
		}

		if (i+1)%10 == 0 || i == scenario.FileCount-1 {
			pct := float64(i+1) / float64(scenario.FileCount) * 100
			fmt.Printf("    Progress: %.0f%% (%d/%d files)\n", pct, i+1, scenario.FileCount)
		}
	}
	return nil
}

// SyncBigFilesScenario ensures the big-files scenario folder is in the desired state.
func SyncBigFilesScenario(baseDir string, scenario BigFilesScenario) error {
	folderPath := filepath.Join(baseDir, scenario.Name)
	fmt.Printf("  %s: %s\n", scenario.Name, scenario.Description)

	if !bigFilesFolderNeedsRecreation(folderPath, scenario) {
		return nil
	}

	fmt.Printf("    Creating fresh folder...\n")
	if err := os.RemoveAll(folderPath); err != nil {
		return fmt.Errorf("failed to remove existing folder: %w", err)
	}
	if err := os.MkdirAll(folderPath, 0755); err != nil {
		return fmt.Errorf("failed to create folder: %w", err)
	}

	var err error
	if scenario.DirCount > 0 {
		err = createHierarchicalBigFiles(folderPath, scenario)
	} else {
		err = createFlatBigFiles(folderPath, scenario)
	}
	if err != nil {
		return err
	}

	fmt.Printf("    Done!\n")
	return nil
}
