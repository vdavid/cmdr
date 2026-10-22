import { Composition, continueRender, delayRender, staticFile } from 'remotion'
import { FPS, HEIGHT, WIDTH } from './config.ts'
import { buildTimeline, f, type FootageManifest, type Timeline, type VoiceManifest } from './timeline.ts'
import { Video } from './Video.tsx'

// Inter is the website's face; the variable font covers every weight we use.
const fontHandle = delayRender('Loading Inter')
const inter = new FontFace('Inter', `url(${staticFile('brand/fonts/inter.woff2')}) format('woff2')`, {
  weight: '100 900',
})
inter
  .load()
  .then((loaded) => {
    document.fonts.add(loaded)
    continueRender(fontHandle)
  })
  .catch((err: unknown) => {
    console.error('Inter failed to load; run `pnpm prepare-assets`', err)
    continueRender(fontHandle)
  })

/** `stem` renders one audio stem alone (`--props='{"stem":"voice"}'`), to measure levels; omit it for the mix. */
export type VideoProps = { timeline: Timeline | null; voice: VoiceManifest | null; stem?: 'voice' | 'music' }

async function loadJson<T>(path: string, fallback?: T): Promise<T> {
  const res = await fetch(staticFile(path))
  if (res.ok) return (await res.json()) as T
  if (fallback !== undefined) return fallback
  throw new Error(`Missing public/${path}. See README.md for how to generate it.`)
}

export const RemotionRoot: React.FC = () => (
  <Composition
    id="CmdrDemo"
    component={Video}
    width={WIDTH}
    height={HEIGHT}
    fps={FPS}
    durationInFrames={90 * FPS}
    defaultProps={{ timeline: null, voice: null } satisfies VideoProps}
    calculateMetadata={async ({ props }) => {
      const voice = await loadJson<VoiceManifest>('voice/manifest.json')
      const footage = await loadJson<FootageManifest>('footage/manifest.json', { beats: {} })
      const timeline = buildTimeline(voice, footage)
      return { durationInFrames: f(timeline.duration), props: { ...props, timeline, voice } }
    }}
  />
)
