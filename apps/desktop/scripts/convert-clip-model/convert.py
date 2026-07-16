#!/usr/bin/env python3
"""Convert OpenAI CLIP ViT-B/32 (MIT) to two Core ML `.mlpackage` towers for Cmdr.

Out-of-tree dev tool — NEVER run by CI or pnpm. See README.md. Produces:
  dist/clip-image.mlpackage  — image tower (baked CLIP preprocessing; 224x224 RGB in)
  dist/clip-text.mlpackage   — text tower (int32 [1,77] token ids in)
  dist/clip-image.mlpackage.zip / clip-text.mlpackage.zip + printed SHA-256 + sizes
  reference-vectors.json     — fidelity cosines + reference output vectors for tests

Preprocessing is baked into the image model: Core ML takes an ImageType (0-255 RGB),
scales by 1/255, and the traced module applies CLIP's per-channel (x-mean)/std. So the
Rust side only resizes/center-crops to 224 and hands raw pixels. Embeddings are the raw
projected 512-d features (NOT L2-normalized in-model; the Rust cosine normalizes).
"""
import hashlib
import json
import os
import shutil
import zipfile

import numpy as np
import torch
import torch.nn as nn
from transformers import CLIPModel

import coremltools as ct
import coremltools.optimize.coreml as cto

MODEL_ID = "openai/clip-vit-base-patch32"
HERE = os.path.dirname(os.path.abspath(__file__))
DIST = os.path.join(HERE, "dist")
CONTEXT_LENGTH = 77
EMBED_DIM = 512
# 8-bit k-means palettization shrinks the towers to ~138 MB but produced NaN inference on
# the text tower (verified 2026-07-16), so it's OFF by default (correct fp16, ~larger). Set
# CLIP_PALETTIZE_NBITS=8 (or 4) to re-enable once a per-layer exclusion is worked out.
PALETTIZE_NBITS = int(os.environ.get("CLIP_PALETTIZE_NBITS", "0"))

# CLIP image normalization (the processor's image_mean / image_std).
CLIP_MEAN = [0.48145466, 0.4578275, 0.40821073]
CLIP_STD = [0.26862954, 0.26130258, 0.27577711]

PROMPTS = [
    "a photo of a cat",
    "a dog",
    "beach sunset",
    "a video game screenshot",
    "hello",
    "a red car on a street",
]


class ImageTower(nn.Module):
    """CLIP image features with normalization baked in. Input: [1,3,224,224] in [0,1]."""

    def __init__(self, clip: CLIPModel):
        super().__init__()
        self.clip = clip
        self.register_buffer("mean", torch.tensor(CLIP_MEAN).view(1, 3, 1, 1))
        self.register_buffer("std", torch.tensor(CLIP_STD).view(1, 3, 1, 1))

    def forward(self, x):
        x = (x - self.mean) / self.std
        return self.clip.get_image_features(pixel_values=x)


class TextTower(nn.Module):
    """CLIP text features. Input: int32 [1,77] token ids (BOS .. EOS .. pad)."""

    def __init__(self, clip: CLIPModel):
        super().__init__()
        self.clip = clip

    def forward(self, input_ids):
        return self.clip.get_text_features(input_ids=input_ids)


def convert_tower(traced, inputs, out_path):
    mlmodel = ct.convert(
        traced,
        inputs=inputs,
        outputs=[ct.TensorType(name="embedding")],
        minimum_deployment_target=ct.target.macOS14,
        # FLOAT32 compute: casting to FLOAT16 overflows a few CLIP weights to inf, which
        # then breaks k-means palettization ("Input X contains infinity"). Palettization
        # (below) drives the final size regardless of compute precision.
        compute_precision=ct.precision.FLOAT32,
        convert_to="mlprogram",
    )
    if PALETTIZE_NBITS:
        cfg = cto.OptimizationConfig(
            global_config=cto.OpPalettizerConfig(nbits=PALETTIZE_NBITS, mode="kmeans")
        )
        mlmodel = cto.palettize_weights(mlmodel, cfg)
    if os.path.exists(out_path):
        shutil.rmtree(out_path)
    mlmodel.save(out_path)
    return mlmodel


def dir_size(path):
    total = 0
    for root, _dirs, files in os.walk(path):
        for name in files:
            total += os.path.getsize(os.path.join(root, name))
    return total


def zip_package(pkg_path):
    zip_path = pkg_path + ".zip"
    base = os.path.dirname(pkg_path)
    if os.path.exists(zip_path):
        os.remove(zip_path)
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
        for root, _dirs, files in os.walk(pkg_path):
            for name in files:
                full = os.path.join(root, name)
                zf.write(full, os.path.relpath(full, base))
    h = hashlib.sha256()
    with open(zip_path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return zip_path, os.path.getsize(zip_path), h.hexdigest()


def cosine(a, b):
    a = np.asarray(a).ravel()
    b = np.asarray(b).ravel()
    return float(np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b)))


