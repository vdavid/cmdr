/**
 * The demo's macOS window buttons (close, minimize, zoom), drawn by the page because no OS draws
 * them. Every window's sit where the app's own `trafficLightPosition` puts them on the desktop
 * (the main window's from `tauri.conf.json`, a panel's from its creation options, `windows.ts`),
 * over the overlay title bar the app already leaves them room in. Measured off macOS 27's own
 * rendering; see `DETAILS.md` § Traffic lights.
 */
import tauriConfig from '../../../src-tauri/tauri.conf.json'
import { getMessage } from '#lib/intl/messages.svelte.js'

/** What one window's lights do. A missing action leaves that light inert (`aria-disabled`). */
export interface TrafficLightActions {
  close?: () => void
  minimize?: () => void
  zoom?: () => void
}

/** The zoom light's hover glyph: macOS shows arrows for full screen and a plus for a plain zoom. */
export type ZoomGlyph = 'enterFullScreen' | 'exitFullScreen' | 'zoom'

export interface TrafficLights {
  root: HTMLElement
  /** Grey when the window isn't key, as macOS draws an inactive window's buttons. */
  setActive(active: boolean): void
  setZoomGlyph(glyph: ZoomGlyph): void
}

/** A window's `trafficLightPosition`, in logical points, as the app hands it to Tauri. */
export interface TrafficLightPosition {
  x: number
  y: number
}

/** The close button's top-left corner inside its window, in CSS pixels. */
export interface LightsInset {
  left: number
  top: number
}

/**
 * How far above `trafficLightPosition.y` the close button's top edge lands. Tauri (tao's
 * `position_traffic_lights`) puts the buttons at `x` and grows the title-bar container to the
 * button's frame height plus `y`; AppKit keeps the button's frame pinned to the container's
 * bottom, so the visible 14 pt light ends up 9 pt above `y` (verified on macOS 27,
 * `screencapture -l` of Cmdr 0.59.0 at 2x, 2026-11-04: main and viewer (9, 17) draw at (9, 8),
 * Settings (20, 29) at (20, 20)).
 */
const LIGHT_TOP_ABOVE_Y = 9

/** Where a window's lights draw, from the `trafficLightPosition` the app gives that window. */
export function lightsInset(position: TrafficLightPosition): LightsInset {
  return { left: position.x, top: position.y - LIGHT_TOP_ABOVE_Y }
}

/** The main window's, from the same `tauri.conf.json` the desktop builds it from. */
export const MAIN_LIGHTS_INSET = lightsInset(tauriConfig.app.windows[0].trafficLightPosition)

/** A glyph as a CSS mask: one SVG in the 28-unit box the 2x measurements were taken in. */
function glyph(shapes: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 28 28">${shapes}</svg>`
  return `url("data:image/svg+xml,${encodeURIComponent(svg)}")`
}

const GLYPHS = {
  close: glyph('<path d="M9 9 19 19M19 9 9 19" stroke="#000" stroke-width="4" stroke-linecap="round"/>'),
  minimize: glyph('<rect x="5.6" y="11.7" width="16.8" height="4.6" rx="2"/>'),
  enterFullScreen: glyph('<path d="M7.8 7.8h10L7.8 17.8zM20.2 20.2h-10l10-10z"/>'),
  exitFullScreen: glyph('<path d="M13.2 13.2h-8.4l8.4-8.4zM14.8 14.8h8.4l-8.4 8.4z"/>'),
  zoom: glyph(
    '<rect x="5.6" y="11.7" width="16.8" height="4.6" rx="2"/><rect x="11.7" y="5.6" width="4.6" height="16.8" rx="2"/>',
  ),
}

/*
 * Colors and gradients sampled from AppKit's own rendering of a standard window's buttons on
 * macOS 27 (light and dark, key and inactive, hovered), at 2x. Light mode: a top-to-bottom fill
 * that brightens toward the bottom, a dark rim strongest at the sides, and a soft tinted glow
 * below. Dark mode: a bright rim at the top and bottom, no glow.
 */
