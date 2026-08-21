# The 643 MB `MALLOC_LARGE` is on-device model weights (2026-08-21)

**What this settles:** the largest unattributed block in Cmdr's idle profile is Core ML holding the two CLIP towers.
Measured: **307–412 MB of `MALLOC_LARGE` plus 120–176 MB of `MALLOC_SMALL`, held for the process lifetime**, paid the
moment anything encodes once, and **80% of it is the TEXT tower**, which an enrichment pass never calls.

**What it does not settle:** whether the CLIP worker was alive in the specific prod run that produced the 643 MB, and
what the 230–340 MB residual is. § "One command settles it" is the discriminator, and it is one command.

Prior art you should read first, in this order: `idle-memory-profile-2026-07-28.md` (where the number comes from),
`idle-cpu-attribution-2026-08-03.md` § "Still open" (the corrected attribution and the four wrong answers before it),
`docs/tooling/memory-debugging.md` (the recipes and the `IOAccelerator` trap).

## Why it stayed anonymous for three investigations

Cmdr has three memory accountants and, until now, no reader that spanned them.

- `query_mimalloc_heap` sees the Rust heap, which is everything Rust allocates and nothing else.
- `query_system_malloc_zones` sees the registered macOS zones, which mimalloc never joins.
- **Core ML allocates through the SYSTEM allocator**, so a tower's weights are invisible to the first and lumped into an
  unnamed total by the second.

So the bytes were real, large, steady, and nameless. Every tool anyone reached for could see the size and not the shape.

`cmdr_fs::process_memory::query_vm_regions` closes it: it walks the task's own VM map, folds it by `user_tag` (the rows
`vmmap -summary` prints), and reports a histogram of distinct REGION SIZES per tag. That last part is what matters:
macOS gives every allocation past its 127 KB large-zone threshold a region sized to the request, so a repeated exact
size is a fingerprint of whatever asked for those bytes. `get_memory_diagnostics` exposes the whole thing over IPC,
against a running app, release build included.

## The measurement

M1 Max, macOS 26.5, debug build, `cargo nextest`, 2026-08-21, by `clip::macos::residency_test`. A fresh process; load
both towers from the shipped pinned model; one image encode and one text encode; read the VM map before and after.

Under the shipped `MLComputeUnits::All`:

```
MALLOC_LARGE   0 -> 310,444,032   (66 regions)   [runs range 307,003,392 - 411,598,848]
MALLOC_SMALL   0 -> 125,059,072                  [runs range 119,537,664 - 176,177,152]

  101,187,584 bytes x  1     the text tower's 49,408 x 512 fp32 token embedding
    4,194,304 bytes x 24     text-tower MLP matrices, 512 x 2048 fp32, two per block
    3,145,728 bytes x 14     text-tower fused QKV projections, 512 x 1536 fp32, one per block
    2,359,296 bytes x 25     image-tower MLP matrices, 768 x 3072 at the shipped 8-bit palettization
    3,440,640 bytes x  1     scratch
    2,129,920 bytes x  1     scratch
```

Those groups account for **every byte**:
`101,187,584 + 24 × 4,194,304 + 25 × 2,359,296 + 14 × 3,145,728 + 3,440,640 + 2,129,920 = 310,444,032`, to the byte.
That exactness is the evidence. The regions are not "about the right size", they are the model's weight matrices one
malloc each.

The 307–412 MB spread across runs is entirely Core ML keeping one or two copies of the 101 MB token embedding. Nothing
else moves.

### Which tower, measured rather than inferred

`CMDR_CLIP_TOWER=image|text` loads one tower alone, and the two halves split cleanly:

| Loaded               | `MALLOC_LARGE` | `MALLOC_SMALL` | Regions                                                    |
| -------------------- | -------------- | -------------- | ---------------------------------------------------------- |
| Image tower alone    | 64.6 MB        | 65.5 MB        | 27, all of the `2,359,296` group                           |
| **Text tower alone** | **251.5 MB**   | 84.8 MB        | 41: the `101,187,584`, `4,194,304`, and `3,145,728` groups |
| Both                 | 310.4 MB       | 125.1 MB       | 66                                                         |

**The text tower is about 80% of the bill**, and it is the one whose only job is encoding a search query the user types.
The image tower, the one enrichment actually runs in a loop, is the cheap half because it ships 8-bit palettized.

### It is permanent by construction

`macos.rs` holds `WORKER: OnceLock<ClipWorker>`. The first `encode_text` (a typed search query) or `encode_image` (an
enrichment pass) spawns the worker, which loads BOTH towers and then lives for the process. Nothing drops them: not
finishing enrichment, not the user turning semantic search off afterwards, not ten hours of idling. That is precisely
the shape the idle profile was looking for: a steady-state cost rather than a runaway.

### The compute-unit assignment moves it 35×

Same measurement, `CMDR_CLIP_COMPUTE_UNITS` switching the assignment:

| Compute units        | `MALLOC_LARGE` | Regions | Shape                                        |
| -------------------- | -------------- | ------- | -------------------------------------------- |
| `All` (shipped)      | 307–412 MB     | 65–67   | every weight matrix, one malloc each         |
| `CPUAndGPU`          | 409 MB         | 66      | identical to `All`                           |
| `CPUOnly`            | **11.8 MB**    | **2**   | 9,437,184 and 2,359,296: two scratch buffers |
| `CPUAndNeuralEngine` | **11.8 MB**    | **2**   | same two                                     |