def main():
    os.makedirs(DIST, exist_ok=True)
    print(f"Loading {MODEL_ID} …")
    clip = CLIPModel.from_pretrained(MODEL_ID).eval()
    from transformers import CLIPTokenizer

    tok = CLIPTokenizer.from_pretrained(MODEL_ID)

    # ── Image tower ───────────────────────────────────────────────────────────
    print("Tracing + converting image tower …")
    img_mod = ImageTower(clip).eval()
    ex_img = torch.rand(1, 3, 224, 224)
    traced_img = torch.jit.trace(img_mod, ex_img)
    img_path = os.path.join(DIST, "clip-image.mlpackage")
    # A float TensorType input in [0,1] (Rust resizes/center-crops to 224 and divides RGB
    # by 255, packing CHW). The CLIP per-channel (x-mean)/std normalization is baked into
    # the traced module, so this is still "preprocessing in the model" — we just feed an
    # MLMultiArray (the exact path the Rust spike proved for the text tower) instead of a
    # Core ML ImageType, which avoids the CVPixelBuffer / MLImageConstraint FFI surface.
    img_ml = convert_tower(
        traced_img,
        [ct.TensorType(name="image", shape=(1, 3, 224, 224), dtype=np.float32)],
        img_path,
    )

    # ── Text tower ────────────────────────────────────────────────────────────
    print("Tracing + converting text tower …")
    txt_mod = TextTower(clip).eval()
    ex_ids = torch.zeros(1, CONTEXT_LENGTH, dtype=torch.int32)
    ex_ids[0, 0] = 49406
    ex_ids[0, 1] = 49407
    traced_txt = torch.jit.trace(txt_mod, ex_ids)
    txt_path = os.path.join(DIST, "clip-text.mlpackage")
    txt_ml = convert_tower(
        traced_txt,
        [ct.TensorType(name="input_ids", shape=(1, CONTEXT_LENGTH), dtype=np.int32)],
        txt_path,
    )

    # ── Fidelity + reference vectors (text tower; ANE via Core ML predict) ─────
    print("Measuring text-tower fidelity …")
    ref = {"model": MODEL_ID, "palettize_nbits": PALETTIZE_NBITS, "text_cases": []}
    cosines = []
    with torch.no_grad():
        for p in PROMPTS:
            ids = tok(p, padding="max_length", max_length=CONTEXT_LENGTH, truncation=True, return_tensors="pt")["input_ids"].to(torch.int32)
            torch_vec = txt_mod(ids).numpy().ravel()
            try:
                cm = txt_ml.predict({"input_ids": ids.numpy().astype(np.int32)})
                cm_vec = np.asarray(next(iter(cm.values()))).ravel()
                c = cosine(torch_vec, cm_vec)
                cosines.append(c)
                ref["text_cases"].append({
                    "text": p,
                    "ids": ids.numpy().ravel().tolist(),
                    "coreml_first8": cm_vec[:8].tolist(),
                    "coreml_l2": float(np.linalg.norm(cm_vec)),
                    "cosine_vs_torch": c,
                })
            except Exception as e:  # predict only works on macOS with a compiled model
                ref["text_cases"].append({"text": p, "ids": ids.numpy().ravel().tolist(), "predict_error": str(e)})
    if cosines:
        ref["fidelity"] = {"min_cosine": min(cosines), "mean_cosine": sum(cosines) / len(cosines)}

    with open(os.path.join(HERE, "reference-vectors.json"), "w") as f:
        json.dump(ref, f, indent=2)

    # ── Zip + checksum ────────────────────────────────────────────────────────
    print("Zipping + hashing …")
    results = []
    for path, ml in [(img_path, img_ml), (txt_path, txt_ml)]:
        pkg_bytes = dir_size(path)
        zip_path, zip_bytes, sha = zip_package(path)
        results.append((os.path.basename(zip_path), pkg_bytes, zip_bytes, sha))

    print("\n=== ARTIFACTS ===")
    for name, pkg_bytes, zip_bytes, sha in results:
        print(f"{name}: pkg={pkg_bytes/1e6:.1f} MB  zip={zip_bytes/1e6:.1f} MB  sha256={sha}")
    if cosines:
        print(f"fidelity: min_cosine={min(cosines):.4f} mean_cosine={sum(cosines)/len(cosines):.4f}")
    print("\n=== DAVID UPLOAD HANDOFF ===")
    for name, _pkg, zip_bytes, sha in results:
        print(f"David: upload {name} (sha256 {sha}, {zip_bytes} bytes) to https://models.getcmdr.com/{name}; confirm the URL returns those exact bytes.")


if __name__ == "__main__":
    main()
