/**
 * `navigate(intent, deps)` headless seam tests: refusal strings (L12).
 *
 * Every refusal kind → its exact `message`, byte-for-byte: each fires from the
 * in-place arm, where a `{ location }` whose volumeId equals the pane's
 * current one (same-volume) routes and the refusals live.
 *
 * Exercises `navigate()` DIRECTLY against injected fakes (no mount needed);
 * harness in `navigate.test-fixtures.ts`.
 */
import { describe, it, expect, beforeEach } from 'vitest'
import { navigate } from './navigate'
import { makeHarness, type Harness } from './navigate.test-fixtures'

let h: Harness
beforeEach(() => {
  h = makeHarness()
})

describe('refusal strings (L12) — byte-for-byte contract', () => {
  it('network-volume pane returns the exact select_volume refusal string', () => {
    h = makeHarness({ left: { path: 'smb://', volumeId: 'network' } })
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'network', path: '/Users/me/doc' } }, source: 'mcp' },
      h.deps,
    )
    expect(result).toEqual({
      status: 'refused',
      reason: {
        kind: 'on-network-volume',
        message: 'Pane is on the Network volume. Use select_volume to switch to a local volume first.',
      },
    })
  })

  it('an smb:// destination is refused with the same string from a LOCAL pane', () => {
    // Pre-fix this landed on the switch arm and reported success while the pane
    // showed the host list: `resolve_location` maps every `smb://` path to the
    // virtual network volume, whose only navigable path is the `smb://` sentinel.
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'network', path: 'smb://naspolya/media' } }, source: 'mcp' },
      h.deps,
    )
    expect(result).toEqual({
      status: 'refused',
      reason: {
        kind: 'smb-path-unsupported',
        message:
          "nav_to_path doesn't take smb:// paths. A mounted share is its own volume, so use select_volume with the name from cmdr://state volumes; for a share that isn't mounted, use select_volume Network and open the host.",
      },
    })
    expect(h.tab('left').volumeId).toBe('root')
  })

  it('an smb:// destination is refused the same way from a NETWORK pane', () => {
    h = makeHarness({ left: { path: 'smb://', volumeId: 'network' } })
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'network', path: 'smb://naspolya/media' } }, source: 'mcp' },
      h.deps,
    )
    expect(result.status).toBe('refused')
    if (result.status === 'refused') expect(result.reason.kind).toBe('smb-path-unsupported')
  })

  it('the smb:// sentinel itself still navigates (the Network volume root)', () => {
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'network', path: 'smb://' } }, source: 'mcp' },
      h.deps,
    )
    expect(result.status).toBe('started')
    expect(h.tab('left').volumeId).toBe('network')
  })

  it('MTP path mismatch returns the exact "not on this MTP volume" string (note the em dash)', () => {
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'root', path: 'mtp://otherdev/2/DCIM' } }, source: 'mcp' },
      h.deps,
    )
    expect(result).toEqual({
      status: 'refused',
      reason: { kind: 'mtp-unconnected', message: 'Pane is not on this MTP volume — call select_volume first.' },
    })
  })

  it('ADB path on a pane not on that device is refused (typed kind, no volume-id derivation)', () => {
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'root', path: 'adb://R58M12345/sdcard' } }, source: 'mcp' },
      h.deps,
    )
    expect(result).toEqual({
      status: 'refused',
      reason: { kind: 'adb-unconnected', message: 'Pane is not on this ADB volume. Call select_volume first.' },
    })
  })

  it('server path on a pane not on that place is refused', () => {
    const result = navigate(
      {
        pane: 'left',
        to: { goTo: { volumeId: 'root', path: 'sftp://ada@nas.local:22/srv/data/photos' } },
        source: 'mcp',
      },
      h.deps,
    )
    expect(result).toEqual({
      status: 'refused',
      reason: { kind: 'server-unconnected', message: 'Pane is not on this server volume. Call select_volume first.' },
    })
  })

  it('server path on a pane on a DIFFERENT server is refused: two servers can both have /srv/data', () => {
    h = makeHarness({ left: { path: 'sftp://ada@nas.local:22/srv/data', volumeId: 'sftp-nas-local-22-ada' } })
    const result = navigate(
      {
        pane: 'left',
        to: { goTo: { volumeId: 'sftp-nas-local-22-ada', path: 'sftp://ada@other.local:22/srv/data/photos' } },
        source: 'mcp',
      },
      h.deps,
    )
    expect(result.status).toBe('refused')
  })

  it("a path under the pane's own server root is accepted", () => {
    h = makeHarness({ left: { path: 'sftp://ada@nas.local:22/srv/data', volumeId: 'sftp-nas-local-22-ada' } })
    const result = navigate(
      {
        pane: 'left',
        to: { goTo: { volumeId: 'sftp-nas-local-22-ada', path: 'sftp://ada@nas.local:22/srv/data/photos' } },
        source: 'mcp',
      },
      h.deps,
    )
    expect(result.status).toBe('started')
  })

  it('a sibling of the server root is refused, not silently anchored onto it', () => {
    // ❗ `/srv/data-1` is a legal name. A string-prefix test would accept it and
    // ask the server for a directory the volume does not contain.
    h = makeHarness({ left: { path: 'sftp://ada@nas.local:22/srv/data', volumeId: 'sftp-nas-local-22-ada' } })
    const result = navigate(
      {
        pane: 'left',
        to: { goTo: { volumeId: 'sftp-nas-local-22-ada', path: 'sftp://ada@nas.local:22/srv/data-1/photos' } },
        source: 'mcp',
      },
      h.deps,
    )
    expect(result.status).toBe('refused')
  })

  it('on-server-volume pane refuses a local path with the exact "on the … server volume" string', () => {
    h = makeHarness({ left: { path: 'sftp://ada@nas.local:22/srv/data', volumeId: 'sftp-nas-local-22-ada' } })
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'sftp-nas-local-22-ada', path: '/Users/me/doc' } }, source: 'mcp' },
      h.deps,
    )
    expect(result).toEqual({
      status: 'refused',
      reason: {
        kind: 'server-unconnected',
        message: 'Pane is on the Naspolya server volume. Use select_volume to switch to a local volume first.',
      },
    })
  })

  it('on-MTP-volume pane returns the exact "on the … MTP volume" string (volumeName falls back to id)', () => {
    h = makeHarness({ left: { path: 'mtp://dev/1/DCIM', volumeId: 'mtp-dev:1' } })
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'mtp-dev:1', path: '/Users/me/doc' } }, source: 'mcp' },
      h.deps,
    )
    expect(result).toEqual({
      status: 'refused',
      reason: {
        kind: 'mtp-unconnected',
        message: 'Pane is on the mtp-dev:1 MTP volume. Use select_volume to switch to a local volume first.',
      },
    })
  })

  it('pane-unavailable returns the exact "Pane not available" string', () => {
    h = makeHarness({ suppressRef: ['left'] })
    const result = navigate(
      { pane: 'left', to: { goTo: { volumeId: 'root', path: '/Users/me/doc' } }, source: 'mcp' },
      h.deps,
    )
    expect(result).toEqual({ status: 'refused', reason: { kind: 'pane-unavailable', message: 'Pane not available' } })
  })
})
