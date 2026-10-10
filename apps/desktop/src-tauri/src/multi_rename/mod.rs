//! Multi-Rename Tool (⌃M): Total Commander's mask renamer. See `DETAILS.md`.

pub(crate) mod error;
pub(crate) mod mask;
pub(crate) mod plan;
pub(crate) mod presets;
pub(crate) mod run;
pub(crate) mod session;
pub(crate) mod transform;
mod transliterate;

#[cfg(test)]
mod mask_test;
#[cfg(test)]
mod plan_test;
#[cfg(test)]
mod presets_test;
#[cfg(test)]
mod run_test;
#[cfg(test)]
mod transform_test;
#[cfg(test)]
mod transliterate_test;
