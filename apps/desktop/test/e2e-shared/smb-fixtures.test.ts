/**
 * The E2E teardown unmounts only the fixture's own SMB mounts. ❗ `/Volumes/public`
 * alone says nothing about whose share sits there: a person's NAS share, or a dev
 * session's mount of another server, lands at the same path, and a blind `umount`
 * took it away.
 */

import { describe, it, expect } from 'vitest'
import { fixtureMountPoints, preMountPlan } from './smb-fixtures.js'

const mountOutput = [
  '/dev/disk3s1s1 on / (apfs, sealed, local, read-only, journaled)',
  '//test_user_3@192.168.50.10/testshare on /Volumes/testshare (smbfs, nodev, nosuid, mounted by test_user_3)',
  '//GUEST:@localhost:11480/public on /Volumes/public (smbfs, nodev, nosuid, noowners, mounted by test_user_3)',
  '//guest@localhost:10480/public on /Volumes/public-1 (smbfs, nodev, nosuid, mounted by test_user_3)',
  '//testuser@localhost:10481/private on /Volumes/my private (smbfs, nodev, nosuid, mounted by test_user_3)',
  '//testuser@localhost:11481/private on /Volumes/private (smbfs, nodev, nosuid, mounted by test_user_3)',
].join('\n')

const fixture = [
  { host: 'localhost', port: 10480, share: 'public' },
  { host: 'localhost', port: 10481, share: 'private' },
]

describe('fixtureMountPoints', () => {
  it('finds the fixture server’s mounts wherever they landed, and nothing else', () => {
    expect(fixtureMountPoints(mountOutput, fixture)).toEqual(['/Volumes/public-1', '/Volumes/my private'])
  })

  it('leaves a share of the same name on another port or machine alone', () => {
    expect(fixtureMountPoints(mountOutput, [{ host: 'localhost', port: 445, share: 'public' }])).toEqual([])
    expect(fixtureMountPoints(mountOutput, [{ host: '192.168.50.10', port: 445, share: 'testshare' }])).toEqual([
      '/Volumes/testshare',
    ])
  })

  it('matches the host and share in any case', () => {
    expect(fixtureMountPoints(mountOutput, [{ host: 'LOCALHOST', port: 10480, share: 'Public' }])).toEqual([
      '/Volumes/public-1',
    ])
  })
})

/**
 * ❗ "Already mounted" means the FIXTURE is mounted, found by source, ❌ never a path
 * that exists: E2E writes and deletes on that share, and a person's share sitting at
 * `/Volumes/public` must never be mistaken for it.
 */
describe('preMountPlan', () => {
  const guest = { host: 'localhost', port: 10480, share: 'public' }
  const foreignAtPublic =
    '//test_user_3@192.168.50.10/public on /Volumes/public (smbfs, nodev, nosuid, mounted by test_user_3)'

  it('reuses the fixture’s own mount, wherever it landed', () => {
    const out = `${foreignAtPublic}\n//guest@localhost:10480/public on /Volumes/public-1 (smbfs, nodev, nosuid)`
    expect(preMountPlan(out, guest, '/Volumes/public', true)).toEqual({ kind: 'reuse', path: '/Volumes/public-1' })
  })

  it('refuses a path held by a foreign share instead of treating it as the fixture', () => {
    expect(preMountPlan(foreignAtPublic, guest, '/Volumes/public', true)).toEqual({ kind: 'occupied' })
  })

  it('mounts at the default path when nothing holds it', () => {
    expect(preMountPlan('', guest, '/Volumes/public', false)).toEqual({ kind: 'mount', path: '/Volumes/public' })
  })
})
