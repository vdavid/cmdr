//! macOS: `phys_footprint` from one `task_info(TASK_VM_INFO)` call.

pub(crate) fn footprint() -> Option<u64> {
    super::super::query_task_vm_info().map(|vm| vm.phys_footprint)
}
