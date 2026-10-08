package main

import (
	"bytes"
	"encoding/base64"
	"errors"
	"fmt"
	"os"
	"slices"
	"strings"
	"testing"
)

// The fixture is a throwaway key made with `tauri signer generate`, and a signature made with
// `tauri signer sign`, so these tests pin the verifier to the real signer's output format.
func readFixture(t *testing.T, name string) string {
	t.Helper()
	b, err := os.ReadFile("testdata/" + name)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

// cmdrPubkey is the app's real updater key (`tauri.conf.json`), which didn't sign the fixture.
const cmdrPubkey = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEEyNjAxRjM2QkIxNjhDMEEKUldRS2pCYTdOaDlnb2lwTE5wZUNVZE5FTTBScXRrTWJPb1EvN3J2Q0FVcC9JNzJkSmphUTl3V3MK"

func TestVerifyAcceptsTheSignersOwnOutput(t *testing.T) {
	err := verifyTauriSignature(readFixture(t, "fixture.pub"), readFixture(t, "payload.txt.sig"),
		strings.NewReader(readFixture(t, "payload.txt")))
	if err != nil {
		t.Fatalf("want a valid signature, got %v", err)
	}
}

func TestVerifyRefusesAChangedFile(t *testing.T) {
	err := verifyTauriSignature(readFixture(t, "fixture.pub"), readFixture(t, "payload.txt.sig"),
		strings.NewReader(readFixture(t, "payload.txt")+"x"))
	if !errors.Is(err, errBadSignature) {
		t.Fatalf("want errBadSignature, got %v", err)
	}
}

func TestVerifyRefusesAnotherKeysSignature(t *testing.T) {
	err := verifyTauriSignature(cmdrPubkey, readFixture(t, "payload.txt.sig"),
		strings.NewReader(readFixture(t, "payload.txt")))
	if !errors.Is(err, errKeyMismatch) {
		t.Fatalf("want errKeyMismatch, got %v", err)
	}
}

func TestVerifyRefusesAnEditedTrustedComment(t *testing.T) {
	text, err := base64.StdEncoding.DecodeString(readFixture(t, "payload.txt.sig"))
	if err != nil {
		t.Fatal(err)
	}
	edited := bytes.Replace(text, []byte("file:payload.txt"), []byte("file:other.txt"), 1)
	if bytes.Equal(edited, text) {
		t.Fatal("fixture has no file: comment to edit")
	}
	err = verifyTauriSignature(readFixture(t, "fixture.pub"), base64.StdEncoding.EncodeToString(edited),
		strings.NewReader(readFixture(t, "payload.txt")))
	if !errors.Is(err, errBadTrustedComments) {
		t.Fatalf("want errBadTrustedComments, got %v", err)
	}
}

func TestVerifyRefusesGarbage(t *testing.T) {
	for name, sig := range map[string]string{
		"not base64":   "%%%",
		"wrong shape":  base64.StdEncoding.EncodeToString([]byte("one line only\n")),
		"empty string": "",
	} {
		t.Run(name, func(t *testing.T) {
			err := verifyTauriSignature(readFixture(t, "fixture.pub"), sig, strings.NewReader("x"))
			if !errors.Is(err, errMalformed) {
				t.Fatalf("want errMalformed, got %v", err)
			}
		})
	}
}

func TestCmdrPubkeyParses(t *testing.T) {
	pk, err := parsePublicKey(cmdrPubkey)
	if err != nil {
		t.Fatal(err)
	}
	// minisign prints key IDs little-endian: the comment's A2601F36BB168C0A.
	id := pk.keyID
	slices.Reverse(id[:])
	if got := fmt.Sprintf("%X", id); got != "A2601F36BB168C0A" {
		t.Fatalf("key ID %s, want A2601F36BB168C0A", got)
	}
}
