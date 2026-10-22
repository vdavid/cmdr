import { AbsoluteFill } from 'remotion'
import { Soundtrack } from './audio/Soundtrack.tsx'
import { TimelineContext } from './context.ts'
import { Captions } from './overlays/Captions.tsx'
import { Effects } from './overlays/Effects.tsx'
import { Presenter } from './presenter/Presenter.tsx'
import type { VideoProps } from './Root.tsx'
import { Backdrop } from './scenes/Backdrop.tsx'
import { Scenes } from './scenes/Scenes.tsx'
import { COLORS } from './lib/motion.ts'
import { videoById } from './videos/index.ts'

// Layers, bottom to top. The presenter is one layer across the whole video, so the stick figure (or webcam) can
// shrink from full frame into the corner bubble and back without a cut.
export const Video: React.FC<VideoProps> = ({ videoId, timeline, voice, stem }) => {
  if (!timeline || !voice) return <AbsoluteFill style={{ background: COLORS.bg }} />
  return (
    <TimelineContext.Provider value={{ timeline, voice, spec: videoById(videoId) }}>
      <AbsoluteFill style={{ background: COLORS.bg, overflow: 'hidden' }}>
        <Backdrop />
        <Scenes />
        <Presenter />
        <Captions />
        <Effects />
        <Soundtrack stem={stem} />
      </AbsoluteFill>
    </TimelineContext.Provider>
  )
}
