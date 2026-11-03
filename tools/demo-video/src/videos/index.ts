// Every video this project renders. Add one: write its spec beside these, add it here, and register nothing else
// (Root.tsx and the scripts read this list).
import { anywhere } from './anywhere.ts'
import { demo } from './demo.ts'
import type { VideoSpec } from './types.ts'

export const VIDEOS: VideoSpec[] = [demo, anywhere]

export function videoById(id: string): VideoSpec {
  const v = VIDEOS.find((x) => x.id === id)
  if (!v) throw new Error(`Unknown video "${id}". Known: ${VIDEOS.map((x) => x.id).join(', ')}`)
  return v
}

/** The `--video=<id>` CLI argument; defaults to the first video. */
export function videoFromArgs(args: string[]): VideoSpec {
  const id = args.find((a) => a.startsWith('--video='))?.split('=')[1]
  return id ? videoById(id) : VIDEOS[0]
}