const STYLE = `
.cmdr-demo-lights {
  display: flex;
  gap: 9px;
  flex-shrink: 0;
}
.cmdr-demo-lights-main {
  position: fixed;
  left: ${String(MAIN_LIGHTS_INSET.left)}px;
  top: calc(var(--cmdr-demo-window-top, 0px) + ${String(MAIN_LIGHTS_INSET.top)}px);
  z-index: var(--z-sticky);
}
.cmdr-demo-light {
  position: relative;
  width: 14px;
  height: 14px;
  padding: 0;
  border: 0;
  border-radius: 50%;
  background: var(--l-fill);
  box-shadow: var(--l-glow, none);
  cursor: default;
  -webkit-tap-highlight-color: transparent;
}
.cmdr-demo-light::before {
  content: '';
  position: absolute;
  inset: 0;
  border-radius: 50%;
  padding: 1px;
  background: var(--l-rim);
  -webkit-mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0);
  -webkit-mask-composite: xor;
  mask: linear-gradient(#000 0 0) content-box exclude, linear-gradient(#000 0 0);
}
.cmdr-demo-light::after {
  content: '';
  position: absolute;
  inset: 0;
  background: var(--l-glyph-color);
  -webkit-mask: var(--l-glyph) center / 14px 14px no-repeat;
  mask: var(--l-glyph) center / 14px 14px no-repeat;
  opacity: 0;
}
.cmdr-demo-lights:hover .cmdr-demo-light::after { opacity: 1; }
.cmdr-demo-light:active:not([aria-disabled='true']) { filter: brightness(0.85); }
.cmdr-demo-light:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 1px; }
.cmdr-demo-light[data-light='close'] {
  --l-glyph: ${GLYPHS.close};
  --l-fill: linear-gradient(#e5776a 0%, #e5786c 33%, #e5857a 52%, #e5948b 78%, #e59288 100%);
  --l-rim: linear-gradient(to bottom, transparent 5%, #af0a0a 50%, transparent 95%);
  --l-glow: 0 1px 2px rgb(229 120 108 / 0.35);
  --l-glyph-color: #901c12;
}
.cmdr-demo-light[data-light='minimize'] {
  --l-glyph: ${GLYPHS.minimize};
  --l-fill: linear-gradient(#eebd4f 0%, #efc14d 33%, #f2cb57 52%, #f5d662 78%, #f5d561 100%);
  --l-rim: linear-gradient(to bottom, transparent 5%, #8d1e0a 50%, transparent 95%);
  --l-glow: 0 1px 2px rgb(239 193 77 / 0.35);
  --l-glyph-color: #ae4d1e;
}
.cmdr-demo-light[data-light='zoom'] {
  --l-fill: linear-gradient(#80c847 0%, #87ce50 22%, #9dd56f 52%, #b0dc8a 78%, #aedc87 100%);
  --l-rim: linear-gradient(to bottom, transparent 5%, #16540a 50%, transparent 95%);
  --l-glow: 0 1px 2px rgb(141 207 87 / 0.35);
  --l-glyph-color: #518624;
}
.cmdr-demo-lights[data-zoom='enterFullScreen'] [data-light='zoom'] { --l-glyph: ${GLYPHS.enterFullScreen}; }
.cmdr-demo-lights[data-zoom='exitFullScreen'] [data-light='zoom'] { --l-glyph: ${GLYPHS.exitFullScreen}; }
.cmdr-demo-lights[data-zoom='zoom'] [data-light='zoom'] { --l-glyph: ${GLYPHS.zoom}; }
.cmdr-demo-lights[data-active='false']:not(:hover) .cmdr-demo-light {
  --l-fill: linear-gradient(#f5f6f7 0%, #f1f1f5 22%, #efeff3 33%, #f1f1f5 48%, #f5f6f7 67%, #f6f7f8 85%, #f5f6f7 100%);
  --l-rim: linear-gradient(to bottom, transparent 5%, #ababaf 50%, transparent 95%);
  --l-glow: none;
}
@media (prefers-color-scheme: dark) {
  .cmdr-demo-light { --l-glow: none !important; }
  .cmdr-demo-light[data-light='close'] {
    --l-fill: radial-gradient(ellipse 80% 60% at 50% 88%, #da7268, transparent),
      linear-gradient(#e26f62 0%, #dd5c4d 33%, #db5f50 100%);
    --l-rim: linear-gradient(to bottom, #f6b2a7 0%, transparent 30% 70%, #f6b0a6 100%);
    --l-glyph-color: #7c160e;
  }
  .cmdr-demo-light[data-light='minimize'] {
    --l-fill: radial-gradient(ellipse 80% 60% at 50% 88%, #f4d257, transparent),
      linear-gradient(#ecb736 0%, #ecb600 22%, #edb700 100%);
    --l-rim: linear-gradient(to bottom, #f8e870 0%, transparent 30% 70%, #f8f27d 100%);
    --l-glyph-color: #954118;
  }
  .cmdr-demo-light[data-light='zoom'] {
    --l-fill: radial-gradient(ellipse 80% 60% at 50% 88%, #79c252, transparent),
      linear-gradient(#74c343 0%, #69be1f 19%, #59b900 30%, #5aba00 100%);
    --l-rim: linear-gradient(to bottom, #b0f086 0%, transparent 30% 70%, #acef85 100%);
    --l-glyph-color: #45711d;
  }
  .cmdr-demo-lights[data-active='false']:not(:hover) .cmdr-demo-light {
    --l-fill: linear-gradient(#494949 0%, #484848 30%, #444444 48%, #404040 70%, #3f3f3f 100%);
    --l-rim: linear-gradient(to bottom, #979797 0%, transparent 30% 70%, #8f8f8f 100%);
  }
}
`

