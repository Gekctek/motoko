//! Write barrier for the incremental GC.
//! Pre-update, field-level barrier used for snapshot-at-the-beginning marking.

use motoko_rts_macros::ic_mem_fn;

use crate::{
    gc::incremental::IncrementalGC,
    memory::Memory,
    types::{is_skewed, Value},
};

/// Write a value with a pre-update and post-update barrier. Used for the incremental GC.
/// `location` (unskewed) denotes the field or array element that will be written.
/// `value` (skewed if a pointer) denotes the value to be written.
/// The barrier can be conservatively called even if the stored value might not be a pointer.
/// Purpose of the barrier for pointer writes:
/// * Pre-update: Used during mark phase to guarantee snapshot-at-the-beginning marking.
/// * Post-update: Used during update phase to forward old pointers to new location.
#[ic_mem_fn]
pub unsafe fn incremental_write_with_barrier<M: Memory>(
    mem: &mut M,
    location: *mut Value,
    value: Value,
) {
    incremental_pre_write_barrier(mem, location);
    *location = value;
}

/// Write barrier to be called BEFORE a pointer store.
/// `location` (unskewed) denotes the field or array element that will be written.
/// The barrier is conservatively called even if the stored value might not be a pointer.
#[ic_mem_fn]
pub unsafe fn incremental_pre_write_barrier<M: Memory>(mem: &mut M, location: *mut Value) {
    debug_assert!(!is_skewed(location as u32));
    debug_assert_ne!(location, core::ptr::null_mut());
    IncrementalGC::instance(mem).pre_write_barrier(*location);
}
