# User research: job stories

Last updated: 2026-02-18

## Who we're building for at launch

**Primary segment:** Developers and power users on macOS who manage files constantly and have outgrown Finder. They
likely used Norton Commander, Total Commander, Far Manager, or Midnight Commander at some point. They prefer keyboard
over mouse. They value speed and reliability over flashy features.

**Adjacent segment (fast follow):** The same crowd, plus AI-curious developers who want to see what happens when file
management gets intelligent. They'll come for the AI, stay for the speed.

## Launch job stories (core file manager)

### Navigation and orientation

**When** I land in a large project directory with hundreds of files, **I want to** see what's here instantly and navigate
to the right file in a few keystrokes, **so I can** stay in flow instead of scrolling or waiting for Finder to load.

**When** I'm working across two related directories (for example, source and build output, or local and remote), **I want to**
see both side by side and move files between them, **so I can** stop juggling multiple Finder windows that overlap and
lose focus.

**When** I'm deep in a nested folder structure, **I want to** jump back to a recent location or bookmark without
retracing my steps, **so I can** avoid the tedious click-click-click back up through parent folders.

**When** I have multiple projects or contexts I switch between throughout the day, **I want to** keep them as separate
tabs that persist, **so I can** context-switch without losing my place.

### File operations

**When** I need to copy or move a batch of files between directories, **I want to** select them, hit a key, and see
them transfer with clear progress, **so I can** get on with my work and intervene if something goes wrong.

**When** I need to delete files or folders, **I want to** do it with a keystroke and see a confirmation that tells me
exactly what I'm about to remove, **so I can** act fast without the anxiety of accidentally nuking something important.

**When** I use Cmd+C / Cmd+V in my daily workflow, **I want to** copy and paste files the way I copy and paste
everything else on macOS, **so I can** not have to learn a separate mental model for the file manager.

**When** a file operation fails partway through (for example, permissions error on file 40 of 200), **I want to** see which
files succeeded and which didn't, and decide what to do next, **so I can** recover gracefully instead of wondering what
state things are in.

### Speed and reliability

**When** I open a directory with thousands of files (node_modules, a photo library, a monorepo root), **I want to** see
results immediately, even before everything has loaded, **so I can** start working without waiting for a progress
spinner to finish.

**When** I connect to a remote server or NAS over SMB / SFTP, **I want to** browse it as fast and reliably as a local
folder, **so I can** stop dreading the "connecting…" lag and the random disconnects I get with Finder.

**When** I'm in the middle of a long copy and realize I selected the wrong files, **I want to** cancel instantly and
have it actually stop, **so I can** not waste time and bandwidth on an operation I didn't mean to start.

### Discovery and trust

**When** I first open Cmdr, **I want to** feel oriented within seconds — see the basics, understand the layout, know
how to do the three things I do most, **so I can** decide quickly whether this is worth keeping.

**When** I don't know the shortcut for something, **I want to** find it through a command palette or contextual hints,
**so I can** keep getting faster without memorizing a manual first.

**When** Cmdr asks me to pay, **I want to** understand exactly what I'm paying for and why it's worth it, **so I can**
feel good about supporting the tool, not tricked into a subscription.

## Fast-follow job stories (AI features)

These target real pain points. Not "AI for AI's sake" — things that are genuinely tedious without intelligence.

### Batch renaming with context

**When** I have a folder of files with inconsistent names (screenshots, downloads, exports from different tools), **I
want to** describe the naming pattern I want in plain language and preview the renames before they happen, **so I can**
bring order to chaos without writing regex or using a separate batch rename tool.

**When** I receive a handoff of assets from a designer or client (like "final_v2_FINAL(1).psd"), **I want to** clean up
the names intelligently based on file content or metadata, **so I can** actually find things later.

### Finding files by intent, not filename

**When** I know I worked on something last week but don't remember what I named it or where I put it, **I want to**
describe what it was about and have Cmdr find likely matches, **so I can** stop the frustrating folder-by-folder hunt.

**When** I'm looking for a specific document in a large project folder (for example, "the API spec for the payments
integration"), **I want to** search by meaning rather than filename, **so I can** find it even if it's named
`spec-v3-draft.md` and buried three levels deep.

### Understanding unfamiliar directories

**When** I clone a new repo or receive a project handoff, **I want to** get a quick summary of what's in this directory
and how it's organized, **so I can** orient myself without opening 20 files to figure out the structure.

**When** I open an old project I haven't touched in months, **I want to** quickly see what I was working on and what
changed recently, **so I can** pick up where I left off without archaeology.

### Safe, informed cleanup

**When** my disk is getting full and I need to reclaim space, **I want to** see what's eating storage and get help
understanding which large files or folders are safe to remove, **so I can** free up space confidently instead of
guessing whether `~/Library/Caches/some-hash/` is important.

**When** I have near-duplicate files scattered across folders (same photo at different resolutions, same doc with minor
edits), **I want to** find and review them in one place, **so I can** decide what to keep without comparing files
manually.

### Automating repetitive file workflows

**When** I regularly move files from one place to another in a specific pattern (for example, moving processed invoices to a
`done/YYYY-MM/` folder), **I want to** teach Cmdr the pattern once and have it suggest or automate it going forward,
**so I can** stop doing the same tedious drag-and-drop every week.

## What's NOT a job story (yet)

These came up in brainstorming but aren't validated pain points for our primary segment:

- **"Organize my downloads by type/date"** — Sounds impressive in a demo, rarely matches how people actually think about
  their files. Real organization is project-based, not category-based.
- **"AI-powered file tagging"** — Useful in theory, but nobody maintains tags. The effort-to-value ratio is wrong.
- **"Natural language terminal"** — Our users already know the terminal. They don't need AI to run `ls`.
- **"Smart folders / auto-sorting rules"** — macOS already has this. Not a differentiator.

## Open questions

1. **Remote file access scope for launch:** SMB and SFTP are partially built. Should we ship both, or focus on one?
   How many of our target users regularly access remote filesystems?
2. **AI feature pricing:** Should AI be the subscription justification (core is free/one-time, AI is subscription)?
   Or is it all bundled? This affects how we position the product at launch.
3. **First AI feature to ship:** Batch renaming feels highest impact-to-effort. Semantic search is flashier but harder.
   Which one should go first?