/** Injects the lights' stylesheet once per page. */
function ensureStyle(): void {
  if (document.getElementById('cmdr-demo-lights-style')) return
  const style = document.createElement('style')
  style.id = 'cmdr-demo-lights-style'
  style.textContent = STYLE
  document.head.append(style)
}

const LABEL_KEYS = {
  close: 'ui.modalDialog.close',
  minimize: 'menu.window.minimize',
  zoom: 'menu.window.zoom',
} as const

export function createTrafficLights(actions: TrafficLightActions, zoomGlyph: ZoomGlyph): TrafficLights {
  ensureStyle()
  const root = document.createElement('div')
  root.className = 'cmdr-demo-lights'
  root.setAttribute('role', 'group')
  root.dataset.active = 'true'
  root.dataset.zoom = zoomGlyph
  for (const kind of ['close', 'minimize', 'zoom'] as const) {
    const button = document.createElement('button')
    button.type = 'button'
    button.className = 'cmdr-demo-light'
    button.dataset.light = kind
    button.setAttribute('aria-label', getMessage(LABEL_KEYS[kind]))
    // Like macOS's, the lights aren't Tab stops: the keyboard has ⌃W, Esc, and the menu.
    button.tabIndex = -1
    const action = actions[kind]
    if (action) button.addEventListener('click', action)
    else button.setAttribute('aria-disabled', 'true')
    // A press on a light never takes focus from the window's content, as on macOS.
    button.addEventListener('mousedown', (event) => {
      event.preventDefault()
    })
    root.append(button)
  }
  return {
    root,
    setActive: (active) => {
      root.dataset.active = String(active)
    },
    setZoomGlyph: (glyph) => {
      root.dataset.zoom = glyph
    },
  }
}

/** Whether this page can enter full screen at all: a frame needs its embedder's `allow="fullscreen"`. */
function canFullScreen(): boolean {
  return document.fullscreenEnabled && typeof document.documentElement.requestFullscreen === 'function'
}

/**
 * The main window's lights, at its top-left. Zoom is the browser's full screen, the closest a page
 * gets to the green button's full screen (refused quietly where the page can't, as in a frame that
 * doesn't allow it). Close and minimize stay inert: a page can't close or hide its own tab.
 */
export function createMainWindowLights(): TrafficLights {
  const fullScreen = canFullScreen()
  const lights = createTrafficLights(
    fullScreen
      ? {
          zoom: () => {
            const done = document.fullscreenElement
              ? // eslint-disable-next-line cmdr/webkit-floor-dom -- only wired when `canFullScreen()` found `requestFullscreen`, which ships with `exitFullscreen`
                document.exitFullscreen()
              : // eslint-disable-next-line cmdr/webkit-floor-dom -- only wired when `canFullScreen()` found it
                document.documentElement.requestFullscreen()
            done.catch(() => undefined)
          },
        }
      : {},
    'enterFullScreen',
  )
  lights.root.classList.add('cmdr-demo-lights-main')
  document.addEventListener('fullscreenchange', () => {
    lights.setZoomGlyph(document.fullscreenElement ? 'exitFullScreen' : 'enterFullScreen')
  })
  return lights
}
