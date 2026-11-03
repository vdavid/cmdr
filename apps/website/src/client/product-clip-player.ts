// The browser half of `ProductClip.astro`: loading, play state, the progress line, scrubbing, and
// key captions for every `[data-product-clip]` on the page. The component's frontmatter writes the
// `data-*` attributes this reads; its doc comment is the behavior overview.

type Theme = 'light' | 'dark'

interface Clip {
  root: HTMLElement
  video: HTMLVideoElement
  button: HTMLButtonElement
  visible: boolean
  /** Whether the clip should play while on screen: autoplay, unless reduced motion or the visitor paused it. */
  wantsPlay: boolean
  /** Theme of the loaded sources; `null` until the clip first comes on screen. */
  theme: Theme | null
  slider: HTMLElement
  progress: HTMLElement
  /** The whole second last written to `aria-valuenow`, so a 60 fps redraw doesn't churn the a11y tree. */
  announcedSecond: number
  /** The animation frame driving the progress line while playing, if any. */
  frame: number | null
  drag: Drag | null
  /** Where to seek once the metadata arrives, for a seek made before the clip loaded. */
  pendingSeek: number | null
  captions: Captions | null
}

interface ClipKey {
  at: number
  label: string
}

interface Captions {
  el: HTMLElement
  keys: Record<Theme, ClipKey[]>
  /** The `keys` index range on screen, `[first, end)`, so the DOM changes only when it moves. */
  first: number
  end: number
}

interface Drag {
  pointerId: number
  startX: number
  startTime: number
  wasPlaying: boolean
  /** False until the pointer travels past `DRAG_THRESHOLD_PX`; a press that never does is a click. */
  moved: boolean
}

const AV1_TYPE = 'video/mp4; codecs="av01.0.08M.08"'
const DRAG_THRESHOLD_PX = 6
const ARROW_STEP_S = 1
const PAGE_STEP_S = 5
/** Seeking to the very end of a looping video lands on its first frame, so End stops just short. */
const END_MARGIN_S = 0.05
/** How long a key cap stays up, and the tail of that over which it fades. */
const KEY_VISIBLE_S = 1.6
const KEY_FADE_S = 0.4
/** A cap pops in over this long, from slightly smaller. */
const KEY_POP_S = 0.12
/** At most this many caps at once; a burst shows its newest presses. */
const KEY_MAX = 5
const clips = new Set<Clip>()
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)')
const prefersLight = window.matchMedia('(prefers-color-scheme: light)')

function activeTheme(): Theme {
  const explicit = document.documentElement.dataset.theme
  if (explicit === 'light' || explicit === 'dark') return explicit
  return prefersLight.matches ? 'light' : 'dark'
}

function source(src: string, type: string): HTMLSourceElement {
  const el = document.createElement('source')
  el.src = src
  el.type = type
  return el
}

/** Points the video at the active theme's files, keeping its position and play state. */
function loadSources(clip: Clip): void {
  const theme = activeTheme()
  if (clip.theme === theme) return
  const { video, root } = clip
  const resumeAt = video.currentTime
  const wasPlaying = !video.paused
  const av1 = theme === 'light' ? root.dataset.av1Light : root.dataset.av1Dark
  const h264 = theme === 'light' ? root.dataset.h264Light : root.dataset.h264Dark
  const sources: HTMLSourceElement[] = []
  if (av1) sources.push(source(av1, AV1_TYPE))
  if (h264) sources.push(source(h264, 'video/mp4'))
  video.replaceChildren(...sources)
  clip.theme = theme
  // Each theme's recording has its own timings, so the shown range must redraw.
  if (clip.captions) {
    clip.captions.first = -1
    clip.captions.end = -1
  }
  delete root.dataset.ready
  video.preload = 'auto'
  video.load()
  if (resumeAt > 0) {
    video.addEventListener(
      'loadedmetadata',
      () => {
        video.currentTime = resumeAt
      },
      { once: true },
    )
  }
  if (wasPlaying) play(clip)
}

function play(clip: Clip): void {
  loadSources(clip)
  clip.video.play().catch(() => {
    // Autoplay refused (data saver, an old browser policy): stay on the poster with the play button showing.
    syncState(clip)
  })
}

function syncState(clip: Clip): void {
  const paused = clip.video.paused
  clip.root.dataset.state = paused ? 'paused' : 'playing'
  clip.button.setAttribute('aria-label', paused ? 'Play clip' : 'Pause clip')
  if (paused || reducedMotion.matches) stopFrames(clip)
  else startFrames(clip)
}

