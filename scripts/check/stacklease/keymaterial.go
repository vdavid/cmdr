package stacklease

import (
	"fmt"
	"strings"
	"time"
)

// keyMaterialDeadline bounds the wait for a container to publish its pair, and
// keyMaterialPoll is the probe interval inside that wait. Generous rather than
// tight: the deadline is never reached on the happy path, and the alternative to
// waiting is a suite that races the regeneration. A var so a test can shorten
// it (`withKeyMaterialDeadline`).
var keyMaterialDeadline = 90 * time.Second

const keyMaterialPoll = 100 * time.Millisecond

// healKeyMaterial republishes key material that went missing from the host while
// the containers kept running, and reports rather than returning to a caller
// whose key-auth cells would all fail.
//
// ❗ Restarting is the whole mechanism: the container's entrypoint regenerates
// the pair and rewrites its own `authorized_keys` from it, which is the only
// thing that can put the two halves back in agreement. `up -d` won't do it (it
// never touches a healthy container) and neither will anything on the host,
// which has no way to add a public key to a running sshd's account.
func (s *Stack) healKeyMaterial(c Composer, requested []string, broughtUp bool) error {
	if len(s.servicesMissingKeyMaterial(requested)) == 0 {
		return nil
	}
	// A stack this call just brought up is still writing. `up -d` returns when a
	// container reaches "running", which is well before its entrypoint has
	// generated anything, so reaching for a restart here would bounce a
	// perfectly healthy container that was seconds from publishing.
	if broughtUp {
		if err := s.waitForKeyMaterial(requested); err == nil {
			return nil
		}
	}
	gaps := s.servicesMissingKeyMaterial(requested)
	Logf("WARN: %s has no published key material for %s under %s; restarting so each entrypoint regenerates the pair its authorized_keys names",
		s.Name, strings.Join(gaps, ", "), s.KeysDir())
	if err := c.Restart(gaps); err != nil {
		return fmt.Errorf("restart %s service(s) with no key material (%s): %w", s.Name, strings.Join(gaps, ", "), err)
	}
	if err := s.waitForKeyMaterial(requested); err != nil {
		return err
	}
	InfoLogf("%s republished key material for %s", s.Name, strings.Join(gaps, ", "))
	return nil
}

// waitForKeyMaterial polls until every requested leaf holds its private key, so
// a suite never races a container that is still generating one.
func (s *Stack) waitForKeyMaterial(requested []string) error {
	deadline := time.Now().Add(keyMaterialDeadline)
	for {
		still := s.servicesMissingKeyMaterial(requested)
		if len(still) == 0 {
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("%s publishes no private key for %s after %s; every key-auth cell would fail an auth rung against a server whose authorized_keys names a key nothing can read. The usual cause is a container running an image older than the entrypoint that knows how to republish: stop.sh then start.sh rebuilds it",
				s.Name, strings.Join(still, ", "), keyMaterialDeadline)
		}
		time.Sleep(keyMaterialPoll)
	}
}
