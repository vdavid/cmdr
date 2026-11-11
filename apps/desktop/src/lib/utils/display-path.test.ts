import { describe, expect, it, onTestFinished } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { copiedPath, displayPath, withoutPicks } from './display-path'

/** A twin's row path, the way the MTP backend lists it (`cmdr_fs::twin_path`). */
function twinRowPath(folder: string, handle: number, name: string): string {
  return `${folder}/\u0000${String(handle)}/${name}`
}

describe('displayPath', () => {
  it('takes a twin’s pick out of its row path', () => {
    expect(displayPath(twinRowPath('/DCIM', 4242, 'IMG_1.JPG'))).toBe('/DCIM/IMG_1.JPG')
    expect(displayPath(`${twinRowPath('/DCIM', 7, 'Trip')}/a.jpg`)).toBe('/DCIM/Trip/a.jpg')
    expect(displayPath('/DCIM/\u00009')).toBe('/DCIM')
    expect(displayPath('\u00003/IMG_1.JPG')).toBe('IMG_1.JPG')
  })

  it('reads text holding twin paths with the plain paths', () => {
    const text = `“${twinRowPath('/DCIM', 1, 'a.jpg')}” and “${twinRowPath('/DCIM', 2, 'a.jpg')}”`
    expect(displayPath(text)).toBe('“/DCIM/a.jpg” and “/DCIM/a.jpg”')
  })

  it('leaves a NUL that is no pick alone', () => {
    expect(displayPath('modified \u0000')).toBe('modified \u0000')
    expect(displayPath('/x/\u000012ab/y')).toBe('/x/\u000012ab/y')
  })

  it('leaves an ordinary path alone', () => {
    expect(displayPath('/Users/me/notes.txt')).toBe('/Users/me/notes.txt')
  })

  // #706: the key id names the account; a person reads the place by host and bucket.
  it('names an S3 place by host and bucket, never by the access key id', () => {
    expect(displayPath('s3://AKIAEXAMPLEKEY@s3.eu-west-1.amazonaws.com:443/photos/a.jpg')).toBe(
      's3://s3.eu-west-1.amazonaws.com:443/photos/a.jpg',
    )
    expect(displayPath('Couldn’t copy “s3://GK00c0de@127.0.0.1:14480/b/x” to s3://AKIA2@h:443/c/')).toBe(
      'Couldn’t copy “s3://127.0.0.1:14480/b/x” to s3://h:443/c/',
    )
  })

  it('leaves an S3 address without an account, and other schemes’ accounts, alone', () => {
    for (const text of ['s3://bucket/key@2x.png', 'sftp://ada@nas:22/x', 'xs3://key@host:1/b']) {
      expect(displayPath(text)).toBe(text)
    }
  })

  // The word-start test is a callback, not a lookbehind (Safari 15 can't compile one), so pin its edges.
  it('takes the key id out only where `s3://` starts a word', () => {
    for (const text of ['ás3://k@h:1/b', '9s3://k@h:1/b', '+s3://k@h:1/b', '.s3://k@h:1/b', '-s3://k@h:1/b']) {
      expect(displayPath(text)).toBe(text)
    }
    // A letter outside the BMP is two UTF-16 units, and still a letter.
    expect(displayPath('\u{1D400}s3://k@h:1/b')).toBe('\u{1D400}s3://k@h:1/b')
    expect(displayPath('a\uDC00s3://k@h:1/b')).toBe('a\uDC00s3://h:1/b')
    expect(displayPath('(s3://k@h:1/b) /s3://k@h:1/c')).toBe('(s3://h:1/b) /s3://h:1/c')
  })
})

describe('withoutPicks', () => {
  it('keeps an S3 path whole, since a copied path has to find its account again', () => {
    const path = 's3://AKIAEXAMPLEKEY@h:443/photos/\u00004/a.jpg'
    expect(withoutPicks(path)).toBe('s3://AKIAEXAMPLEKEY@h:443/photos/a.jpg')
  })
})

describe('a local path on each platform', () => {
  it('reads with backslashes on Windows, a twin pick still taken out', () => {
    onTestFinished(withPlatform('windows'))
    expect(displayPath('C:/Users/ada/notes.txt')).toBe('C:\\Users\\ada\\notes.txt')
    expect(displayPath('//server/share/x')).toBe('\\\\server\\share\\x')
    expect(displayPath(twinRowPath('C:/DCIM', 4, 'a.jpg'))).toBe('C:\\DCIM\\a.jpg')
  })

  it('leaves prose holding a path, and every scheme URL, alone on Windows', () => {
    onTestFinished(withPlatform('windows'))
    expect(displayPath('Copy C:/a to D:/b')).toBe('Copy C:/a to D:/b')
    expect(displayPath('sftp://ada@nas.local:22/home/ada')).toBe('sftp://ada@nas.local:22/home/ada')
  })

  it.each(['macos', 'linux'] as const)('leaves every path alone on %s', (platform) => {
    onTestFinished(withPlatform(platform))
    expect(displayPath('/Users/ada/a\\b')).toBe('/Users/ada/a\\b')
    expect(displayPath('C:/Users/ada')).toBe('C:/Users/ada')
    expect(copiedPath('/Users/ada/notes.txt')).toBe('/Users/ada/notes.txt')
  })

  it('copies the shown form of a path on Windows, an S3 account kept', () => {
    onTestFinished(withPlatform('windows'))
    expect(copiedPath('C:/Users/ada/notes.txt')).toBe('C:\\Users\\ada\\notes.txt')
    expect(copiedPath('s3://AKIA@host:9000/bucket/a')).toBe('s3://AKIA@host:9000/bucket/a')
  })
})
