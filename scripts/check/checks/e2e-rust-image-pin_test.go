package checks

import (
	"reflect"
	"testing"
)

const e2eRustToolchainFixture = "[toolchain]\n# A comment\nchannel = \"1.99.1\"\ncomponents = [\"rustfmt\"]\n"

func TestE2ERustVersionsReadsThePinAndEveryImageTag(t *testing.T) {
	dockerfile := "# rust:1.0.0-slim is only prose here\n" +
		"COPY --from=rust:1.99.1-slim-trixie@sha256:abc /usr/local/cargo /root/.cargo\n" +
		"COPY --from=rust:1.99.0-slim-trixie@sha256:abc /usr/local/rustup /root/.rustup\n" +
		"ENV RUSTUP_TOOLCHAIN=1.98.2\n"
	pinned, tags, err := e2eRustVersions(e2eRustToolchainFixture, dockerfile)
	if err != nil {
		t.Fatalf("e2eRustVersions: %v", err)
	}
	if pinned != "1.99.1" {
		t.Errorf("pinned = %q, want 1.99.1", pinned)
	}
	if want := []string{"1.99.1", "1.99.0", "1.98.2"}; !reflect.DeepEqual(tags, want) {
		t.Errorf("tags = %v, want %v", tags, want)
	}
}

func TestE2ERustVersionsFailsWithoutAnImage(t *testing.T) {
	if _, _, err := e2eRustVersions(e2eRustToolchainFixture, "FROM ubuntu:26.04\n"); err == nil {
		t.Fatal("expected an error when the Dockerfile names no rust image")
	}
}

func TestE2ERustVersionsFailsWithoutAChannel(t *testing.T) {
	if _, _, err := e2eRustVersions("[toolchain]\n", "COPY --from=rust:1.99.1-slim /a /b\n"); err == nil {
		t.Fatal("expected an error when rust-toolchain.toml pins no channel")
	}
}
