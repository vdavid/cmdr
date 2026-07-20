# CLIP → Core ML conversion (dev-only, out-of-tree)

Produces the two Core ML towers Cmdr's semantic image search (media_index M3) downloads on demand: an **image tower**
(enrichment embeds every photo) and a **text tower** (a search query is encoded and cosine-matched against the stored
image embeddings).

This is a **one-off developer tool**. It is NEVER invoked by CI or `pnpm` — it runs by hand in a throwaway virtualenv to
regenerate the shippable artifacts when the model or its preprocessing changes.

## Model + license

- `openai/clip-vit-base-patch32` — OpenAI CLIP ViT-B/32, **MIT-licensed** weights (a commercial product can ship them;
  Apple's MobileCLIP is research-only and can't — see `docs/notes/clip-coreml-rust-spike.md`). Embedding dim 512, image
  224×224, text context 77.
- Verify the license from the model card before regenerating: it must stay MIT (or another commercial-OK license).
  Record what you found in the commit.

## The pins are frozen on purpose

`requirements.txt` records the versions that produced the artifacts currently hosted on Hugging Face and pinned by
SHA-256 in `media_index/clip/install.rs`. It's provenance, not a dependency set to keep current, so Renovate is disabled
on it (`renovate.json`) and a routine bump is drift rather than maintenance. Note that `coremltools` declares no `torch`
/ `transformers` bound at all, so resolving cleanly proves nothing: `uv pip compile` will happily give you
`transformers` 5.x, which this script's `from transformers import CLIPModel` predates.

Moving the model is a deliberate act: bump the pins, re-run both steps below, and confirm the fidelity cosines in the
regenerated `reference-vectors.json` still match the checked-in ones before uploading. That comparison is the only real
validation. A cosine regression is a failed bump, not a new baseline.

## Run

Requires Python 3.11 or 3.12 (coremltools/torch have no cp314 wheels as of 2026-07). With
[`uv`](https://docs.astral.sh/uv/) this needs no system Python:

```sh
cd apps/desktop/scripts/convert-clip-model
uv venv --python 3.12 .venv
uv pip install --python .venv -r requirements.txt

# 1) Emit the tokenization reference the Rust tokenizer tests assert against (fast):
.venv/bin/python emit_reference.py      # writes reference-tokenization.json

# 2) Convert both towers, palettize, zip, and print sizes + SHA-256 (slow, downloads weights):
.venv/bin/python convert.py             # writes dist/*.mlpackage(.zip) + reference-vectors.json
```

`convert.py` bakes CLIP's per-channel `(x-mean)/std` normalization INTO the image model, so the Rust side only resizes +
center-crops to 224×224, divides RGB by 255, and packs a CHW float `[1,3,224,224]` tensor. Both towers take an
`MLMultiArray` (the path the Rust spike proved), avoiding the Core ML `ImageType` / CVPixelBuffer FFI surface. Weights
are fp16 then 8-bit palettized (`PALETTIZE_NBITS`; drop to 4 to shrink further, at some quality cost — measure the
fidelity cosine). Embeddings are the **raw** projected 512-d features; the Rust cosine normalizes.

The app ships the `.mlpackage` and compiles it to `.mlmodelc` on-device at first run (`.mlmodelc` is OS-version-specific
— never bundle a prebuilt one).

## Outputs

- `dist/clip-image.mlpackage`, `dist/clip-text.mlpackage` — the towers (gitignored).
- `dist/clip-image.mlpackage.zip`, `dist/clip-text.mlpackage.zip` — the distributables, with their SHA-256 and byte
  sizes printed at the end.
- `reference-tokenization.json` — token-id reference (checked in; backs the Rust tests).
- `reference-vectors.json` — fidelity cosines + reference output vectors (checked in).

## Upload (with David's explicit approval)

The shipped artifacts live in the public Hugging Face repo `veszelovszki/cmdr-clip-vit-b32-coreml` (uploaded with the
`hf` CLI; token via `secret HF_TOKEN`). `convert.py` prints an upload-handoff line per artifact:

> David: upload `<artifact>` (sha256 `<hash>`, `<size>` bytes) to the model host; confirm the pinned URL in
> `media_index/clip/install.rs` returns those exact bytes.

The app pins the URL + SHA-256 in `media_index/clip/install.rs` (`ClipTowerSpec`). The download is checksum-verified
before install, so the bytes at the URL must match the printed hash exactly. Whatever the host, the URL must return the
exact bytes the checked-in SHA-256 pins — after re-converting, update both the hashes and (if the host changes) the URLs
there.
