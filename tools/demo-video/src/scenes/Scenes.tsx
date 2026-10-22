import { sectionAt, useT, useTimeline } from '../context.ts'
import { Beat } from './Beat.tsx'
import { Close } from './Close.tsx'
import { Intro } from './Intro.tsx'
import { IntroEverywhere } from './IntroEverywhere.tsx'
import { LogoSting } from './LogoSting.tsx'

// Every cut is a hard cut on a bar line, so only the current section renders.
export const Scenes: React.FC = () => {
  const t = useT()
  const { timeline, spec } = useTimeline()
  const section = sectionAt(timeline, t)
  const firstVoice = timeline.voice[0].start
  const IntroScene = spec.intro === 'everywhere' ? IntroEverywhere : Intro
  return (
    <>
      {section.kind === 'intro' && <IntroScene t={t} section={section} />}
      {section.kind === 'beat' && <Beat key={section.id} t={t} section={section} />}
      {section.kind === 'close' && <Close t={t} section={section} />}
      {t < firstVoice + 0.2 && <LogoSting t={t} end={firstVoice} />}
    </>
  )
}
