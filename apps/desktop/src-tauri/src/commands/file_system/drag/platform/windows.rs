//! Windows: no drag-out yet (OLE `DoDragDrop` with `CF_HDROP` is the one to add).

pub(crate) use super::unsupported::{
    clear_self_drag_overlay, prepare_self_drag_overlay, set_self_drag_resolved_op, start_drag,
};