**The GPU path is the whole bill.** Without it Core ML leaves the weights in the mmap'd `weight.bin` and allocates two
working buffers; with it, every matrix gets its own copy.

⚠️ Note where the idle profile's **9 MB** came from: `9,437,184` is one of those two scratch buffers, and `2,359,296` is
both the other scratch buffer AND the image tower's palettized MLP matrix. Both of the sizes that note reported exist in
this system. ❌ But do not read that as proof the prod process was on the CPU path: a scratch buffer appears once, and
the profile described many.

### Vision is not the answer, and that is worth knowing

`vision::tests::what_one_vision_analyze_leaves_resident`, same machine and date: a first Vision analyze (OCR plus
classification plus feature print) grows total dirty by ~49 MB, of which **2.1 MB** is `MALLOC_LARGE`. Apple runs those
models largely out of process, so Vision is the wrong suspect when attributing a large in-process block.

## What this does and does not explain

- **Explains**: several hundred MB of `MALLOC_LARGE`, held forever, not SQLite, not the Rust heap, invisible to both
  allocator APIs, present exactly when the media features have been used. Every property the open question listed.
- **Does not explain**: the residual. 643 minus 307–412 leaves 230–340 MB unattributed, and this note makes no claim
  about it. The same region histogram is how to go after it: whichever exact size repeats most in what's left.
- **Does not establish**: that the CLIP worker was alive in the profiled run at all. The measurement proves the towers
  cost this; it does not prove they were loaded on that machine that day.

⚠️ Every number here comes from a DEBUG build on an M1 Max with no window server, encoding once. A shipped release build
on David's laptop, doing thousands of image encodes over ten hours, is a different process. The weight residency should
not vary with any of that (it is a constant of the model and the compute-unit assignment), but "should not" is a
prediction, and this repo has a note about what those cost.

## One command settles it

Against the live app on the machine that produced the 643 MB, with no app support needed, so a shipped release build
answers it too:

```sh
vmmap $(pgrep -x Cmdr) | awk '$1 == "MALLOC_LARGE" { print $4 }' | sort | uniq -c | sort -rn | head -12
```

That prints `MALLOC_LARGE` region sizes with their counts, in `vmmap`'s own units. Read the top of the list:

- **A `96.5M` region present** → the CLIP text tower is loaded, and this note's attribution holds. `96.5M` is
  101,187,584 bytes, the `49,408 × 512` fp32 token embedding, and nothing else in the process is that size: a signature,
  not an inference. Expect `4096K`, `3072K`, and `2304K` in the dozens beside it.
- **No `96.5M`** → the towers were never loaded, this attribution is wrong for that run, and whichever exact size
  repeats most IS the next lead. Feed it back into this method.

In a dev build, `get_memory_diagnostics` returns the same histogram as structured data along with both allocators'
accounting, which is more useful for anything past this one question.

Two cheaper corroborations, in case the app has since restarted: does
`~/Library/Application Support/com.veszelovszki.cmdr/clip-model` exist, and is semantic search on? If the model was
never installed, the towers could not have loaded, and this note does not apply to that machine at all.

## Recommended next steps, in order, none of them taken here

❗ Naming the block was the deliverable; the fix was explicitly out of scope, and a fix aimed at a misattributed number
is what `idle-cpu-attribution-2026-08-03.md` exists to prevent. Run the discriminator above first.

1. **Load each tower on demand, separately.** `load_towers` loads BOTH whichever one is wanted, so an enrichment pass
   pays for the text tower and one typed search query pays for the image tower. Measured above: the text tower alone is
   251.5 MB of `MALLOC_LARGE`, about 80% of the bill, and a user who never types a semantic search never needs it
   resident. They are independently loadable today. The largest win available, with no quality question attached and no
   inference-speed question either.
2. **Unload after idle.** A tower that has not been asked for anything in N minutes could be dropped and reloaded in the
   1–2 s a cold Core ML load costs. Whether that is acceptable depends on whether the reload lands in a user's typing
   latency, which is a product call.
3. **Reconsider `MLComputeUnits::All`.** Worth ~400 MB, but ❌ not on the strength of the memory number alone: the
   enrichment throughput cost of dropping the GPU has NOT been measured, and enrichment speed is a real user-facing
   property. Measure both sides before touching it.
4. **Convert the text tower to fp16.** It ships fp32 (253,750,976 bytes of weights on disk) while the image tower is
   8-bit palettized. `install.rs` records why: the text tower's 8-bit Core ML inference is all-NaN. fp16 was not tried,
   and it sits between the two.

## What the diagnostic surface leaves behind

Independent of any of this:

- `cmdr_fs::process_memory::query_vm_regions`: the in-process `vmmap -summary` plus per-tag region-size histogram.
- `commands::memory_diagnostics::get_memory_diagnostics`: all four readers in one IPC payload, macOS, release builds
  included. The next "what is Cmdr holding?" starts here.
- `clip::macos::residency_test` and `vision::tests::what_one_vision_analyze_leaves_resident`: `#[ignore]`d harnesses
  that re-measure the ML residency on any Mac, per tower and under any compute-unit assignment.
