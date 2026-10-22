import { Composition, continueRender, delayRender, staticFile } from 'remotion'
import { FPS, HEIGHT, WIDTH, paths } from './config.ts'
import { buildTimeline, f, type FootageManifest, type Timeline, type VoiceManifest } from './timeline.ts'
import { Video } from './Video.tsx'
import { VIDEOS, videoById } from './videos/index.ts'

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

/** `stem` renders one audio stem alone (`--props=props/stem-voice.json`), to measure levels; omit it for the mix. */
export type VideoProps = {
  videoId: string
  timeline: Timeline | null
  voice: VoiceManifest | null
  stem?: 'voice' | 'music'
}

async function loadJson<T>(path: string, fallback?: T): Promise<T> {
  const res = await fetch(staticFile(path))
  if (res.ok) return (await res.json()) as T
  if (fallback !== undefined) return fallback
  throw new Error(`Missing public/${path}. See README.md for how to generate it.`)
}

export const RemotionRoot: React.FC = () => (
  <>
    {VIDEOS.map((spec) => (
      <Composition
        key={spec.id}
        id={spec.compositionId}
        component={Video}
        width={WIDTH}
        height={HEIGHT}
        fps={FPS}
        durationInFrames={90 * FPS}
        defaultProps={{ videoId: spec.id, timeline: null, voice: null } satisfies VideoProps}
        calculateMetadata={async ({ props }) => {
          const video = videoById(props.videoId)
          const p = paths(video.id)
          const voice = await loadJson<VoiceManifest>(`${p.voice}/manifest.json`)
          const footage = await loadJson<FootageManifest>(`${p.footage}/manifest.json`, { beats: {} })
          const onsets = await loadJson<number[]>(p.onsets, [])
          const timeline = buildTimeline(video, voice, footage, onsets)
          return { durationInFrames: f(timeline.duration), props: { ...props, timeline, voice } }
        }}
      />
    ))}
  </>
)
