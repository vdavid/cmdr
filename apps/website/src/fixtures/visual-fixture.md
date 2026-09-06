---
title: Blog rendering fixture
description: Every custom markdown feature the blog pipeline can render, on one short page.
date: '2026-01-01'
---

This page is the visual-regression fixture. It exercises every custom transform in the markdown pipeline deliberately,
so the committed baselines change when the _machinery_ changes and stay still when blog copy does. Nothing here is
editorial: don't reword it to read better, and don't add real content. Every block below is load-bearing for a plugin or
a prose style.

Smartypants: "curly quotes", an apostrophe's turn, and an ellipsis... A link to
[the Rust book](https://doc.rust-lang.org/book/) renders the external-link arrow.

## Inline icons

The `:name:` tokens from `src/plugins/blog-icons.ts`, one of each registered glyph:

| State       | Glyph  |
| ----------- | ------ |
| Supported   | :yes:  |
| Unsupported | :no:   |
| Caveat      | :warn: |
| Planned     | :soon: |

## Code

Shiki renders dual-theme, so this block carries both palettes at once:

```rust
fn main() {
    let total: u64 = (1..=9).sum();
    println!("{total}");
}
```

## Download dropdown

The `cmdr:download` marker becomes the arch-aware menu: [download](cmdr:download)

## Theme image

One reference, two files, swapped by the site theme:

![Theme-aware fixture panel](/fixtures/panel-{theme}.png 'Theme panel')

## Figure row

Two images in one paragraph become a side-by-side row:

![Before panel](/fixtures/panel-before.png 'Before') ![After panel](/fixtures/panel-light.png 'After')

## Comparison slider

Two images plus the `[slider]` token become the draggable before/after slider, resting at 50%:

![Before panel](/fixtures/panel-before.png 'Before') ![After panel](/fixtures/panel-{theme}.png 'After') [slider]
