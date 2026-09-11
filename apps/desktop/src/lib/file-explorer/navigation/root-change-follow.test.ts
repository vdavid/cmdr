/**
 * Where a pane, a tab, or a remembered path goes when an edit moves a connected
 * place's root or start folder.
 *
 * ❗ Matching is by whole components, the same rule the backend's
 * `RemoteRoot::to_remote_path` and `navigate.ts`'s `isUnderServerRoot` hold:
 * `/srv/data-1` is a sibling of `/srv/data`, never a folder inside it.
 */

import { describe, expect, it } from 'vitest'
import type { VolumeRootChanged } from '$lib/ipc/bindings'
import { pathAfterRootChange } from './root-change-follow'

const NASPI = 'sftp://david@192.168.1.111:22/share/ZFS18_DATA/naspi'

function change(over: Partial<VolumeRootChanged>): VolumeRootChanged {
  return {
    volumeId: 'sftp-192.168.1.111-22-david',
    oldRoot: `${NASPI}/tmp`,
    newRoot: NASPI,
    oldLanding: `${NASPI}/tmp`,
    newLanding: NASPI,
    ...over,
  }
}

describe('pathAfterRootChange: a widened root', () => {
  it('moves a pane standing on the old root to the new landing (the prod case)', () => {
    // David narrowed a server to `/tmp` by mistake, then widened it to `naspi`.
    // The pane standing on `/tmp` is where he was, and the new root is where he
    // asked to be.
    expect(pathAfterRootChange(`${NASPI}/tmp`, change({}))).toBe(NASPI)
  })

  it('leaves a pane deeper inside the new root where it is', () => {
    expect(pathAfterRootChange(`${NASPI}/tmp/photos/2024`, change({}))).toBe(`${NASPI}/tmp/photos/2024`)
  })

  it('moves a pane on the old root to the new start folder when the edit set one', () => {
    const withStart = change({ newLanding: `${NASPI}/media` })
    expect(pathAfterRootChange(`${NASPI}/tmp`, withStart)).toBe(`${NASPI}/media`)
  })
})

describe('pathAfterRootChange: a narrowed root', () => {
  const narrowed = change({
    oldRoot: 'sftp://ada@nas.local:22/srv',
    oldLanding: 'sftp://ada@nas.local:22/srv',
    newRoot: 'sftp://ada@nas.local:22/srv/data',
    newLanding: 'sftp://ada@nas.local:22/srv/data',
  })

  it('moves a pane outside the new root to the new landing', () => {
    expect(pathAfterRootChange('sftp://ada@nas.local:22/srv/other/x', narrowed)).toBe(
      'sftp://ada@nas.local:22/srv/data',
    )
  })

  it('moves a pane on the old root to the new landing', () => {
    expect(pathAfterRootChange('sftp://ada@nas.local:22/srv', narrowed)).toBe('sftp://ada@nas.local:22/srv/data')
  })

  it('leaves a pane already inside the new root where it is', () => {
    expect(pathAfterRootChange('sftp://ada@nas.local:22/srv/data/photos', narrowed)).toBe(
      'sftp://ada@nas.local:22/srv/data/photos',
    )
  })

  it('treats a sibling sharing the new root as a prefix as OUTSIDE it', () => {
    // ❗ A string-prefix test would keep this pane on `/srv/data-1`, a folder the
    // new root refuses, and every listing there would fail.
    expect(pathAfterRootChange('sftp://ada@nas.local:22/srv/data-1/x', narrowed)).toBe(
      'sftp://ada@nas.local:22/srv/data',
    )
  })
})

describe('pathAfterRootChange: a start-folder-only edit', () => {
  const root = 'sftp://ada@nas.local:22/srv/data'

  it('moves a pane on the root to the new start folder', () => {
    const edit = change({ oldRoot: root, newRoot: root, oldLanding: root, newLanding: `${root}/photos` })
    expect(pathAfterRootChange(root, edit)).toBe(`${root}/photos`)
  })

  it('moves a pane on the old start folder to the new one', () => {
    const edit = change({ oldRoot: root, newRoot: root, oldLanding: `${root}/docs`, newLanding: `${root}/photos` })
    expect(pathAfterRootChange(`${root}/docs`, edit)).toBe(`${root}/photos`)
  })

  it('leaves a pane somewhere else in the root alone', () => {
    const edit = change({ oldRoot: root, newRoot: root, oldLanding: root, newLanding: `${root}/photos` })
    expect(pathAfterRootChange(`${root}/music`, edit)).toBe(`${root}/music`)
  })

  it('leaves a pane below the old start folder alone, since the root still holds it', () => {
    const edit = change({ oldRoot: root, newRoot: root, oldLanding: `${root}/docs`, newLanding: `${root}/photos` })
    expect(pathAfterRootChange(`${root}/docs/2024`, edit)).toBe(`${root}/docs/2024`)
  })
})

describe('pathAfterRootChange: spellings and repeats', () => {
  it('reads a server root with or without its trailing slash as the same folder', () => {
    // Rust spells a `/` root `sftp://ada@nas.local:22/`; the frontend's own
    // builder spells the same root without the slash.
    const toServerRoot = change({
      oldRoot: 'sftp://ada@nas.local:22/srv',
      oldLanding: 'sftp://ada@nas.local:22/srv',
      newRoot: 'sftp://ada@nas.local:22/',
      newLanding: 'sftp://ada@nas.local:22/',
    })
    expect(pathAfterRootChange('sftp://ada@nas.local:22/home/ada', toServerRoot)).toBe(
      'sftp://ada@nas.local:22/home/ada',
    )
    expect(pathAfterRootChange('sftp://ada@nas.local:22', toServerRoot)).toBe('sftp://ada@nas.local:22')
    expect(pathAfterRootChange('sftp://ada@nas.local:22/srv/', toServerRoot)).toBe('sftp://ada@nas.local:22/')
  })

  it('gives the same answer when applied twice, so two windows following one edit agree', () => {
    const edits = [
      change({}),
      change({ newLanding: `${NASPI}/media` }),
      change({
        oldRoot: 'sftp://ada@nas.local:22/srv',
        oldLanding: 'sftp://ada@nas.local:22/srv/docs',
        newRoot: 'sftp://ada@nas.local:22/srv/data',
        newLanding: 'sftp://ada@nas.local:22/srv/data/photos',
      }),
    ]
    const paths = [`${NASPI}/tmp`, `${NASPI}/tmp/x`, 'sftp://ada@nas.local:22/srv/docs', 'sftp://ada@nas.local:22/srv']
    for (const edit of edits) {
      for (const path of paths) {
        const once = pathAfterRootChange(path, edit)
        expect(pathAfterRootChange(once, edit)).toBe(once)
      }
    }
  })
})
