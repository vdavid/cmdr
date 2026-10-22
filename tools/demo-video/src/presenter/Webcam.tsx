import { OffthreadVideo, Sequence, staticFile } from 'remotion'
import { useTimeline } from '../context.ts'
import { f } from '../timeline.ts'

// Real webcam takes in the presenter slot: `public/webcam/<sectionId>.mp4`, one take per script section, whose audio
// also became the voice (`pnpm voice --source=webcam`). Each take plays in sync with its voice clip and keeps
// rolling (silently) until the next one starts, so there are no frozen faces between lines.
export const Webcam: React.FC = () => {
  const { timeline, voice } = useTimeline()
  const clips = timeline.voice.map((clip, i, all) => {
    // Take time = voice-file time + the silence trimmed off the take's start.
    const takeFrom = clip.from + voice.sections[clip.sectionId].lead
    const prevEnd = i > 0 ? all[i - 1].start + all[i - 1].to - all[i - 1].from : 0
    // Start the take up to 0.6 s early (a breath before the line), without overlapping the previous line.
    const pre = Math.max(0, Math.min(0.6, takeFrom, clip.start - prevEnd))
    return { clip, takeFrom, pre, start: clip.start - pre }
  })
  return (
    <>
      {clips.map(({ clip, takeFrom, pre, start }, i) => {
        const end = i < clips.length - 1 ? clips[i + 1].start : timeline.duration
        return (
          <Sequence key={i} from={f(start)} durationInFrames={Math.max(1, f(end) - f(start))} layout="none">
            <OffthreadVideo
              src={staticFile(`webcam/${clip.sectionId}.mp4`)}
              trimBefore={f(takeFrom - pre)}
              muted
              style={{ width: '100%', height: '100%', objectFit: 'cover' }}
            />
          </Sequence>
        )
      })}
    </>
  )
}
