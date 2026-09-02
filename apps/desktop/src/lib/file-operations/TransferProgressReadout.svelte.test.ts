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

  it('draws no bar and no percentage while the count total is still unknown', () => {
    // A total of zero is the backend saying "still counting", not "nothing to
    // do". A fraction against it is a fraction against a number that hasn't been
    // decided, and it renders as a bar frozen at 0% beside "(0%)" on an
    // operation that is moving. So the readout reports what it knows — how many
    // it has got through — and waits for a denominator before drawing one.
    render({ bytesDone: 0, bytesTotal: 0, filesDone: 17_238, filesTotal: 0 })
    expect(target.querySelectorAll('[role="progressbar"]').length).toBe(0)
    expect(texts('.amount')).toEqual(['17,238'])
    expect(texts('.percent')).toEqual([''])
  })

  it('never shows 100% off a total that only equals the done count because both are zero', () => {
    render({ bytesDone: 0, bytesTotal: 0, filesDone: 0, filesTotal: 0 })
    expect(texts('.percent')).toEqual([''])
  })

  it('shows both amounts: bytes done/total and files done/total', () => {
    render(halfway)
    expect(texts('.amount')).toEqual(['50 bytes / 200 bytes', '1 / 4'])
  })

  it('coarsens live sizes so digits stop churning, without losing a whole gigabyte', () => {
    // 7.61 GB / 22.66 GB, at 11.96 MB/s. A size column elsewhere keeps its two
    // decimals; a number that changes several times a second doesn't earn them,
    // but it doesn't get to round away the difference between the two either.
    render({ ...halfway, bytesDone: 7_610_000_000, bytesTotal: 22_660_000_000, bytesPerSecond: 11_960_000 })
    expect(texts('.amount')[0]).toBe('7.6 GB / 23 GB')
    expect(texts('.rate')[0]).toBe('12 MB/s')
  })

  it('never prints the same number twice for two different sizes', () => {
    // Pre-fix this read "2 GB / 2 GB (70%)": whole units at every scale, so a
    // 1.7 GB / 2.4 GB transfer showed two identical numbers beside a percentage
    // that contradicted them. Every transfer in the 1-10 GB range hit it.
    render({ ...halfway, bytesDone: 1_700_000_000, bytesTotal: 2_400_000_000 })
    expect(texts('.amount')[0]).toBe('1.7 GB / 2.4 GB')
    expect(texts('.percent')[0]).toBe('(71%)')
  })

  it('shows both rates, and neither before the estimator warms up', () => {
    render({ ...halfway, bytesPerSecond: 1_500_000, filesPerSecond: 27 })
    expect(texts('.rate')).toEqual(['1.5 MB/s', '27 files/s'])

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

  it('labels its rows in BOTH densities, so a queue row is no more of a puzzle than the dialog', () => {
    render(halfway)
    // "Bytes", not "Size": the label pairs with the count bar under it.
    expect(texts('.bar-label')).toEqual(['Bytes', 'Files'])

    document.body.innerHTML = ''
    render({ ...halfway, countKind: 'items' })
    expect(texts('.bar-label')).toEqual(['Bytes', 'Items'])

    // A queue row gets the same two words. Two unlabelled bars side by side read
    // as a puzzle, and the units in the amounts don't answer it fast enough.
    document.body.innerHTML = ''
    render({ ...halfway, density: 'compact' })
    expect(texts('.bar-label')).toEqual(['Bytes', 'Files'])

    document.body.innerHTML = ''
    render({ ...halfway, density: 'compact', countKind: 'items' })
    expect(texts('.bar-label')).toEqual(['Bytes', 'Items'])
  })
})
