import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync, type ComponentProps } from 'svelte'
import TransferProgressReadout from './TransferProgressReadout.svelte'
import { seconds } from '$lib/units'

// The component reads reactive settings (file-size format) deep in `<Size>`. The
// real path needs the settings store; stub the format getter to keep the unit
// test isolated.
vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'decimal',
}))

let target: HTMLElement

function render(props: ComponentProps<typeof TransferProgressReadout>): void {
  target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferProgressReadout, { target, props })
  flushSync()
}

function texts(selector: string): string[] {
  return [...target.querySelectorAll(selector)].map((el) => el.textContent.replace(/\s+/g, ' ').trim())
}

const halfway = { bytesDone: 50, bytesTotal: 200, filesDone: 1, filesTotal: 4 }

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('TransferProgressReadout', () => {
  it('renders a bytes bar and a count bar, each at its own percentage', () => {
    render(halfway)
    const bars = [...target.querySelectorAll('[role="progressbar"]')]
    expect(bars.map((b) => b.getAttribute('aria-valuenow'))).toEqual(['25', '25'])
    expect(bars.map((b) => b.getAttribute('aria-label'))).toEqual(['Size progress', 'File progress'])
    // Bracketed, so it reads as a qualifier on the amount beside it.
    expect(texts('.percent')).toEqual(['(25%)', '(25%)'])
  })

  it('drops the bytes row when the total size is unknown, keeping the count row', () => {
    render({ ...halfway, bytesDone: 0, bytesTotal: 0 })
    const bars = [...target.querySelectorAll('[role="progressbar"]')]
    expect(bars.length).toBe(1)
    expect(bars[0].getAttribute('aria-label')).toBe('File progress')
    expect(texts('.percent')).toEqual(['(25%)'])
  })

  it('shows both amounts: bytes done/total and files done/total', () => {
    render(halfway)
    expect(texts('.amount')).toEqual(['50 bytes / 200 bytes', '1 / 4'])
  })

  it('rounds live sizes to whole units, so digits stop churning under the eye', () => {
    // 7.61 GB / 22.66 GB, at 11.96 MB/s. A size column elsewhere keeps its
    // decimals; a number that changes several times a second doesn't earn them.
    render({ ...halfway, bytesDone: 7_610_000_000, bytesTotal: 22_660_000_000, bytesPerSecond: 11_960_000 })
    expect(texts('.amount')[0]).toBe('8 GB / 23 GB')
    expect(texts('.rate')[0]).toBe('12 MB/s')
  })

  it('shows both rates, and neither before the estimator warms up', () => {
    render({ ...halfway, bytesPerSecond: 1_500_000, filesPerSecond: 27 })
    expect(texts('.rate')).toEqual(['2 MB/s', '27 files/s'])

    document.body.innerHTML = ''
    render(halfway)
    expect(texts('.rate')).toEqual(['', ''])
  })

  it('renders the time left, and keeps the cell in place before an ETA lands', () => {
    render({ ...halfway, etaSeconds: seconds(154) })
    expect(texts('.time')).toEqual(['2m 34s left'])

    document.body.innerHTML = ''
    render(halfway)
    // Present but empty: the cell holds its width so nothing shifts when the
    // estimate arrives.
    expect(texts('.time')).toEqual([''])
  })

  it('a stall displaces the countdown we no longer believe', () => {
    render({
      ...halfway,
      etaSeconds: seconds(154),
      stall: { stillForSeconds: 45, reason: 'destination', inFlight: 2 },
    })
    expect(texts('.time')).toEqual(['No progress for 45s'])
    expect(target.querySelector('.time')?.classList.contains('stalled')).toBe(true)
  })

  it('labels its rows in the dialog and drops the labels in a list row', () => {
    render(halfway)
    // "Bytes", not "Size": the label pairs with the count bar under it.
    expect(texts('.bar-label')).toEqual(['Bytes', 'Files'])

    document.body.innerHTML = ''
    render({ ...halfway, countKind: 'items' })
    expect(texts('.bar-label')).toEqual(['Bytes', 'Items'])

    document.body.innerHTML = ''
    render({ ...halfway, density: 'compact' })
    expect(texts('.bar-label')).toEqual([])
  })
})
