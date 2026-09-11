/**
 * Where a picked volume opens: a saved server place on its start folder, and
 * everything else at its root, where the volume switch picks the path.
 */

import { describe, expect, it } from 'vitest'
import type { VolumeInfo } from '../types'
import { pathForPickedVolume } from './picked-volume-path'

const ROOT = 'sftp://ada@nas.local:22/srv/data'
const LANDING = 'sftp://ada@nas.local:22/srv/data/photos'

function serverPlace(over: Partial<VolumeInfo>): VolumeInfo {
  return {
    id: 'sftp-nas-local-22-ada',
    name: 'Naspolya',
    path: ROOT,
    category: 'network',
    fsType: 'sftp',
    isEjectable: false,
    landingPath: LANDING,
    ...over,
  }
}

describe('pathForPickedVolume', () => {
  it('opens a saved place on its start folder, since the connect is what lands it', () => {
    expect(pathForPickedVolume(serverPlace({ connectionState: 'saved' }))).toBe(LANDING)
  })

  it('opens a saved place with no start folder at its root', () => {
    expect(pathForPickedVolume(serverPlace({ connectionState: 'saved', landingPath: null }))).toBe(ROOT)
    expect(pathForPickedVolume(serverPlace({ connectionState: 'saved', landingPath: undefined }))).toBe(ROOT)
  })

  it('opens a connected place at its root, so the other pane and the remembered path still get their turn', () => {
    // `determineNavigationPath` treats any target other than the root as a
    // favorite and returns it outright, so handing it the landing here would skip
    // both. The landing is its last default instead.
    expect(pathForPickedVolume(serverPlace({ connectionState: 'direct' }))).toBe(ROOT)
    expect(pathForPickedVolume(serverPlace({ connectionState: 'disconnected' }))).toBe(ROOT)
  })

  it('opens a local disk at its root', () => {
    expect(
      pathForPickedVolume({ id: 'usb', name: 'USB', path: '/Volumes/USB', category: 'attached_volume', isEjectable: true }),
    ).toBe('/Volumes/USB')
  })
})
