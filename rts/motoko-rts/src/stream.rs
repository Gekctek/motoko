//! The implementation of streaming serialisation
//!
//! When serialising Motoko stable variables to stable memory we used to first completely
//! fill up an in-heap buffer and then copy that wholesale into stable memory. This can be
//! disadvantageous for two reasons:
//!  - double copying
//!  - heap congestion (especially for the compacting collector)
//!
//! Instead now we'll only allocate a small(ish) blob that will serve as a temporary storage
//! for bytes in transit, while bigger chunks will flush this staging area before being written
//! directly to destination.
//!
//!

// Layout of a stream node:
//
// ┌────────────┬─────┬─────────┬───────┬─────────┬─────────┬───────────┬────────┬──────────┐
// │ obj header │ len │ padding | ptr64 │ start64 │ limit64 │ outputter │ filled │ cache... │
// └────────────┴─────┴─────────┴───────┴─────────┴─────────┴───────────┴────────┴──────────┘
//
// We reuse the opaque nature of blobs (to Motoko) and stick Rust-related information
// into the leading bytes:
// - `obj header` contains tag and forwarding pointer
// - `len` is in blob metadata
// - 'padding' to align to 64-bit
// - `ptr64` and `limit64` are the next and past-end pointers into stable memory
// - `filled` and `cache` are the number of bytes consumed from the blob, and the
//   staging area of the stream, respectively
// - `outputter` is the function to be called when `len - filled` approaches zero.
// - INVARIANT: keep `BlobStream.{ptr64_field, start64_field, filled_field}`,
//              (from `compile.ml`) in sync with the layout!
// - Note: `len` and `filled` are relative to the encompassing blob.

use crate::bigint::{check, mp_get_u32, mp_isneg, mp_iszero};
use crate::gc::incremental::post_allocation_barrier;
use crate::mem_utils::memcpy_bytes;
use crate::memory::{alloc_blob, Memory};
use crate::rts_trap_with;
use crate::tommath_bindings::{mp_div_2d, mp_int};
use crate::types::{size_of, Blob, Bytes, Stream, Value, TAG_BLOB};

use motoko_rts_macros::ic_mem_fn;

const MAX_STREAM_SIZE: Bytes<u32> = Bytes((1 << 30) - 1);
const INITIAL_STREAM_FILLED: Bytes<u32> = Bytes(36);
const STREAM_CHUNK_SIZE: Bytes<u32> = Bytes(128);

#[ic_mem_fn]
pub unsafe fn alloc_stream<M: Memory>(mem: &mut M, size: Bytes<u32>) -> *mut Stream {
    debug_assert_eq!(
        INITIAL_STREAM_FILLED,
        (size_of::<Stream>() - size_of::<Blob>()).to_bytes()
    );
    if size > MAX_STREAM_SIZE {
        rts_trap_with("alloc_stream: Cache too large");
    }
    let ptr = alloc_blob(mem, size + INITIAL_STREAM_FILLED);
    let stream = ptr.as_stream();
    (*stream).padding = 0;
    (*stream).ptr64 = 0;
    (*stream).start64 = 0;
    (*stream).limit64 = 0;
    (*stream).outputter = Stream::no_backing_store;
    (*stream).filled = INITIAL_STREAM_FILLED;
    post_allocation_barrier(ptr);
    stream
}

#[allow(dead_code)]
extern "C" {
    // generated by `moc`
    fn stable64_write_moc(to: u64, ptr: u64, n: u64);
}

impl Stream {
    #[inline]
    pub unsafe fn cache_addr(self: *const Self) -> *const u8 {
        self.add(1) as *const u8 // skip closure header
    }

    /// make sure that the cache is empty
    fn flush(self: *mut Self) {
        unsafe {
            if (*self).filled > INITIAL_STREAM_FILLED {
                ((*self).outputter)(
                    self,
                    self.cache_addr(),
                    (*self).filled - INITIAL_STREAM_FILLED,
                );
                (*self).filled = INITIAL_STREAM_FILLED
            }
        }
    }

    fn no_backing_store(self: *mut Self, _ptr: *const u8, _n: Bytes<u32>) {
        assert!(false)
    }

