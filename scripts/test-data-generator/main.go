// Test data generator for Cmdr.
// Creates folders with markdown files containing humorous random sentences.
// Run with: go run scripts/test-data-generator/main.go
package main

import (
	cryptoRand "crypto/rand"
	"fmt"
	"math/rand"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

// Target file counts for each folder
var targets = map[string]int{
	"folder with 1000 files":   1000,
	"folder with 5000 files":   5000,
	"folder with 20000 files":  20000,
	"folder with 50000 files":  50000,
	"folder with 100000 files": 100000,
	"folder with 200000 files": 200000,
}

// Word lists for sentence generation - picked for maximum entertainment value

var names = []string{
	"David", "Gertrude", "Chad", "Beatrice", "Wolfgang", "Thomas", "Bartholomew", "Helga",
	"Donald", "Mildred", "Cornelius", "Julia", "Archibald", "Edith", "Montgomery", "Gladys",
	"Willy", "Brunhilde", "Percival", "Agatha",
}

var verbsPast = []string{
	"devoured", "grated", "befriended", "interrogated", "serenaded",
	"catapulted", "photobombed", "ghosted", "rickrolled", "bamboozled",
}

var verbsPresent = []string{
	"eats", "greets", "befriends", "interrogates", "serenades",
	"catapults", "photobombs", "ghosts", "rickrolls", "bamboozles",
}

var verbsFuture = []string{
	"will devour", "will say goodbye to", "will befriend", "will interrogate", "will serenade",
	"will catapult", "will photobomb", "will ghost", "will rickroll", "will bamboozle",
}

var articles = []string{"a", "the"}

// Adverbs starting with consonant (to match "a")
var adverbs = []string{
	"suspiciously", "dramatically", "rather", "quite", "passionately",
	"massively", "mysteriously", "aggressively", "surprisingly", "sarcastically",
}

var positiveAdjectives = []string{
	"magnificent", "glorious", "spectacular", "fabulous", "majestic",
	"legendary", "pristine", "exquisite", "splendid", "divine",
	"radiant", "dazzling", "illustrious", "sublime", "phenomenal",
	"resplendent", "sumptuous", "transcendent", "nice", "wondrous",
}

var conjunctions = []string{"but", "and"}

var negativeAdjectives = []string{
	"cursed", "suspicious", "questionable", "haunted", "soggy",
	"expired", "possessed", "radioactive", "sentient", "vengeful",
	"chaotic", "forbidden", "unhinged", "ominous", "volatile",
	"malevolent", "treacherous", "diabolical", "nefarious", "apocalyptic",
}

var objects = []string{
	"banana", "kazoo", "rubber duck", "burrito", "accordion",
	"sock puppet", "disco ball", "potato", "chainsaw", "unicycle",
	"trombone", "waffle iron", "lawn flamingo", "fog machine", "cheese wheel",
	"bagpipe", "lava lamp", "taco", "hedge trimmer", "bowling ball",
	"theremin", "cactus", "sousaphone", "meatball", "submarine",
	"anvil", "pickle jar", "trampoline", "baguette", "jetpack",
	"saxophone", "watermelon", "catapult", "chandelier", "harmonica",
	"wheelbarrow", "croissant", "pogo stick", "xylophone", "spatula",
	"didgeridoo", "pretzel", "hovercraft", "gargoyle", "ukulele",
	"jackhammer", "pancake", "trebuchet", "gnome statue", "kazoo army",
}

// generateSentence creates a random humorous sentence.
// Structure: "{Name} {verb} {article} {adverb} {positive adj} {and/but} {adverb} {negative adj} {object}."
// Example: "Gertrude is yeeting a suspiciously magnificent but dramatically cursed rubber duck."
func generateSentence() string {
	// Pick random tense
	var verb string
	switch rand.Intn(3) {
	case 0:
		verb = verbsPast[rand.Intn(len(verbsPast))]
	case 1:
		verb = verbsPresent[rand.Intn(len(verbsPresent))]
	default:
		verb = verbsFuture[rand.Intn(len(verbsFuture))]
	}

	return fmt.Sprintf("%s %s %s %s %s %s %s %s %s.",
		names[rand.Intn(len(names))],
		verb,
		articles[rand.Intn(len(articles))],
		adverbs[rand.Intn(len(adverbs))],
		positiveAdjectives[rand.Intn(len(positiveAdjectives))],
		conjunctions[rand.Intn(len(conjunctions))],
		adverbs[rand.Intn(len(adverbs))],
		negativeAdjectives[rand.Intn(len(negativeAdjectives))],
		objects[rand.Intn(len(objects))],
	)
}

// generateTimestamp returns a random timestamp between 2030-01-01 and 2040-01-01.
func generateTimestamp() time.Time {
	start := time.Date(2030, 1, 1, 0, 0, 0, 0, time.UTC)
	end := time.Date(2040, 1, 1, 0, 0, 0, 0, time.UTC)
	delta := end.Sub(start)
	randomDuration := time.Duration(rand.Int63n(int64(delta)))
	return start.Add(randomDuration)
}

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

// syncFolder ensures a folder has exactly targetCount files, creating or deleting as needed.
func syncFolder(folderPath string, targetCount int) error {
	if err := os.MkdirAll(folderPath, 0755); err != nil {
		return fmt.Errorf("failed to create folder %s: %w", folderPath, err)
	}

	entries, err := os.ReadDir(folderPath)
	if err != nil {
		return fmt.Errorf("failed to read folder %s: %w", folderPath, err)
	}

	var existingFiles []string
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

// createIconTestData creates a folder with various file types for testing icons.
// Includes: fake files with different extensions, symlinks, and folders with custom icons.
func createIconTestData(baseDir string) error {
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
	if err := os.WriteFile(filepath.Join(regularFolder, "README.md"), []byte("# Regular folder\n\nThis folder has the default macOS folder icon.\n"), 0644); err != nil {
		return fmt.Errorf("failed to create README in regular folder: %w", err)
	}

	fmt.Println("  Icon test data created successfully!")
	return nil
}

// Big files configuration
type bigFilesScenario struct {
	name        string
	totalSize   int64 // in bytes
	fileCount   int   // approximate number of files
	dirCount    int   // number of directories (for hierarchical scenarios)
	description string
}

var bigFilesScenarios = []bigFilesScenario{
	{
		name:        "big-files-100-files-total-1GB",
		totalSize:   1 * 1024 * 1024 * 1024, // 1GB
		fileCount:   100,
		dirCount:    0,
		description: "100 files totaling 1GB (~10MB each)",
	},
	{
		name:        "big-files-one-file-5GB",
		totalSize:   5 * 1024 * 1024 * 1024, // 5GB
		fileCount:   1,
		dirCount:    0,
		description: "Single 5GB file",
	},
	{
		name:        "big-files-100k-files-and-dirs-total-2GB",
		totalSize:   2 * 1024 * 1024 * 1024, // 2GB
		fileCount:   100000,
		dirCount:    1000, // 1000 directories with ~100 files each
		description: "100k files in 1000 directories totaling 2GB",
	},
}

// createBigFile creates a file with random data of the specified size.
// Uses sparse file creation for efficiency where possible.
func createBigFile(path string, size int64) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer func(f *os.File) {
		_ = f.Close()
	}(f)

	// Write random data in chunks for better performance
	const chunkSize = 64 * 1024 * 1024 // 64MB chunks
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
func bigFilesFolderNeedsRecreation(folderPath string, scenario bigFilesScenario) bool {
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

	withinRange := currentSize >= scenario.totalSize*95/100 && currentSize <= scenario.totalSize*105/100
	if withinRange {
		fmt.Printf("    Already exists with ~%d files, %.2f GB - skipping\n",
			fileCount, float64(currentSize)/(1024*1024*1024))
		return false
	}
	return true
}

// createHierarchicalBigFiles creates files organized in directories.
func createHierarchicalBigFiles(folderPath string, scenario bigFilesScenario) error {
	filesPerDir := scenario.fileCount / scenario.dirCount
	fileSize := scenario.totalSize / int64(scenario.fileCount)

	fmt.Printf("    Creating %d directories with ~%d files each (~%d KB per file)...\n",
		scenario.dirCount, filesPerDir, fileSize/1024)

	fileIndex := 0
	for d := range scenario.dirCount {
		dirPath := filepath.Join(folderPath, fmt.Sprintf("dir-%05d", d))
		if err := os.MkdirAll(dirPath, 0755); err != nil {
			return fmt.Errorf("failed to create directory %d: %w", d, err)
		}

		for f := 0; f < filesPerDir && fileIndex < scenario.fileCount; f++ {
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
	fmt.Printf("    Created %d files in %d directories\n", fileIndex, scenario.dirCount)
	return nil
}

// createFlatBigFiles creates files in a single directory.
func createFlatBigFiles(folderPath string, scenario bigFilesScenario) error {
	fileSize := scenario.totalSize / int64(scenario.fileCount)
	fmt.Printf("    Creating %d files (~%.2f MB each)...\n",
		scenario.fileCount, float64(fileSize)/(1024*1024))

	for i := range scenario.fileCount {
		filePath := filepath.Join(folderPath, fmt.Sprintf("file-%06d.dat", i))
		if err := createBigFile(filePath, fileSize); err != nil {
			return fmt.Errorf("failed to create file %d: %w", i, err)
		}

		if (i+1)%10 == 0 || i == scenario.fileCount-1 {
			pct := float64(i+1) / float64(scenario.fileCount) * 100
			fmt.Printf("    Progress: %.0f%% (%d/%d files)\n", pct, i+1, scenario.fileCount)
		}
	}
	return nil
}

// syncBigFilesScenario ensures the big-files scenario folder is in the desired state.
func syncBigFilesScenario(baseDir string, scenario bigFilesScenario) error {
	folderPath := filepath.Join(baseDir, scenario.name)
	fmt.Printf("  %s: %s\n", scenario.name, scenario.description)

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
	if scenario.dirCount > 0 {
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

func main() {
	baseDir := "_ignored/test-data"
	bigFilesDir := "_ignored/test-data/big-files"
	fmt.Printf("Syncing test data folders in %s/\n\n", baseDir)

	// Create icon test data first
	if err := createIconTestData(baseDir); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error creating icon test data: %v\n", err)
		os.Exit(1)
	}
	fmt.Println()

	// Sync large file count folders
	for folderName, target := range targets {
		folderPath := filepath.Join(baseDir, folderName)
		if err := syncFolder(folderPath, target); err != nil {
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

	for _, scenario := range bigFilesScenarios {
		if err := syncBigFilesScenario(bigFilesDir, scenario); err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "Error creating big-files scenario %s: %v\n", scenario.name, err)
			os.Exit(1)
		}
	}
	fmt.Println()

	fmt.Println("All folders synced successfully!")
}
