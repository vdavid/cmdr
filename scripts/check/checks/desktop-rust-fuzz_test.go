package checks

import (
	"bytes"
	"compress/gzip"
	"os"
	"path/filepath"
	"testing"
)

func TestUnpackLargeSeeds_WritesEachSeedDecompressed(t *testing.T) {
	fuzzDir := t.TempDir()
	var packed bytes.Buffer
	writer := gzip.NewWriter(&packed)
	if _, err := writer.Write([]byte("a large seed")); err != nil {
		t.Fatal(err)
	}
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}
	writeTestFile(t, fuzzDir, "large-seeds/t/big.bin.gz", packed.String())
	writeTestFile(t, fuzzDir, "target/large-seeds/t/stale.bin", "a seed someone deleted")

	dir, err := unpackLargeSeeds(fuzzDir, "t")
	if err != nil {
		t.Fatal(err)
	}

	if dir != filepath.Join("target", "large-seeds", "t") {
		t.Errorf("dir = %q", dir)
	}
	got, err := os.ReadFile(filepath.Join(fuzzDir, dir, "big.bin"))
	if err != nil || string(got) != "a large seed" {
		t.Errorf("got %q, %v", got, err)
	}
	if _, err := os.Stat(filepath.Join(fuzzDir, dir, "stale.bin")); !os.IsNotExist(err) {
		t.Errorf("a stale unpacked seed survived: %v", err)
	}
}

func TestUnpackLargeSeeds_NoneIsNoDir(t *testing.T) {
	dir, err := unpackLargeSeeds(t.TempDir(), "t")
	if err != nil || dir != "" {
		t.Errorf("got %q, %v", dir, err)
	}
}
