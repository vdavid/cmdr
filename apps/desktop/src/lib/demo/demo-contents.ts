/**
 * Real bytes for the demo's text-like files, so the viewer has something to show. Every other file
 * reads bytes the engine generates from its path. A file's bytes decide its size, whatever
 * `demo-tree.ts` lists. PLACEHOLDER copy: David reviews it with the tree (issue #302).
 */
import { DEMO_HOME } from './demo-tree.js'

const H = DEMO_HOME

const TEXTS: Readonly<Record<string, string>> = {
  [`${H}/.zshrc`]: `# Prompt and history
export HISTSIZE=50000
setopt share_history
alias ll='ls -la'
`,
  [`${H}/.gitconfig`]: `[user]
\tname = You
\temail = you@example.com
[init]
\tdefaultBranch = main
`,
  [`${H}/notes.md`]: `# Notes

Things to try in Cmdr:

- Press **F3** on a file to view it, and **Esc** to close the viewer.
- In the viewer, press **1**, **2**, or **3** for text, binary, or hex.
- Press **⌘F** to search, then **Enter** for the next match.

My recipes live in [Recipe ideas](Documents/Recipe%20ideas.md), and the trip plan in
[the itinerary](Documents/Travel/Itinerary.md).
`,
  [`${H}/Desktop/todo.txt`]: `- [x] Book the train
- [ ] Renew the passport
- [ ] Water the plants
- [ ] Call the landlord about the heating
`,
  [`${H}/Documents/Recipe ideas.md`]: `# Recipe ideas

## Weeknight pasta

1. Boil the pasta.
2. Fry garlic in olive oil, add chili and lemon zest.
3. Toss, then finish with parsley and parmesan.

## Overnight oats

Oats, milk, a spoon of yogurt, and berries. Into the fridge, done by morning.
`,
  [`${H}/Documents/旅行の計画.txt`]: `旅行の計画

一日目: 東京に到着、浅草を散歩
二日目: 京都へ移動、清水寺
三日目: 奈良で鹿に会う
`,
  [`${H}/Documents/Travel/Itinerary.md`]: `# Itinerary

| Day | Where | What |
| --- | ----- | ---- |
| 1 | Oslo | Arrive, walk the harbor |
| 2 | Bergen | Train over the mountains |
| 3 | Fjords | Ferry day |

Back to [the notes](../../notes.md).
`,
  [`${H}/Projects/website/README.md`]: `# Website

A tiny static site. The page is [index.html](index.html), the look is [styles.css](styles.css).

\`\`\`sh
python3 -m http.server 8080
\`\`\`
`,
  [`${H}/Projects/website/index.html`]: `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Hello</title>
    <link rel="stylesheet" href="styles.css" />
  </head>
  <body>
    <h1>Hello, world</h1>
  </body>
</html>
`,
  [`${H}/Projects/website/styles.css`]: `body {
  margin: 0;
  font-family: system-ui, sans-serif;
}

h1 {
  text-align: center;
}
`,
  [`${H}/Projects/rust-cli/Cargo.toml`]: `[package]
name = "rust-cli"
version = "0.1.0"
edition = "2024"

[dependencies]
`,
  [`${H}/Projects/rust-cli/main.rs`]: `use std::env;

fn main() {
    let name = env::args().nth(1).unwrap_or_else(|| "world".to_string());
    println!("Hello, {name}!");
}
`,
}

/** Every file with real bytes, by path, as the engine takes them. */
export function demoContents(): Record<string, Uint8Array> {
  const encoder = new TextEncoder()
  return Object.fromEntries(Object.entries(TEXTS).map(([path, text]) => [path, encoder.encode(text)]))
}
