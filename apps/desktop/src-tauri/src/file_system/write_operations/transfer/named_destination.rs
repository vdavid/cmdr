//! The explicit leaf of a single transfer shares the ordinary subtree remap.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::file_system::write_operations::types::WriteOperationError;
use crate::file_system::write_operations::validation::validate_transfer_destination_name;

pub(super) fn initial_remap(
    sources: &[PathBuf],
    destination: &Path,
    name: Option<&str>,
) -> Result<HashMap<PathBuf, PathBuf>, WriteOperationError> {
    let Some(name) = name else { return Ok(HashMap::new()) };
    validate_transfer_destination_name(sources, destination, Some(name))?;
    let source_name = sources[0].file_name().expect("validated source filename");
    Ok(HashMap::from([(destination.join(source_name), destination.join(name))]))
}
