package main

import (
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

func mirror(cfg config, b bucket) error {
	tag := "v" + cfg.version
	names, digests, err := releaseAssets(tag, cfg.repo)
	if err != nil {
		return err
	}
	files, err := mirroredAssets(names, cfg.version)
	if err != nil {
		return err
	}

	dir := cfg.dir
	if dir == "" {
		if dir, err = os.MkdirTemp("", "release-mirror-"); err != nil {
			return err
		}
		defer os.RemoveAll(dir)
	}
	hashes, err := downloadAndHash(tag, cfg.repo, dir, files, digests)
	if err != nil {
		return err
	}

	for _, name := range files {
		if err := b.put(objectKey(cfg.version, name), filepath.Join(dir, name)); err != nil {
			return err
		}
		fmt.Printf("Uploaded %s\n", objectKey(cfg.version, name))
	}
	client := &http.Client{Timeout: 15 * time.Minute}
	for _, name := range files {
		url := publicURL(cfg.host, cfg.version, name)
		if err := verifyPublic(client, url, hashes[name], defaultPause); err != nil {
			return err
		}
		fmt.Printf("Verified %s\n", url)
	}

	if cfg.manifest != "" {
		body, err := os.ReadFile(cfg.manifest)
		if err != nil {
			return err
		}
		if err := checkManifest(body, cfg.version, cfg.host); err != nil {
			return err
		}
		fmt.Printf("%s names only mirrored archives.\n", cfg.manifest)
	}
	fmt.Printf("Mirrored %d files of %s to %s.\n", len(files), tag, cfg.host)
	return nil
}

// releaseAssets lists the release's asset names and GitHub's SHA-256 digest of each. `gh release
// view` finds a draft by tag too.
func releaseAssets(tag, repo string) ([]string, map[string]string, error) {
	out, err := exec.Command("gh", "release", "view", tag, "-R", repo, "--json", "assets").Output()
	if err != nil {
		return nil, nil, fmt.Errorf("couldn't read release %s on %s: %w", tag, repo, err)
	}
	var release struct {
		Assets []ghAsset `json:"assets"`
	}
	if err := json.Unmarshal(out, &release); err != nil {
		return nil, nil, fmt.Errorf("gh's answer for %s isn't JSON: %w", tag, err)
	}
	digests := map[string]string{}
	var names []string
	for _, a := range release.Assets {
		names = append(names, a.Name)
		digests[a.Name] = strings.TrimPrefix(a.Digest, "sha256:")
	}
	return names, digests, nil
}

// downloadAndHash downloads `files` into `dir` and answers each one's SHA-256, once
// `checkHashes` agreed they're the release's bytes.
func downloadAndHash(tag, repo, dir string, files []string, digests map[string]string) (map[string]string, error) {
	args := []string{"release", "download", tag, "-R", repo, "-D", dir, "--clobber"}
	for _, name := range files {
		args = append(args, "-p", name)
	}
	if out, err := exec.Command("gh", args...).CombinedOutput(); err != nil {
		return nil, fmt.Errorf("couldn't download %s's files: %w\n%s", tag, err, out)
	}
	checksums, err := os.ReadFile(filepath.Join(dir, "checksums.txt"))
	if err != nil {
		return nil, err
	}
	listed, err := parseChecksums(string(checksums))
	if err != nil {
		return nil, err
	}
	hashes := map[string]string{}
	for _, name := range files {
		if hashes[name], err = fileSHA256(filepath.Join(dir, name)); err != nil {
			return nil, err
		}
	}
	return hashes, checkHashes(hashes, digests, listed)
}

// checkHashes requires every byte that goes up to be the release's: GitHub's own digest for each
// file (when GitHub reports one), and `checksums.txt` (what a user checks a download against) for
// every file it lists, each of which the mirror must carry.
func checkHashes(local, github, listed map[string]string) error {
	for name, sum := range local {
		if want := github[name]; want != "" && want != sum {
			return fmt.Errorf("%s downloaded with SHA-256 %s, but GitHub says %s", name, sum, want)
		}
		if want, ok := listed[name]; ok && want != sum {
			return fmt.Errorf("%s has SHA-256 %s, but checksums.txt says %s", name, sum, want)
		}
	}
	for name := range listed {
		if _, ok := local[name]; !ok {
			return fmt.Errorf("checksums.txt lists %s, which the mirror doesn't carry", name)
		}
	}
	return nil
}
