// Icon test data generator.
// Creates files with various extensions and folders with custom icons for testing.
package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// CreateIconTestData creates a folder with various file types for testing icons.
// Includes: fake files with different extensions, symlinks, and folders with custom icons.
func CreateIconTestData(baseDir string) error {
	iconDir := filepath.Join(baseDir, "icons")
	fmt.Printf("Creating icon test data in %s/\n", iconDir)

	// Clean and recreate the folder
	if err := os.RemoveAll(iconDir); err != nil {
		return fmt.Errorf("failed to remove existing icon folder: %w", err)
	}
	if err := os.MkdirAll(iconDir, 0755); err != nil {
		return fmt.Errorf("failed to create icon folder: %w", err)
	}

	// Create fake files with various extensions
	fakeFiles := []string{
		"fake-report.pdf",
		"fake-document.docx",
		"fake-spreadsheet.xlsx",
		"fake-notes.txt",
		"fake-script.ts",
		"fake-program.go",
		"fake-code.rs",
		"fake-config.json",
		"fake-data.csv",
		"fake-archive.zip",
		"fake-photo.jpg",
		"fake-image.png",
		"fake-video.mp4",
		"fake-audio.mp3",
		"fake-presentation.pptx",
		"fake-database.db",
		"fake-markup.html",
		"fake-styles.css",
		"fake-readme.md",
		"fake-shell.sh",
	}

	fmt.Printf("  Creating %d fake files...\n", len(fakeFiles))
	for _, name := range fakeFiles {
		filePath := filepath.Join(iconDir, name)
		content := fmt.Sprintf("This is a fake %s file for icon testing.\n", filepath.Ext(name))
		if err := os.WriteFile(filePath, []byte(content), 0644); err != nil {
			return fmt.Errorf("failed to create %s: %w", name, err)
		}
	}

	// Create a symlink to a file
	fmt.Println("  Creating symlinks...")
	symlinkPath := filepath.Join(iconDir, "symlink-to-fake-photo.jpg")
	if err := os.Symlink("fake-photo.jpg", symlinkPath); err != nil {
		return fmt.Errorf("failed to create symlink to file: %w", err)
	}

	// Create a symlink to a folder (for testing symlink folder navigation)
	symlinkToFolderPath := filepath.Join(iconDir, "symlink-to-regular-folder")
	// Create the target folder first (will be created below, so use absolute path)
	regularFolder := filepath.Join(iconDir, "regular-folder")
	if err := os.MkdirAll(regularFolder, 0755); err != nil {
		return fmt.Errorf("failed to create regular folder: %w", err)
	}
	if err := os.Symlink("regular-folder", symlinkToFolderPath); err != nil {
		return fmt.Errorf("failed to create symlink to folder: %w", err)
	}

	// Create folders with custom icons
	assetsDir := "scripts/test-data-generator/assets/icons"
	iconColors := []string{"red", "blue", "green", "yellow"}

	fmt.Printf("  Creating %d folders with custom icons...\n", len(iconColors))
	for _, color := range iconColors {
		folderName := fmt.Sprintf("%s-folder", color)
		folderPath := filepath.Join(iconDir, folderName)

		if err := os.MkdirAll(folderPath, 0755); err != nil {
			return fmt.Errorf("failed to create folder %s: %w", folderName, err)
		}

		// Create a readme inside the folder
		readmePath := filepath.Join(folderPath, "README.md")
		colorTitle := strings.ToUpper(color[:1]) + color[1:]
		readmeContent := fmt.Sprintf("# %s folder\n\nThis folder has a custom %s circle icon.\n", colorTitle, color)
		if err := os.WriteFile(readmePath, []byte(readmeContent), 0644); err != nil {
			return fmt.Errorf("failed to create README in %s: %w", folderName, err)
		}

		// Apply custom icon using fileicon CLI (macOS only)
		icnsPath := filepath.Join(assetsDir, fmt.Sprintf("%s-circle.icns", color))
		if _, err := os.Stat(icnsPath); err == nil {
			// fileicon is available via: brew install fileicon
			cmd := exec.Command("fileicon", "set", folderPath, icnsPath)
			if err := cmd.Run(); err != nil {
				fmt.Printf("  Warning: failed to set icon for %s (install fileicon: brew install fileicon)\n", folderName)
			}
		}
	}

	// Add README to regular folder (already created earlier as symlink target)
	readmeContent := "# Regular folder\n\nThis folder has the default macOS folder icon.\n"
	if err := os.WriteFile(filepath.Join(regularFolder, "README.md"), []byte(readmeContent), 0644); err != nil {
		return fmt.Errorf("failed to create README in regular folder: %w", err)
	}

	fmt.Println("  Icon test data created successfully!")
	return nil
}
