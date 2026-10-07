//! Multi-Rename Tool (⌃M): Total Commander's mask renamer. See `DETAILS.md`.

pub(crate) mod mask;
pub(crate) mod plan;
pub(crate) mod presets;
pub(crate) mod run;
pub(crate) mod transform;

#[cfg(test)]
mod mask_test;
#[cfg(test)]
mod plan_test;
#[cfg(test)]
mod run_test;
#[cfg(test)]
mod transform_test;
