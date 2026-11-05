package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"time"
)

// uploadTries bounds each upload and each read-back, like the release's GitHub uploads (whose
// asset API failed often enough to cost a release two runs).
const uploadTries = 4

func defaultPause(try int) { time.Sleep(time.Duration(try*5) * time.Second) }

// bucket writes objects into an R2 bucket in the `eu` jurisdiction through Cloudflare's REST
// API (the endpoint `wrangler r2 object put` uses), so the release job runs no third-party code
// beside its credentials.
type bucket struct {
	api     string // `https://api.cloudflare.com/client/v4`, or a test server
	account string
	name    string
	token   string
	http    *http.Client
	pause   func(try int)
}

// put uploads the file at `path` as `key`, overwriting, with its content type and the
// immutable cache header. Retries a failed attempt.
func (b bucket) put(key, path string) error {
	var last error
	for try := 1; try <= uploadTries; try++ {
		if last = b.putOnce(key, path); last == nil {
			return nil
		}
		if try < uploadTries {
			b.wait(try)
		}
	}
	return fmt.Errorf("couldn't upload %s after %d tries: %w", key, uploadTries, last)
}

func (b bucket) wait(try int) {
	if b.pause != nil {
		b.pause(try)
		return
	}
	defaultPause(try)
}

func (b bucket) putOnce(key, path string) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()
	info, err := f.Stat()
	if err != nil {
		return err
	}
	url := fmt.Sprintf("%s/accounts/%s/r2/buckets/%s/objects/%s", b.api, b.account, b.name, key)
	req, err := http.NewRequest(http.MethodPut, url, f)
	if err != nil {
		return err
	}
	req.ContentLength = info.Size()
	req.Header.Set("Authorization", "Bearer "+b.token)
	req.Header.Set("cf-r2-jurisdiction", "eu")
	req.Header.Set("Content-Type", contentType(key))
	req.Header.Set("Cache-Control", immutableCache)
	resp, err := b.http.Do(req)
	if err != nil || resp == nil {
		return fmt.Errorf("no answer: %v", err)
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 64<<10))
	if resp.StatusCode/100 != 2 {
		return fmt.Errorf("HTTP %d: %s", resp.StatusCode, strings.TrimSpace(string(body)))
	}
	var answer struct {
		Success bool `json:"success"`
		Errors  []struct {
			Message string `json:"message"`
		} `json:"errors"`
	}
	if err := json.Unmarshal(body, &answer); err != nil {
		return fmt.Errorf("the API's answer isn't JSON: %w", err)
	}
	if !answer.Success {
		return fmt.Errorf("the API refused the upload: %+v", answer.Errors)
	}
	return nil
}

// verifyPublic fetches `url` the way a user would and requires its SHA-256 to be `wantHex`:
// proof that the public host serves exactly the bytes on the GitHub release. Retries, since a
// fresh custom domain or object can take a moment to answer.
func verifyPublic(client *http.Client, url, wantHex string, pause func(int)) error {
	var last error
	for try := 1; try <= uploadTries; try++ {
		if last = verifyOnce(client, url, wantHex); last == nil {
			return nil
		}
		if try < uploadTries {
			pause(try)
		}
	}
	return fmt.Errorf("%s doesn't serve the release's bytes: %w", url, last)
}

func verifyOnce(client *http.Client, url, wantHex string) error {
	resp, err := client.Get(url)
	if err != nil || resp == nil {
		return fmt.Errorf("no answer: %v", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("HTTP %d", resp.StatusCode)
	}
	h := sha256.New()
	if _, err := io.Copy(h, resp.Body); err != nil {
		return err
	}
	if got := hex.EncodeToString(h.Sum(nil)); got != wantHex {
		return fmt.Errorf("SHA-256 %s, want %s", got, wantHex)
	}
	return nil
}

// fileSHA256 is the hex SHA-256 of a local file.
func fileSHA256(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}
