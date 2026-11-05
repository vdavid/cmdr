package main

import (
	"crypto/sha256"
	"encoding/hex"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

func writeFile(t *testing.T, name, body string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), name)
	if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestPutSendsTheObjectToTheEUBucketWithItsHeaders(t *testing.T) {
	var gotPath, gotAuth, gotJurisdiction, gotCache, gotType, gotBody string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPut {
			t.Errorf("method %s", r.Method)
		}
		gotPath = r.URL.EscapedPath()
		gotAuth = r.Header.Get("Authorization")
		gotJurisdiction = r.Header.Get("cf-r2-jurisdiction")
		gotCache = r.Header.Get("Cache-Control")
		gotType = r.Header.Get("Content-Type")
		b, _ := io.ReadAll(r.Body)
		gotBody = string(b)
		_, _ = io.WriteString(w, `{"success":true,"errors":[]}`)
	}))
	defer srv.Close()

	b := bucket{api: srv.URL, account: "acct", name: "cmdr-releases-eu", token: "tok", http: srv.Client()}
	path := writeFile(t, "Cmdr_0.60.0_x64.dmg", "dmg bytes")
	if err := b.put("v0.60.0/Cmdr_0.60.0_x64.dmg", path); err != nil {
		t.Fatal(err)
	}
	if gotPath != "/accounts/acct/r2/buckets/cmdr-releases-eu/objects/v0.60.0/Cmdr_0.60.0_x64.dmg" {
		t.Errorf("path %s", gotPath)
	}
	if gotAuth != "Bearer tok" || gotJurisdiction != "eu" || gotBody != "dmg bytes" {
		t.Errorf("auth %q, jurisdiction %q, body %q", gotAuth, gotJurisdiction, gotBody)
	}
	if gotCache != immutableCache || gotType != "application/x-apple-diskimage" {
		t.Errorf("cache %q, type %q", gotCache, gotType)
	}
}

func TestPutRetriesAFailedUploadAndThenGivesUp(t *testing.T) {
	var mu sync.Mutex
	calls := 0
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		calls++
		mu.Unlock()
		w.WriteHeader(http.StatusBadGateway)
	}))
	defer srv.Close()

	b := bucket{api: srv.URL, account: "acct", name: "b", token: "tok", http: srv.Client(), pause: noPause}
	err := b.put("v0.60.0/checksums.txt", writeFile(t, "checksums.txt", "x"))
	if err == nil || !strings.Contains(err.Error(), "502") {
		t.Fatalf("err = %v", err)
	}
	if calls != uploadTries {
		t.Errorf("%d calls, want %d", calls, uploadTries)
	}
}

func TestPutRefusesAnAPIAnswerThatSaysItFailed(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = io.WriteString(w, `{"success":false,"errors":[{"code":10000,"message":"Authentication error"}]}`)
	}))
	defer srv.Close()

	b := bucket{api: srv.URL, account: "acct", name: "b", token: "tok", http: srv.Client(), pause: noPause}
	err := b.put("k", writeFile(t, "f", "x"))
	if err == nil || !strings.Contains(err.Error(), "Authentication error") {
		t.Fatalf("err = %v", err)
	}
}

func sha(s string) string {
	sum := sha256.Sum256([]byte(s))
	return hex.EncodeToString(sum[:])
}

func TestVerifyPublicAcceptsTheSameBytesAndRefusesOthers(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/v0.60.0/good":
			_, _ = io.WriteString(w, "the bytes")
		case "/v0.60.0/stale":
			_, _ = io.WriteString(w, "other bytes")
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer srv.Close()

	if err := verifyPublic(srv.Client(), srv.URL+"/v0.60.0/good", sha("the bytes"), noPause); err != nil {
		t.Errorf("good: %v", err)
	}
	if err := verifyPublic(srv.Client(), srv.URL+"/v0.60.0/stale", sha("the bytes"), noPause); err == nil {
		t.Error("stale bytes were accepted")
	}
	if err := verifyPublic(srv.Client(), srv.URL+"/v0.60.0/missing", sha("the bytes"), noPause); err == nil || !strings.Contains(err.Error(), "404") {
		t.Errorf("missing: %v", err)
	}
}

func noPause(int) {}