/** Seconds; the manifest's value until the video's own metadata arrives. */
function duration(clip: Clip): number {
  const fromVideo = clip.video.duration
  return Number.isFinite(fromVideo) && fromVideo > 0 ? fromVideo : Number(clip.root.dataset.duration) || 0
}

function renderProgress(clip: Clip): void {
  const total = duration(clip)
  const now = clip.video.currentTime
  const fraction = total > 0 ? Math.min(1, now / total) : 0
  clip.progress.style.transform = `scaleX(${fraction})`
  renderKeys(clip, now)
  const second = Math.round(now)
  if (second === clip.announcedSecond) return
  clip.announcedSecond = second
  clip.slider.setAttribute('aria-valuenow', String(second))
  clip.slider.setAttribute('aria-valuetext', `${second} of ${Math.round(total)} seconds`)
}

/**
 * Shows the presses made in the last `KEY_VISIBLE_S` before `now`. It's a pure function of the
 * playback position, so scrubbing, seeking, and looping need nothing special. With reduced
 * motion, caps appear and disappear without the pop or the fade.
 */
function renderKeys(clip: Clip, now: number): void {
  const captions = clip.captions
  if (!captions) return
  const keys = captions.keys[clip.theme ?? activeTheme()]
  // Presses are sorted, so the visible ones are one contiguous run ending at `now`.
  let end = 0
  while (end < keys.length && keys[end].at <= now) end++
  let first = end
  while (first > 0 && now - keys[first - 1].at < KEY_VISIBLE_S && end - first < KEY_MAX) first--
  if (first !== captions.first || end !== captions.end) {
    captions.el.replaceChildren(
      ...keys.slice(first, end).map((key) => {
        const cap = document.createElement('kbd')
        cap.className = 'clip-key'
        cap.textContent = key.label
        return cap
      }),
    )
    captions.first = first
    captions.end = end
  }
  if (reducedMotion.matches) return
  const caps = captions.el.children
  for (let i = 0; i < caps.length; i++) {
    const age = now - keys[first + i].at
    const cap = caps[i] as HTMLElement
    cap.style.opacity = String(Math.min(1, (KEY_VISIBLE_S - age) / KEY_FADE_S))
    cap.style.transform = `scale(${0.9 + 0.1 * Math.min(1, age / KEY_POP_S)})`
  }
}

function parseKeys(json: string | undefined): ClipKey[] {
  if (!json) return []
  try {
    return JSON.parse(json) as ClipKey[]
  } catch {
    return []
  }
}

/**
 * A smooth line needs a redraw per frame, so it runs only while the clip plays. With reduced motion
 * the line steps on `timeupdate` (a few times a second) instead of gliding.
 */
function startFrames(clip: Clip): void {
  if (clip.frame !== null) return
  const tick = () => {
    renderProgress(clip)
    clip.frame = requestAnimationFrame(tick)
  }
  clip.frame = requestAnimationFrame(tick)
}

function stopFrames(clip: Clip): void {
  if (clip.frame === null) return
  cancelAnimationFrame(clip.frame)
  clip.frame = null
}

/** Seeks to `seconds`, loading the sources first when the clip hasn't come on screen yet. */
function seekTo(clip: Clip, seconds: number): void {
  const { video } = clip
  const go = (to: number) => {
    video.currentTime = Math.max(0, Math.min(to, duration(clip) - END_MARGIN_S))
    renderProgress(clip)
  }
  if (video.readyState >= HTMLMediaElement.HAVE_METADATA) {
    go(seconds)
    return
  }
  // A drag fires many seeks before the metadata lands; only the latest one matters.
  const firstWait = clip.pendingSeek === null
  clip.pendingSeek = seconds
  if (!firstWait) return
  video.addEventListener(
    'loadedmetadata',
    () => {
      if (clip.pendingSeek !== null) go(clip.pendingSeek)
      clip.pendingSeek = null
    },
    { once: true },
  )
  loadSources(clip)
}

function apply(clip: Clip): void {
  // A drag owns the play state until it ends, then restores it itself.
  if (clip.drag?.moved) return
  if (clip.visible && clip.wantsPlay) play(clip)
  else if (!clip.video.paused) clip.video.pause()
}

const visibility = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      const clip = [...clips].find((c) => c.root === entry.target)
      if (!clip) continue
      clip.visible = entry.isIntersecting
      apply(clip)
    }
  },
  { rootMargin: '120px 0px', threshold: 0.15 },
)

