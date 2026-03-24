---
title: Kindle read/write support
date: 2026-03-24
description:
    Kindles weren't detected as MTP devices. Fixed it in mtp-rs (the pure-Rust MTP library powering Cmdr's phone and
    e-reader support), shipping in Cmdr v0.9.1.
---

Cmdr can browse Android phones and e-readers over USB via [MTP](https://github.com/vdavid/mtp-v1_1-spec-md). But Kindles
weren't detected. As of v0.9.1, they are.

<!-- more -->

## What happened

Someone filed the [first bug](https://github.com/vdavid/mtp-rs/issues/1) against `mtp-rs` (the library powering Cmdr's
MTP support): their Kindle wasn't showing up. Turns out Amazon uses vendor-specific USB descriptors instead of the
standard MTP ones, so the detection logic skipped it.

Fixed and [released](https://crates.io/crates/mtp-rs/0.4.1) right after noticing the issue. I wrote up the
[full debugging process](https://www.veszelovszki.com/a/mtp-rs-bugfix/) if you're curious how I work with AI on stuff
like this.

## Background: `mtp-rs`

I regularly copy our family photos from our phones, so it's something I needed in Cmdr early. It was also a great excuse
to abstract away the file system to allow for other stuff like S3 buckets, git repos, etc.

When I started looking into MTP support, I found that the only real option, `libmtp`, is a C library from ~20 years ago.
I thought: can we do this in pure Rust? Turns out yes. And it's a lot cleaner and faster, too.

[`mtp-rs`](https://github.com/vdavid/mtp-rs) is a pure-Rust, async MTP library. No C dependencies, no FFI, no `unsafe`.
Built on top of [`nusb`](https://crates.io/crates/nusb) for USB access.

The benchmarks are pretty happy. On a Pixel 9 Pro XL, `mtp-rs` is **1.06–4.04x faster** than `libmtp` across all
operations. And the consistency: for 100 MB downloads, `libmtp`'s times ranged from 3.7s to 18.2s (a 5x spread).
`mtp-rs` stayed within a 15 ms (!) band. When it took my family photos an hour to copy, I always thought the USB
connection was slow. Now it turns out, it was only `libmtp` that was slow.

Full benchmarks and details in the [announcement post](https://www.veszelovszki.com/a/mtp-rs/).
