package main

import (
	"bytes"
	"crypto/ed25519"
	"encoding/base64"
	"errors"
	"fmt"
	"io"
	"strings"

	"golang.org/x/crypto/blake2b"
)

// Minisign verification of a Tauri updater signature, done natively rather than by trusting the
// signer's exit code: the finish step proves every `.sig` against the public key the app ships
// with, on the exact bytes the release serves, before anything publishes. It's the check the
// app's updater runs (`apps/desktop/src-tauri/src/updater/signature.rs`), so a signature that
// passes here passes there.
//
// Both the key and the signature are Tauri's double encoding: base64 of minisign's text format.
//
// Public key text: an untrusted comment line, then base64 of `Ed` + 8-byte key ID + 32-byte key.
// Signature text, four lines: untrusted comment; base64 of algorithm (`ED` signs the BLAKE2b-512
// of the file, legacy `Ed` the file itself) + key ID + 64-byte signature; `trusted comment: …`;
// base64 of a 64-byte signature over the first signature followed by the trusted comment.

var (
	errMalformed          = errors.New("malformed minisign data")
	errKeyMismatch        = errors.New("signed by a different key")
	errBadSignature       = errors.New("signature doesn't match the file")
	errBadTrustedComments = errors.New("trusted comment signature doesn't match")
)

type publicKey struct {
	keyID [8]byte
	key   ed25519.PublicKey
}

type signature struct {
	prehashed      bool
	keyID          [8]byte
	sig            []byte
	trustedComment string
	globalSig      []byte
}

// unwrapTauri decodes Tauri's outer base64 into minisign's text lines.
func unwrapTauri(b64 string) ([]string, error) {
	text, err := base64.StdEncoding.DecodeString(strings.TrimSpace(b64))
	if err != nil {
		return nil, fmt.Errorf("%w: outer base64: %v", errMalformed, err)
	}
	return strings.Split(strings.TrimRight(string(text), "\n"), "\n"), nil
}

func parsePublicKey(b64 string) (publicKey, error) {
	lines, err := unwrapTauri(b64)
	if err != nil {
		return publicKey{}, err
	}
	if len(lines) != 2 {
		return publicKey{}, fmt.Errorf("%w: public key has %d lines, want 2", errMalformed, len(lines))
	}
	raw, err := base64.StdEncoding.DecodeString(lines[1])
	if err != nil || len(raw) != 2+8+ed25519.PublicKeySize || string(raw[:2]) != "Ed" {
		return publicKey{}, fmt.Errorf("%w: public key body", errMalformed)
	}
	var pk publicKey
	copy(pk.keyID[:], raw[2:10])
	pk.key = ed25519.PublicKey(raw[10:])
	return pk, nil
}

func parseSignature(b64 string) (signature, error) {
	lines, err := unwrapTauri(b64)
	if err != nil {
		return signature{}, err
	}
	if len(lines) != 4 {
		return signature{}, fmt.Errorf("%w: signature has %d lines, want 4", errMalformed, len(lines))
	}
	raw, err := base64.StdEncoding.DecodeString(lines[1])
	if err != nil || len(raw) != 2+8+ed25519.SignatureSize {
		return signature{}, fmt.Errorf("%w: signature body", errMalformed)
	}
	var s signature
	switch string(raw[:2]) {
	case "ED":
		s.prehashed = true
	case "Ed":
	default:
		return signature{}, fmt.Errorf("%w: unknown algorithm %q", errMalformed, raw[:2])
	}
	copy(s.keyID[:], raw[2:10])
	s.sig = raw[10:]
	comment, ok := strings.CutPrefix(lines[2], "trusted comment: ")
	if !ok {
		return signature{}, fmt.Errorf("%w: no trusted comment line", errMalformed)
	}
	s.trustedComment = comment
	s.globalSig, err = base64.StdEncoding.DecodeString(lines[3])
	if err != nil || len(s.globalSig) != ed25519.SignatureSize {
		return signature{}, fmt.Errorf("%w: trusted comment signature", errMalformed)
	}
	return s, nil
}

// verify checks `data` against `sig` the way minisign does, including the trusted comment.
func (pk publicKey) verify(sig signature, data io.Reader) error {
	if sig.keyID != pk.keyID {
		return fmt.Errorf("%w: key ID %X, want %X", errKeyMismatch, sig.keyID, pk.keyID)
	}
	var message []byte
	if sig.prehashed {
		h, err := blake2b.New512(nil)
		if err != nil {
			return err
		}
		if _, err := io.Copy(h, data); err != nil {
			return fmt.Errorf("reading the signed file: %w", err)
		}
		message = h.Sum(nil)
	} else {
		var buf bytes.Buffer
		if _, err := io.Copy(&buf, data); err != nil {
			return fmt.Errorf("reading the signed file: %w", err)
		}
		message = buf.Bytes()
	}
	if !ed25519.Verify(pk.key, message, sig.sig) {
		return errBadSignature
	}
	if !ed25519.Verify(pk.key, append(append([]byte{}, sig.sig...), sig.trustedComment...), sig.globalSig) {
		return errBadTrustedComments
	}
	return nil
}

// verifyTauriSignature parses both halves and verifies `data`.
func verifyTauriSignature(pubkeyB64, sigB64 string, data io.Reader) error {
	pk, err := parsePublicKey(pubkeyB64)
	if err != nil {
		return err
	}
	sig, err := parseSignature(sigB64)
	if err != nil {
		return err
	}
	return pk.verify(sig, data)
}
