// Builds the voice track, one file per script section, plus `public/voice/manifest.json` with each section's
// duration, word-timed caption phrases, and a per-frame loudness envelope (the stick figure's mouth).
//
//   OPENAI_API_KEY=$(secret OPENAI_API_KEY) node scripts/voice.ts           TTS, cached by text + voice settings
//   OPENAI_API_KEY=$(secret OPENAI_API_KEY) node scripts/voice.ts --force   regenerate every TTS take
//   OPENAI_API_KEY=$(secret OPENAI_API_KEY) node scripts/voice.ts --source=webcam
//                                     use the audio of public/webcam/<id>.mp4 instead of TTS (real takes)
//   node scripts/voice.ts --only=trust    limit to one section
//
// Raw takes live in assets/voice-raw/ (the paid part); everything in public/voice/ is derived and cheap to redo.
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { FPS, VOICE } from '../src/config.ts'
import { SCRIPT, VOICE_INSTRUCTIONS, type Section } from '../src/script.ts'
import type { VoiceManifest } from '../src/timeline.ts'
import { decodeMono, durationSec, rmsWindows, toDb } from './lib/audio.ts'

const ROOT = join(import.meta.dirname, '..')
const RAW = join(ROOT, 'assets/voice-raw')
const OUT = join(ROOT, 'public/voice')
const args = process.argv.slice(2)
const force = args.includes('--force')
const source = args.find((a) => a.startsWith('--source='))?.split('=')[1] ?? 'tts'
const only = args.find((a) => a.startsWith('--only='))?.split('=')[1]

type Word = { word: string; start: number; end: number }
type Phrase = { show: string; start: number; end: number }

function apiKey(): string {
  const key = process.env.OPENAI_API_KEY
  if (!key)
    throw new Error('Set OPENAI_API_KEY, for example: OPENAI_API_KEY=$(secret OPENAI_API_KEY) node scripts/voice.ts')
  return key
}