    #[cfg(feature = "ic")]
    fn send_to_stable(self: *mut Self, ptr: *const u8, n: Bytes<u32>) {
        unsafe {
            let next_ptr64 = (*self).ptr64 + n.as_u32() as u64;
            stable64_write_moc((*self).ptr64, ptr as u64, n.as_u32() as u64);
            (*self).ptr64 = next_ptr64
        }
    }

    #[cfg(feature = "ic")]
    /// Sets up the bottleneck routine to output towards a range of stable memory
    /// Note: assumes that the entire byte range is writable
    #[export_name = "stream_stable_dest"]
    pub fn setup_stable_dest(self: *mut Self, start: u64, limit: u64) {
        unsafe {
            (*self).padding = 0;
            (*self).ptr64 = start;
            (*self).start64 = start;
            (*self).limit64 = limit;
            (*self).outputter = Self::send_to_stable;
        }
    }

    /// Ingest a number of bytes into the stream.
    #[export_name = "stream_write"]
    pub fn cache_bytes(self: *mut Self, ptr: *const u8, n: Bytes<u32>) {
        unsafe {
            if (*self).limit64 != 0 && n > STREAM_CHUNK_SIZE
                || (*self).filled + n > (*self).header.len
            {
                self.flush();
                ((*self).outputter)(self, ptr, n);
            } else {
                let dest = self
                    .as_blob_mut()
                    .payload_addr()
                    .add((*self).filled.as_usize());
                (*self).filled += n;
                assert!((*self).filled <= (*self).header.len);
                memcpy_bytes(dest as usize, ptr as usize, n);
            }
        }
    }

    /// Ingest a single byte into the stream.
    #[inline]
    #[export_name = "stream_write_byte"]
    pub fn cache_byte(self: *mut Self, byte: u8) {
        unsafe {
            if (*self).filled >= (*self).header.len {
                self.flush()
            }
            self.as_blob_mut().set((*self).filled.as_u32(), byte);
            (*self).filled += Bytes(1)
        }
    }

    /// Return a pointer to a reserved area of the cache and advance the
    /// fill indicator beyond it.
    #[export_name = "stream_reserve"]
    pub fn reserve(self: *mut Self, bytes: Bytes<u32>) -> *mut u8 {
        unsafe {
            if (*self).filled + bytes > (*self).header.len {
                self.flush()
            }
            let ptr = self
                .as_blob_mut()
                .payload_addr()
                .add((*self).filled.as_usize());
            (*self).filled += bytes;
            ptr
        }
    }

    /// like `bigint_leb128_encode_go`, but to a stream
    pub(crate) unsafe fn write_leb128(self: *mut Stream, tmp: *mut mp_int, add_bit: bool) {
        debug_assert!(!mp_isneg(tmp));

        loop {
            let byte = mp_get_u32(tmp) as u8;
            check(mp_div_2d(tmp, 7, tmp, core::ptr::null_mut()));
            if !mp_iszero(tmp) || (add_bit && byte & (1 << 6) != 0) {
                self.cache_byte(byte | (1 << 7));
            } else {
                return self.cache_byte(byte);
            }
        }
    }

    /// Split the stream object into two `Blob`s, a front-runner (small) one
    /// and a latter one that comprises the current amount of the cached bytes.
    /// Lengths are adjusted correspondingly.
    #[export_name = "stream_split"]
    pub unsafe fn split(self: *mut Self) -> Value {
        if (*self).header.len > (*self).filled {
            self.as_blob_mut().shrink((*self).filled);
        }
        (*self).header.len = INITIAL_STREAM_FILLED - size_of::<Blob>().to_bytes();
        (*self).filled -= INITIAL_STREAM_FILLED;
        let blob = (self.cache_addr() as *mut Blob).sub(1);
        blob.initialize_tag(TAG_BLOB);
        let ptr = Value::from_ptr(blob as usize);
        (*blob).header.forward = ptr;
        debug_assert_eq!(blob.len(), (*self).filled);
        post_allocation_barrier(ptr);
        ptr
    }

    /// Shut down the stream by outputting all data. Lengths are
    /// adjusted correspondingly, and the stream remains intact.
    #[export_name = "stream_shutdown"]
    pub unsafe fn shutdown(self: *mut Self) {
        self.flush()
    }
}
