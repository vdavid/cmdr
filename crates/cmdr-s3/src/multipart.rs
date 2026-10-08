//! How a big object is cut into parts.
//!
//! One part size per upload, every part that size except the last: R2 refuses
//! a completion whose parts differ (`InvalidPart`), so we always do it, and it
//! makes a resumed or re-sent part land on the same byte range. What happens
//! to a tail under 5 MiB is the provider's [`ShortTail`]: Garage refuses an
//! `UploadPartCopy` source that small even as the last part
//! (`apps/desktop/test/s3-servers/README.md`), so it folds into the part before
//! it there; R2 refuses a last part LARGER than the rest (live, 2026-10-02),
//! so it stays its own part there.

/// 1 MiB.
const MIB: u64 = 1024 * 1024;

/// The smallest part we cut. Well above S3's 5 MiB floor: fewer, bigger parts
/// mean fewer billed requests, and progress still moves every few seconds.
pub(crate) const MIN_PART_SIZE: u64 = 64 * MIB;

/// A last part smaller than this joins the part before it under
/// [`ShortTail::Fold`].
const MIN_TAIL: u64 = 5 * MIB;

/// S3's ceiling on parts per upload, on every provider we know.
pub(crate) const MAX_PARTS: u64 = 10_000;

/// S3's ceiling on one part.
pub(crate) const MAX_PART_SIZE: u64 = 5 * 1024 * MIB;

/// S3's ceiling on one `CopyObject`, which is all a provider without
/// `UploadPartCopy` (GCS) has.
pub(crate) const MAX_COPY_OBJECT_SIZE: u64 = 5 * 1024 * MIB;

/// What a tail under 5 MiB does, per provider (`ProviderProfile::short_tail`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShortTail {
    /// Joins the part before it, making that last part up to 5 MiB larger:
    /// for a server that refuses a small `UploadPartCopy` source (Garage).
    Fold,
    /// Stays its own, smaller last part: S3's rule, and the only shape R2
    /// takes.
    Keep,
}

/// An upload's parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PartPlan {
    /// Every part's size except the last, which may be smaller, or up to
    /// 5 MiB larger when a short tail folded into it.
    pub part_size: u64,
    pub part_count: u32,
    pub total: u64,
}

/// Too big for 10,000 parts of 5 GiB (about 48.8 TiB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TooLarge;

/// The plan for `total` bytes: parts of at least 64 MiB and at least
/// `total / 10,000`, rounded up to a whole MiB, with a tail under 5 MiB folded
/// into the part before it unless that part would pass 5 GiB. Production
/// plans through [`plan_parts_with_floor`] with the volume's floor, which is
/// this unless a Docker cell lowered it.
#[cfg(test)]
pub(crate) fn plan_parts(total: u64) -> Result<PartPlan, TooLarge> {
    plan_parts_with_floor(total, MIN_PART_SIZE, ShortTail::Fold)
}

/// The plan for `total` bytes with parts of at least `floor` and the
/// provider's `tail` rule. ❗ Production always plans with
/// [`MIN_PART_SIZE`]; a Docker or live cell lowers it to see several parts
/// without uploading hundreds of megabytes. `floor` is clamped to S3's 5 MiB.
pub(crate) fn plan_parts_with_floor(total: u64, floor: u64, tail_rule: ShortTail) -> Result<PartPlan, TooLarge> {
    if total > MAX_PARTS * MAX_PART_SIZE {
        return Err(TooLarge);
    }
    // At most 5 GiB, itself a whole MiB, so rounding can't push it past the cap.
    let needed = total.div_ceil(MAX_PARTS).div_ceil(MIB) * MIB;
    let part_size = needed.max(floor.max(MIN_TAIL));
    let mut part_count = total.div_ceil(part_size).max(1);
    let tail = total % part_size;
    if tail_rule == ShortTail::Fold
        && part_count > 1
        && tail > 0
        && tail < MIN_TAIL
        && part_size + tail <= MAX_PART_SIZE
    {
        part_count -= 1;
    }
    Ok(PartPlan {
        part_size,
        part_count: u32::try_from(part_count).map_err(|_| TooLarge)?,
        total,
    })
}

impl PartPlan {
    /// `total` bytes as one part, which S3 takes at any size as an upload's
    /// only part: an overwrite that must not go as a PUT (`volume/writes.rs`).
    /// `total` stays within a one-PUT shape, far under the 5 GiB part ceiling.
    pub(crate) fn whole(total: u64) -> Self {
        Self {
            part_size: total,
            part_count: 1,
            total,
        }
    }

    /// The inclusive byte range of 1-based `part_number`, the shape
    /// `x-amz-copy-source-range` and a ranged read both take. The last part
    /// runs to the end of the object. Meaningless for an empty upload, which
    /// has no bytes to range over.
    pub(crate) fn range(&self, part_number: u32) -> (u64, u64) {
        let start = u64::from(part_number.saturating_sub(1)) * self.part_size;
        let end = if part_number >= self.part_count {
            self.total
        } else {
            (start + self.part_size).min(self.total)
        };
        (start, end.saturating_sub(1))
    }
}

#[cfg(test)]
#[path = "multipart_test.rs"]
mod multipart_test;