async function tts(section: Section, rawPath: string): Promise<void> {
  const input = section.phrases.map((p) => p.say).join(' ')
  const res = await fetch('https://api.openai.com/v1/audio/speech', {
    method: 'POST',
    headers: { Authorization: `Bearer ${apiKey()}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: VOICE.model,
      voice: VOICE.voice,
      input,
      instructions: VOICE_INSTRUCTIONS,
      response_format: 'wav',
    }),
  })
  if (!res.ok) throw new Error(`TTS for ${section.id}: HTTP ${res.status} ${await res.text()}`)
  writeFileSync(rawPath, Buffer.from(await res.arrayBuffer()))
}

async function transcribeWords(path: string): Promise<Word[]> {
  const form = new FormData()
  form.append('file', new Blob([readFileSync(path)], { type: 'audio/wav' }), 'voice.wav')
  form.append('model', 'whisper-1')
  form.append('response_format', 'verbose_json')
  form.append('timestamp_granularities[]', 'word')
  const res = await fetch('https://api.openai.com/v1/audio/transcriptions', {
    method: 'POST',
    headers: { Authorization: `Bearer ${apiKey()}` },
    body: form,
  })
  if (!res.ok) throw new Error(`Transcription: HTTP ${res.status} ${await res.text()}`)
  const json = (await res.json()) as { words?: Word[] }
  return json.words ?? []
}

const tokens = (s: string): string[] =>
  s
    .toLowerCase()
    .replace(/['’]/g, '')
    .replace(/[^a-z0-9]+/g, ' ')
    .trim()
    .split(' ')
    .filter(Boolean)

/** Aligns script tokens to transcribed words (edit-distance alignment), returning each script token's word index. */
function align(script: string[], heard: string[]): (number | undefined)[] {
  const n = script.length
  const m = heard.length
  const cost: number[][] = Array.from({ length: n + 1 }, (_, i) => Array.from({ length: m + 1 }, (_, j) => i + j))
  for (let i = 1; i <= n; i++)
    for (let j = 1; j <= m; j++)
      cost[i][j] = Math.min(
        cost[i - 1][j - 1] + (script[i - 1] === heard[j - 1] ? 0 : 1),
        cost[i - 1][j] + 1,
        cost[i][j - 1] + 1,
      )
  const out: (number | undefined)[] = new Array(n)
  let i = n
  let j = m
  while (i > 0 && j > 0) {
    if (cost[i][j] === cost[i - 1][j - 1] + (script[i - 1] === heard[j - 1] ? 0 : 1)) {
      out[i - 1] = j - 1
      i--
      j--
    } else if (cost[i][j] === cost[i - 1][j] + 1) i--
    else j--
  }
  return out
}

/**
 * The TTS model sometimes drops or garbles a sentence (seen on the intro: the last line went missing). A take passes
 * when the transcript matches most script words AND the last phrase is actually there.
 */
function takeIsComplete(section: Section, words: Word[]): boolean {
  const perPhrase = section.phrases.map((p) => tokens(p.say))
  const flat = perPhrase.flat()
  const heard = words.map((w) => tokens(w.word).join(''))
  const map = align(flat, heard)
  const exact = map.filter((j, i) => j !== undefined && heard[j] === flat[i]).length
  const lastPhraseStart = flat.length - perPhrase[perPhrase.length - 1].length
  const lastPhraseHeard = map
    .slice(lastPhraseStart)
    .some((j, i) => j !== undefined && heard[j] === flat[lastPhraseStart + i])
  return exact / flat.length >= 0.75 && lastPhraseHeard && heard.length <= flat.length * 1.3
}

/** Phrase timings from word timestamps; falls back to spreading phrases by length over the audio. */
function phraseTimes(section: Section, words: Word[], duration: number): Phrase[] {
  const perPhrase = section.phrases.map((p) => tokens(p.say))
  const flat = perPhrase.flat()
  const map = align(
    flat,
    words.map((w) => tokens(w.word).join('')),
  )
  const phrases: Phrase[] = []
  let k = 0
  for (const [pi, toks] of perPhrase.entries()) {
    const idx = toks.map((_, t) => map[k + t]).filter((x): x is number => x !== undefined)
    k += toks.length
    const show = section.phrases[pi].show
    if (idx.length > 0) phrases.push({ show, start: words[idx[0]].start, end: words[idx[idx.length - 1]].end })
    else phrases.push({ show, start: Number.NaN, end: Number.NaN })
  }
  if (phrases.some((p) => Number.isNaN(p.start))) {
    console.warn(`  ${section.id}: couldn't align every phrase, spreading them by length instead`)
    const total = flat.length
    let acc = 0
    return section.phrases.map((p, i) => {
      const start = (acc / total) * duration
      acc += perPhrase[i].length
      return { show: p.show, start, end: (acc / total) * duration }
    })
  }
  return phrases
}

/** Seconds of silence before the first sound, minus a 50 ms cushion so consonants keep their attack. */
function leadingSilence(path: string): number {
  const rate = 16000
  const rms = rmsWindows(decodeMono(path, rate), rate, 0.01)
  const first = rms.findIndex((v) => toDb(v) > -45)
  return Math.max(0, first * 0.01 - 0.05)
}

function envelope(path: string): number[] {
  const rate = 16000
  // Band-limit to the voice so breaths and sibilance don't flap the mouth.
  const rms = rmsWindows(decodeMono(path, rate, 'highpass=f=120,lowpass=f=3500'), rate, 1 / FPS)
  const peak = Math.max(...rms, 1e-6)
  return rms.map((v) => Math.round(Math.min(1, v / (peak * 0.7)) * 1000) / 1000)
}

async function main(): Promise<void> {
  mkdirSync(RAW, { recursive: true })
  mkdirSync(OUT, { recursive: true })
  const manifestPath = join(OUT, 'manifest.json')
  const previous: VoiceManifest | undefined = existsSync(manifestPath)
    ? JSON.parse(readFileSync(manifestPath, 'utf8'))
    : undefined
  const manifest: VoiceManifest = { fps: FPS, source, sections: previous?.source === source ? previous.sections : {} }

  for (const section of SCRIPT) {
    if (only && section.id !== only) continue
    console.log(`${section.id}:`)
    const hash = createHash('sha256')
      .update(JSON.stringify([section.phrases.map((p) => p.say), VOICE, VOICE_INSTRUCTIONS]))
      .digest('hex')
      .slice(0, 12)
    const rawPath = source === 'webcam' ? join(RAW, `webcam-${section.id}.wav`) : join(RAW, `${section.id}-${hash}.wav`)

    const file = `voice/${section.id}.wav`
    const outPath = join(ROOT, 'public', file)
    let words: Word[] = []
    let duration = 0
    let lead = 0
    for (let attempt = 1; ; attempt++) {
      if (source === 'webcam') {
        const take = join(ROOT, 'public/webcam', `${section.id}.mp4`)
        if (!existsSync(take)) throw new Error(`Missing webcam take ${take}`)
        execFileSync('ffmpeg', ['-v', 'error', '-y', '-i', take, '-vn', '-ac', '1', '-ar', '48000', rawPath])
      } else if (force || attempt > 1 || !existsSync(rawPath)) {
        console.log(`  generating speech (attempt ${attempt})`)
        await tts(section, rawPath)
      } else console.log('  speech cached')

      // Trim leading/trailing silence so timing is about the words, then loudness-normalize. The lead-in is cut
      // by a measured amount (not silenceremove) and recorded, so a webcam take's video can shift by the same.
      lead = leadingSilence(rawPath)
      execFileSync('ffmpeg', [
        '-v',
        'error',
        '-y',
        '-i',
        rawPath,
        '-af',
        [
          `atrim=start=${lead}`,
          'asetpts=PTS-STARTPTS',
          'areverse',
          'silenceremove=start_periods=1:start_threshold=-45dB:start_silence=0.08',
          'areverse',
          `loudnorm=I=${VOICE.lufs}:TP=-2:LRA=9`,
        ].join(','),
        '-ar',
        '48000',
        '-ac',
        '1',
        outPath,
      ])
      duration = durationSec(outPath)
      console.log(`  ${duration.toFixed(2)} s, transcribing for captions`)
      words = await transcribeWords(outPath)
      if (source === 'webcam' || takeIsComplete(section, words)) break
      console.warn(`  the take doesn't match the script: heard "${words.map((w) => w.word).join(' ')}"`)
      if (attempt >= 4) throw new Error(`${section.id}: no complete take after ${attempt} attempts`)
    }
    manifest.sections[section.id] = {
      file,
      duration,
      lead,
      phrases: phraseTimes(section, words, duration),
      envelope: envelope(outPath),
    }
  }
  writeFileSync(manifestPath, JSON.stringify(manifest))
  const total = Object.values(manifest.sections).reduce((s, v) => s + v.duration, 0)
  console.log(`Wrote ${manifestPath} (${total.toFixed(1)} s of voice in total)`)
}

await main()
