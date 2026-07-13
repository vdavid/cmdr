//! The vector store for image feature-print embeddings: brute-force cosine
//! similarity in Rust (plan Decision 2 — brute-force first, NO `sqlite-vec`), plus a
//! resident per-volume cache so query-time work never reloads ~MBs of BLOBs per call.
//!
//! ## Why brute-force
//!
//! A single user's library is small (well under ~100k images); a linear cosine scan
//! over pre-normalized vectors is low-ms and adds ZERO dependencies. `sqlite-vec` is a
//! real build+signing project (a loadable extension our `rusqlite` isn't built for),
//! adopted only if a real library outgrows brute force — behind this same trait
//! ([`VectorStore`]), so the swap is local.
//!
//! ## Resident cache + the memory watchdog
//!
//! Loading every embedding from `media.db` per query is real work that must run OFF
//! the synchronous IPC thread (plan § Query-time vector residency). [`cache`] keeps a
//! load-once [`BruteForceVectorStore`] per volume (mirroring `search/`'s warm
//! `SEARCH_INDEX` arena). It's invalidated per completed enrichment pass (not per
//! write — that would thrash-reload mid-pass; the plan accepts eventual consistency
//! until a pass completes) and DROPPED wholesale when the indexing memory watchdog
//! fires, so the resident vectors are counted against the ONE shared ceiling, never a
//! second independent budget.

pub mod cache;

#[cfg(test)]
mod tests;

/// One image-similarity hit: the matched image's path and its cosine similarity to
/// the query in `[-1.0, 1.0]` (`1.0` = identical direction). Crosses the IPC boundary
/// (find-similar is surfaced by a command), so it derives `Serialize` + `specta::Type`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SimilarImage {
    /// The matched image's path.
    pub path: String,
    /// Cosine similarity to the query vector.
    pub score: f32,
}

/// A near-duplicate cluster: the paths of images whose feature prints are within the
/// dedup cosine threshold of each other. Crosses the IPC boundary.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DedupCluster {
    /// The paths in this near-duplicate group (two or more).
    pub paths: Vec<String>,
}

/// Cosine similarity between two equal-length vectors, in `[-1.0, 1.0]`. Returns `0.0`
/// when the lengths differ or either vector has zero magnitude (no meaningful angle),
/// rather than a `NaN` — a degenerate vector simply never ranks.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// The vector-store seam. A brute-force impl ships now; a future `sqlite-vec`-backed
/// impl would satisfy the same trait so callers don't change.
pub trait VectorStore {
    /// The `k` images most similar to `query` by cosine, highest first, excluding the
    /// path in `exclude` (the source image of a "find similar" query, so it never
    /// returns itself). Ties broken by path for determinism.
    fn top_k(&self, query: &[f32], k: usize, exclude: Option<&str>) -> Vec<SimilarImage>;

    /// Group the stored images into near-duplicate clusters: every pair within
    /// `threshold` cosine is placed in the same cluster (single-linkage). Only
    /// clusters of two or more are returned. Deterministic ordering (by first path).
    fn dedup_clusters(&self, threshold: f32) -> Vec<DedupCluster>;
}

/// A brute-force cosine vector store over a snapshot of a volume's embeddings. Vectors
/// are stored as-loaded; cosine normalizes per comparison, so no pre-normalization
/// step is needed (the store is small and loaded once).
#[derive(Debug, Clone, Default)]
pub struct BruteForceVectorStore {
    entries: Vec<(String, Vec<f32>)>,
}

impl BruteForceVectorStore {
    /// Build a store from `(path, vector)` pairs (the resident cache's load result).
    pub fn new(entries: Vec<(String, Vec<f32>)>) -> Self {
        Self { entries }
    }

    /// The number of stored vectors.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the store holds no vectors.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The stored vector for `path`, if present (the source vector of a find-similar
    /// query, so a caller needn't re-read the DB).
    pub fn vector_for(&self, path: &str) -> Option<&[f32]> {
        self.entries.iter().find(|(p, _)| p == path).map(|(_, v)| v.as_slice())
    }
}

impl VectorStore for BruteForceVectorStore {
    fn top_k(&self, query: &[f32], k: usize, exclude: Option<&str>) -> Vec<SimilarImage> {
        if k == 0 || query.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<SimilarImage> = self
            .entries
            .iter()
            .filter(|(path, _)| exclude != Some(path.as_str()))
            .map(|(path, vector)| SimilarImage {
                path: path.clone(),
                score: cosine(query, vector),
            })
            .collect();
        // Highest similarity first; ties by path for determinism.
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path))
        });
        scored.truncate(k);
        scored
    }

    fn dedup_clusters(&self, threshold: f32) -> Vec<DedupCluster> {
        let n = self.entries.len();
        // Single-linkage union-find over pairs within the threshold.
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        for i in 0..n {
            for j in (i + 1)..n {
                if cosine(&self.entries[i].1, &self.entries[j].1) >= threshold {
                    let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                    if ri != rj {
                        parent[ri] = rj;
                    }
                }
            }
        }
        // Gather members per root, keeping only clusters of two or more.
        let mut groups: std::collections::HashMap<usize, Vec<String>> = std::collections::HashMap::new();
        for i in 0..n {
            let root = find(&mut parent, i);
            groups.entry(root).or_default().push(self.entries[i].0.clone());
        }
        let mut clusters: Vec<DedupCluster> = groups
            .into_values()
            .filter(|paths| paths.len() >= 2)
            .map(|mut paths| {
                paths.sort();
                DedupCluster { paths }
            })
            .collect();
        // Deterministic order: by the first (smallest) path in each cluster.
        clusters.sort_by(|a, b| a.paths[0].cmp(&b.paths[0]));
        clusters
    }
}
