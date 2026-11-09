// Command release-mirror copies a release's update archives, signatures, DMGs, pkg, and
// `checksums.txt` from its GitHub release into the EU R2 bucket behind `download.getcmdr.com`,
// then proves the public host serves the same bytes. The release pipeline's `publish` job runs it
// before the release goes Latest, so `latest.json` (which names the mirror) never points at a
// file that isn't there. GitHub Releases stays the public mirror and the home of the SBOMs and
// attestations. `docs/guides/releasing.md` § Where downloads are served from.
//
// Usage:
//
//	release-mirror [-repo owner/name] [-manifest latest.json] <version>
//	release-mirror -preflight
//
// Needs `CLOUDFLARE_API_TOKEN` (R2 write on the bucket) and `CLOUDFLARE_ACCOUNT_ID`, plus `gh`
// signed in for the release download. It only ever writes: it never deletes an object.
package main

import (
	"errors"
	"flag"
	"fmt"
	"net/http"
	"os"
	"strings"
	"time"
)

const (
	defaultHost   = "https://download.getcmdr.com"
	defaultBucket = "cmdr-releases-eu"
	cloudflareAPI = "https://api.cloudflare.com/client/v4"
	preflightKey  = "preflight/last-check.txt"
)

type config struct {
	version   string
	repo      string
	host      string
	bucket    string
	dir       string
	manifest  string
	preflight bool
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "release-mirror:", err)
		os.Exit(1)
	}
}

func run() error {
	var cfg config
	flag.StringVar(&cfg.repo, "repo", envOr("GITHUB_REPOSITORY", "getcmdr/cmdr"), "the GitHub repo the release is on")
	flag.StringVar(&cfg.host, "host", defaultHost, "the mirror's public host")
	flag.StringVar(&cfg.bucket, "bucket", defaultBucket, "the R2 bucket (jurisdiction eu)")
	flag.StringVar(&cfg.dir, "dir", "", "where to download the release's files (default: a temp dir)")
	flag.StringVar(&cfg.manifest, "manifest", "", "a latest.json to check names only mirrored archives")
	flag.BoolVar(&cfg.preflight, "preflight", false, "only prove the credentials can write the bucket")
	flag.Parse()

	b, err := bucketFromEnv(cfg.bucket)
	if err != nil {
		return err
	}
	if cfg.preflight {
		return preflight(b)
	}
	if flag.NArg() != 1 {
		return errors.New("usage: release-mirror [-repo owner/name] [-manifest latest.json] <version>")
	}
	cfg.version = strings.TrimPrefix(flag.Arg(0), "v")
	return mirror(cfg, b)
}

func envOr(name, fallback string) string {
	if v := os.Getenv(name); v != "" {
		return v
	}
	return fallback
}

func bucketFromEnv(name string) (bucket, error) {
	token, account := os.Getenv("CLOUDFLARE_API_TOKEN"), os.Getenv("CLOUDFLARE_ACCOUNT_ID")
	if token == "" || account == "" {
		return bucket{}, errors.New("CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID must both be set")
	}
	return bucket{
		api: cloudflareAPI, account: account, name: name, token: token,
		http: &http.Client{Timeout: 15 * time.Minute},
	}, nil
}

// preflight writes one small, fixed object, overwriting the last one, so a token that can't
// write the bucket fails the release's first job instead of its last.
func preflight(b bucket) error {
	f, err := os.CreateTemp("", "release-mirror-preflight-*.txt")
	if err != nil {
		return err
	}
	defer os.Remove(f.Name())
	fmt.Fprintf(f, "release pipeline preflight, %s\n", time.Now().UTC().Format(time.RFC3339))
	f.Close()
	if err := b.put(preflightKey, f.Name()); err != nil {
		return fmt.Errorf("the Cloudflare token can't write the %s bucket: %w", b.name, err)
	}
	fmt.Printf("The Cloudflare token can write the %s bucket.\n", b.name)
	return nil
}

type ghAsset struct {
	Name   string `json:"name"`
	Digest string `json:"digest"`
}
