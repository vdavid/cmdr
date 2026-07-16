#!/usr/bin/env python3
"""Emit the CLIP tokenization reference the Rust tokenizer tests assert against.

Light step (transformers tokenizer only, no torch inference), run first so the Rust
side is unblocked while the heavy Core ML conversion runs. See README.md.
"""
import json
import os

from transformers import CLIPTokenizer

MODEL_ID = "openai/clip-vit-base-patch32"
CONTEXT_LENGTH = 77
BOS, EOS, PAD = 49406, 49407, 0

CASES = [
    "a photo of a cat",
    "a dog",
    "beach sunset",
    "a video game screenshot",
    "hello",
    "a red car on a street",
    "",
]


def main() -> None:
    tok = CLIPTokenizer.from_pretrained(MODEL_ID)
    out = []
    for text in CASES:
        # transformers pads/truncates to the model context length with the pad token
        # and inserts BOS/EOS; this is exactly the sequence CLIP feeds the text tower.
        enc = tok(
            text,
            padding="max_length",
            max_length=CONTEXT_LENGTH,
            truncation=True,
            return_tensors=None,
        )
        ids = list(enc["input_ids"])
        n_tokens = sum(1 for i in ids if i != PAD)
        out.append({"text": text, "ids": ids, "n_tokens": n_tokens})

    doc = {
        "model": MODEL_ID,
        "context_length": CONTEXT_LENGTH,
        "bos": BOS,
        "eos": EOS,
        "pad": PAD,
        "notes": (
            "CLIP text cleaning: NFC, lowercased, whitespace-collapsed, then byte-pair "
            "encoded. Sequence = [BOS] bpe-ids [EOS] then PAD(0) to context_length=77. "
            "Over-length input is truncated to 75 content tokens + BOS/EOS."
        ),
        "cases": out,
    }
    dest = os.path.join(os.path.dirname(__file__), "reference-tokenization.json")
    with open(dest, "w") as f:
        json.dump(doc, f, indent=2)
    print(f"wrote {dest} ({len(out)} cases)")


if __name__ == "__main__":
    main()
