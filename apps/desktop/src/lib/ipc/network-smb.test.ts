/**
 * IPC contract tests for the network/SMB connection surface: `connect_to_server`,
 * `list_shares_on_host`, `mount_network_share`.
 *
 * MTP and SMB stood out in the coverage report as nearly-zero IPC coverage. The
 * underlying smb2 / mDNS logic is tested in its own crate; what this group catches
 * is the **wire format**, especially the many-positional-arg shapes that AGENTS.md
 * specifically calls out as fragile (the `mountNetworkShare(server, share, username,
 * password, port, timeoutMs)` signature has 6 positional args; getting the order
 * wrong silently breaks at runtime).
 */

import { afterEach, describe, expect, it } from 'vitest'

import { commands } from '$lib/ipc/bindings'
import type { ManualConnectResult, MountResult, ShareListResult } from '$lib/ipc/bindings'
import { clearIpcMocks, installIpcMock } from '$lib/ipc/test-helpers'

afterEach(() => {
  clearIpcMocks()
})

describe('commands.connectToServer', () => {
  it('forwards address as a single payload key', async () => {
    const ipc = installIpcMock()
    const result: ManualConnectResult = {
      host: {
        id: 'manual-1',
        name: 'storage.local',
        hostname: 'storage.local',
        ipAddress: '192.168.1.42',
        port: 445,
      },
      sharePath: null,
    }
    ipc.mock('connect_to_server', () => result)

    const out = await commands.connectToServer('smb://storage.local/share')

    expect(out).toEqual({ status: 'ok', data: result })
    expect(ipc.lastCall('connect_to_server')?.payload).toEqual({
      address: 'smb://storage.local/share',
    })
  })

  it('surfaces a string error on unreachable hosts', async () => {
    const ipc = installIpcMock()
    ipc.mock('connect_to_server', () => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- mockIPC requires throwing the raw wire shape to test the wire contract
      throw 'host unreachable'
    })

    const out = await commands.connectToServer('smb://nonexistent.invalid')

    expect(out.status).toBe('error')
    if (out.status === 'error') expect(out.error).toBe('host unreachable')
  })
})

describe('commands.listSharesOnHost', () => {
  it('sends all six positional args as camelCase payload keys', async () => {
    const ipc = installIpcMock()
    const result: ShareListResult = {
      shares: [{ name: 'Public', isDisk: true, comment: null }],
      authMode: 'guest_allowed',
      fromCache: false,
    }
    ipc.mock('list_shares_on_host', () => result)

    const hostId = 'host-1'
    const hostname = 'TEST_SERVER.local'
    const ipAddress: string | null = '192.168.1.42'
    const port = 4450
    const timeoutMs: number | null = 15000
    const cacheTtlMs: number | null = 30000

    await commands.listSharesOnHost(hostId, hostname, ipAddress, port, timeoutMs, cacheTtlMs)

    expect(ipc.lastCall('list_shares_on_host')?.payload).toEqual({
      hostId,
      hostname,
      ipAddress,
      port,
      timeoutMs,
      cacheTtlMs,
    })
  })

  it('surfaces the typed ShareListError discriminator (e.g. auth_required)', async () => {
    const ipc = installIpcMock()
    ipc.mock('list_shares_on_host', () => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- mockIPC requires throwing the raw typed-error shape to test the wire contract
      throw { type: 'auth_required', message: 'Server requires credentials' }
    })

    const out = await commands.listSharesOnHost('h', 'x.local', null, 445, null, null)

    expect(out.status).toBe('error')
    if (out.status === 'error') {
      expect(out.error).toEqual({ type: 'auth_required', message: 'Server requires credentials' })
    }
  })
})

describe('commands.mountNetworkShare', () => {
  it('sends the 6 positional args in declared order (server, share, username, password, port, timeoutMs)', async () => {
    const ipc = installIpcMock()
    const result: MountResult = { mountPath: '/Volumes/Public', alreadyMounted: false }
    ipc.mock('mount_network_share', () => result)

    const server = 'storage.local'
    const share = 'Public'
    const username: string | null = 'dave'
    const password: string | null = 'hunter2'
    const port: number | null = 445
    const timeoutMs: number | null = 20000

    await commands.mountNetworkShare(server, share, username, password, port, timeoutMs)

    expect(ipc.lastCall('mount_network_share')?.payload).toEqual({
      server,
      share,
      username,
      password,
      port,
      timeoutMs,
    })
  })

  it('surfaces typed MountError variants (auth_failed) on the error branch', async () => {
    const ipc = installIpcMock()
    ipc.mock('mount_network_share', () => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- mockIPC requires throwing the raw typed-error shape to test the wire contract
      throw { type: 'auth_failed', message: 'bad credentials' }
    })

    const out = await commands.mountNetworkShare('s', 'sh', 'u', 'p', null, null)

    expect(out.status).toBe('error')
    if (out.status === 'error') {
      expect(out.error).toEqual({ type: 'auth_failed', message: 'bad credentials' })
    }
  })
})