function bind(root: HTMLElement): void {
  const video = root.querySelector('video')
  const button = root.querySelector<HTMLButtonElement>('[data-clip-toggle]')
  const slider = root.querySelector<HTMLElement>('[data-clip-scrub]')
  const progress = root.querySelector<HTMLElement>('[data-clip-progress]')
  if (!video || !button || !slider || !progress) return
  const keysEl = root.querySelector<HTMLElement>('[data-clip-keys]')
  const captions: Captions | null = keysEl
    ? {
        el: keysEl,
        keys: { light: parseKeys(root.dataset.keysLight), dark: parseKeys(root.dataset.keysDark) },
        first: 0,
        end: 0,
      }
    : null
  const clip: Clip = {
    root,
    video,
    button,
    visible: false,
    wantsPlay: !reducedMotion.matches,
    theme: null,
    slider,
    progress,
    announcedSecond: 0,
    frame: null,
    drag: null,
    pendingSeek: null,
    captions,
  }
  clips.add(clip)
  root.dataset.bound = ''

  const toggle = () => {
    clip.wantsPlay = clip.video.paused
    if (clip.wantsPlay) play(clip)
    else clip.video.pause()
  }
  button.addEventListener('click', toggle)
  video.addEventListener('play', () => syncState(clip))
  video.addEventListener('pause', () => syncState(clip))
  video.addEventListener('loadeddata', () => {
    root.dataset.ready = ''
    renderProgress(clip)
  })
  video.addEventListener('seeked', () => renderProgress(clip))
  video.addEventListener('timeupdate', () => {
    if (clip.frame === null) renderProgress(clip)
  })

  // Press and release in place toggles play; a horizontal drag scrubs relative to where it
  // started (the frame's full width spans the whole clip), pausing only while it lasts.
  slider.addEventListener('pointerdown', (event) => {
    if (event.pointerType === 'mouse' && event.button !== 0) return
    clip.drag = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startTime: video.currentTime,
      wasPlaying: !video.paused,
      moved: false,
    }
    slider.setPointerCapture(event.pointerId)
  })
  slider.addEventListener('pointermove', (event) => {
    const drag = clip.drag
    if (!drag || drag.pointerId !== event.pointerId) return
    const dx = event.clientX - drag.startX
    if (!drag.moved) {
      if (Math.abs(dx) < DRAG_THRESHOLD_PX) return
      drag.moved = true
      root.dataset.scrubbing = ''
      video.pause()
    }
    seekTo(clip, drag.startTime + (dx / slider.clientWidth) * duration(clip))
  })
  const endDrag = (event: PointerEvent, completed: boolean) => {
    const drag = clip.drag
    if (!drag || drag.pointerId !== event.pointerId) return
    clip.drag = null
    if (slider.hasPointerCapture(event.pointerId)) slider.releasePointerCapture(event.pointerId)
    if (!drag.moved) {
      if (completed) toggle()
      return
    }
    delete root.dataset.scrubbing
    if (drag.wasPlaying && clip.visible) play(clip)
  }
  slider.addEventListener('pointerup', (event) => endDrag(event, true))
  // The browser took the gesture over (a vertical swipe became a page scroll): no toggle.
  slider.addEventListener('pointercancel', (event) => endDrag(event, false))

  slider.addEventListener('keydown', (event) => {
    const now = video.currentTime
    let target: number
    switch (event.key) {
      case ' ':
      case 'Enter':
        event.preventDefault()
        toggle()
        return
      case 'ArrowRight':
      case 'ArrowUp':
        target = now + ARROW_STEP_S
        break
      case 'ArrowLeft':
      case 'ArrowDown':
        target = now - ARROW_STEP_S
        break
      case 'PageUp':
        target = now + PAGE_STEP_S
        break
      case 'PageDown':
        target = now - PAGE_STEP_S
        break
      case 'Home':
        target = 0
        break
      case 'End':
        target = duration(clip)
        break
      default:
        return
    }
    event.preventDefault()
    seekTo(clip, target)
  })
  visibility.observe(root)
}

function reloadAllForTheme(): void {
  for (const clip of clips) if (clip.theme !== null) loadSources(clip)
}

new MutationObserver(reloadAllForTheme).observe(document.documentElement, {
  attributes: true,
  attributeFilter: ['data-theme'],
})
prefersLight.addEventListener('change', reloadAllForTheme)
reducedMotion.addEventListener('change', () => {
  for (const clip of clips) {
    clip.wantsPlay = !reducedMotion.matches
    apply(clip)
    syncState(clip)
  }
})

document.addEventListener('astro:page-load', () => {
  // Forget clips a client-side navigation removed.
  for (const clip of clips) {
    if (!clip.root.isConnected) {
      stopFrames(clip)
      visibility.unobserve(clip.root)
      clips.delete(clip)
    }
  }
  document.querySelectorAll<HTMLElement>('[data-product-clip]:not([data-bound])').forEach(bind)
})
