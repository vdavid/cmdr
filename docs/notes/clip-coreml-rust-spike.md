# Spike: CLIP text→image via Core ML, called from Rust

Backs the `docs/specs/later/media-ml-index-plan.md` M3 decision. Run 2026-06-30 on Apple Silicon, macOS 26.5.1 (arm64),
Xcode 26.5, coremltools 9.0. Independently license-verified against Apple's own `LICENSE_MODELS`.

## Verdict

The macOS-native **CLIP-text-encoder-via-Core-ML, called from Rust** path is **technically proven end-to-end** — a
minimal `objc2-core-ml` Rust spike loaded a compiled model, ran a prediction, and returned an embedding
**bit-identical** to the Python `coremltools` reference. Native Core ML adds **zero binary weight** (system framework).

**But Apple's MobileCLIP/MobileCLIP2 weights are NOT shippable in Cmdr.** Per Apple's `ml-mobileclip` `LICENSE_MODELS`,
the weights are under the **Apple ML Research Model Terms of Use** (code is MIT, data CC-BY-NC-ND) — research-only,
excluding commercial product use. The pre-converted Core ML towers (`apple/coreml-mobileclip`) carry the same
restriction. Cmdr is a commercial product, so it cannot ship these weights.

**Resolution (no architecture change):** keep the proven, **model-agnostic** Core ML + `objc2-core-ml` plumbing; swap
the weights for a **commercially-licensed CLIP** — OpenAI CLIP (MIT) or SigLIP 2 (Apache-2.0) — convert once with
`coremltools` on a dev box, ship the pre-converted `.mlpackage` (image + text towers). Trade-off: those models are
heavier than MobileCLIP-S0 (MobileCLIP's efficiency edge is lost to licensing), but still run fine on-device via Core ML
on the ANE. Verify the specific chosen model's license + conversion fidelity at impl time.

## What was proven (Goal-by-goal)

**A — model + text encoder exist.** `apple/coreml-mobileclip` ships separate image + text `.mlpackage` towers
(S0/S1/S2/B/B-LT), embedding dim 512, image input 256×256 RGB, text input int32 `[1,77]` (CLIP BPE, ctx 77). Inference
needs only `coremltools + numpy + pillow + transformers` (~164 MB venv, no torch). Used here to prove the mechanism;
**not shippable** (license). Same license on `apple/MobileCLIP2-*`.

**B — text→image alignment works on-device** (Core ML text tower, ANE, `MLComputeUnits::All`, S0, 12 benign local test
images + 12 prompts, cosine). Correct top-1: "a child" → child (0.141); "a person on a scooter" → scooter (0.210);
"seagulls flying" → seagulls (0.130); "a portrait of a woman" → portrait (0.170); "a video game screenshot" → game
screenshot (0.274); "a duolingo streak" → that screenshot (0.383); "a screenshot of statistics" → all 3 stats
screenshots in the top-3. Misses (beach/dog/mountain/food) were correct null results — the set has no such subject.
Latency (S0, warm, incl. Python/IPC): image 1.5 ms, text 2.1 ms per encode. Cold model load ~1 s (S0) / ~2.3 s (S2).

**C — Rust FFI proven.** Crates (crates.io-verified): `objc2-core-ml` 0.3.2, `objc2` 0.6.4, `objc2-foundation` 0.3.2. It
exposes everything: `MLModel` (`modelWithContentsOfURL_configuration_error`, `predictionFromFeatures_error`),
`MLModelConfiguration` (`setComputeUnits`), `MLMultiArray` (`initWithShape_dataType_error`, `dataPointer`, `count`),
`MLFeatureValue`, `MLDictionaryFeatureProvider`, `MLFeatureProvider`. The spike built an int32 `[1,77]` `MLMultiArray`,
ran one prediction, read `[1,512]` into `Vec<f32>` — bit-identical to the coremltools reference (L2 9.7594; first6
0.13543 / −0.21844 / 0.41546 …). **Unsafe surface ~12–15 mechanical objc2 calls**, wrappable in ~150–250 lines behind a
safe `encode_text(&[i32;77]) -> Vec<f32>` / `encode_image(pixels) -> Vec<f32>`. Real concerns: the raw `dataPointer`
read/write (trust the shape) and serializing/pooling `MLModel.prediction` calls.

**Compiled-model wrinkle:** Core ML wants a compiled `.mlmodelc` (an `.mlpackage` compiles at load or via
`MLModel.compileModel(at:)` / `xcrun coremlcompiler`). `.mlmodelc` is OS-version-specific, so **compile on-device at
first run and cache** — ship the `.mlpackage`, don't bundle a prebuilt `.mlmodelc`.

**`ort` + ONNX fallback cost:** `ort` runs ONNX, so it means shipping `libonnxruntime.dylib` (~20–30 MB) + ONNX
artifacts, and `ort` is still pre-1.0 (2.0.0-rc.12) — **~25–35 MB of native binary the native Core ML path avoids.**
Reserve `ort` for a hypothetical non-Apple platform only.

## Side finding (privacy)

The test folder `/Volumes/naspi/_todo_pics` mixes **sensitive ID scans (passport, driver's license, Covid test)** with
ordinary photos. The image index will OCR/tag/detect-faces on such documents. On-device default keeps that local (fine),
but it sharpens the M5 cloud-caption egress consent and argues for sensitive-document awareness — see the plan's privacy
cross-cutting.
