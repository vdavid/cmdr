package stacklease

import (
	"sort"
	"strings"
)

// smbNotifydReset clears the change-notify watches a killed client leaked into a
// Samba container, by stopping `notifyd` so smbd starts a fresh one. Prints what
// it cleared, or nothing on a clean container.
//
// Why: when a client's smbd child dies without closing its watches (a nextest
// timeout, a cancelled lane), Samba 4.23.8's `notifyd_send_delete` addresses the
// cleanup to the dead process instead of itself, so the watch stays forever and
// every later write on the share logs three failed sends per leaked watch. ~150
// leaks (a few killed runs) slows delivery to the live watchers by seconds.
//
// Why `notifyd` alone, rather than the container: smbd respawns it at once
// ("remove_child_pid: Restarting notifyd"), port 445 never closes, and live
// clients re-register their watches with the new one, so there's no health
// gap to wait out. With 150 leaked watches one write logged 450 failed sends;
// after the reset, none. A no-op exec costs ~0.1 s, a reset ~1.2 s. (Verified on
// Samba 4.23.8 in `consumer-smb-consumer-guest`, killing the smbd child behind
// `smbclient -c 'notify docs'`, 2026-10-07. Samba 4.20.6 drops such watches
// itself, so there the reset finds nothing.)
//
// ❗ `pgrep -x` matches the process name. `-f` would also match this script's
// own `sh -c` command line, and the `kill` would end the script itself.
const smbNotifydReset = `n=$(smbstatus --notify 2>/dev/null | grep -c '^/')
[ "$n" -gt 0 ] || exit 0
old=$(pgrep -x smbd-notifyd)
kill $old || exit 1
i=0
while :; do
  new=$(pgrep -x smbd-notifyd)
  [ -n "$new" ] && [ "$new" != "$old" ] && break
  i=$((i+1))
  [ $i -ge 100 ] && { echo "notifyd didn't come back after 5 s" >&2; exit 1; }
  sleep 0.05
done
echo "restarted notifyd, clearing $n leaked change-notify watch(es)"`

// runSoloResets runs the stack's solo resets on the requested services. The
// caller holds the lock and has seen no other holder, so nothing else is using
// the containers: the one moment a reset can't pull state from under a live
// suite. Best-effort: a failed reset costs speed, never the run, so it warns and
// moves on.
func (s *Stack) runSoloResets(c Composer, requested []string) {
	if len(s.soloResets) == 0 {
		return
	}
	wanted := make(map[string]bool, len(requested))
	for _, svc := range requested {
		wanted[svc] = true
	}
	services := make([]string, 0, len(s.soloResets))
	for svc := range s.soloResets {
		if wanted[svc] {
			services = append(services, svc)
		}
	}
	sort.Strings(services)
	for _, svc := range services {
		out, err := c.Exec(svc, s.soloResets[svc])
		if err != nil {
			Logf("WARN: %s solo reset in %s didn't finish (%v); the run goes on with whatever state it left", s.Name, svc, err)
			continue
		}
		if msg := strings.TrimSpace(out); msg != "" {
			InfoLogf("%s %s: %s", s.Name, svc, msg)
		}
	}
}
