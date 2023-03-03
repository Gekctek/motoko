use crate::memory::{ic::NEXT_REGION_ID, Memory, alloc_blob};
use crate::types::{size_of, Region, Value, Words, Blob, Bytes, TAG_REGION};
use crate::rts_trap_with;

use motoko_rts_macros::ic_mem_fn;

// Mutable meta data stored in stable memory header (See motoko/design/StableRegions.md)
mod meta_data {
    // to do: use ic0_stable module to implement the getters/settings below.

    pub mod total_allocated_blocks {
	// to do:
	// get,
	// set
    }

    pub mod total_allocated_regions {
	// (will subsume the temp global, REGION_NEXT_ID)
	// to do: get, incr
    }

    pub mod block_region_table {
	// invariant:
	//  all blocks whose IDs are below the total_allocated_blocks are valid.

	// to do:
	// - set_block_region,
	// - get_block_region
    }

    pub mod region_table {
	// invariant (for now, pre-GC integration):
	//  all regions whose IDs are below the total_allocated_regions are valid.

	// to do:
	// - get_region_size
	// - set_region_size


    }
}

#[ic_mem_fn]
pub unsafe fn region_new<M: Memory>(mem: &mut M) -> Value {
    let r_ptr = mem.alloc_words(size_of::<Region>() + Words(1));
    // NB. cannot use as_region() here as we didn't write the header yet
    let region = r_ptr.get_ptr() as *mut Region;
    (*region).header.tag = TAG_REGION;
    (*region).id = NEXT_REGION_ID;
    NEXT_REGION_ID += 1;
    (*region).page_count = 0;
    (*region).vec_pages = alloc_blob(mem, Bytes(0));
    Value::from_ptr(region as usize)
}

#[ic_mem_fn]
pub unsafe fn region_id<M: Memory>(_mem: &mut M, r: Value) -> u32 {
    let r = r.as_region();
    (*r).id.into()
}

#[ic_mem_fn]
pub unsafe fn region_size<M: Memory>(_mem: &mut M, r: Value) -> u64 {
    let r = r.as_region();
    (*r).page_count.into()
}

#[ic_mem_fn]
pub unsafe fn region_grow<M: Memory>(mem: &mut M, r: Value, new_pages: u64) -> u64 {
    let r = r.as_region();
    let new_pages_ = new_pages as u32;
    let old_page_count = (*r).page_count;
    let new_block_count = (old_page_count + new_pages_ + 127) / 128;
    (*r).page_count += new_pages_;
    let new_vec_pages = alloc_blob(mem, Bytes(new_block_count * 2));
    let old_vec_byte_count = (old_page_count + 127 / 128) * 2;
    let new_vec_byte_count = (old_page_count + new_pages_ + 127 / 128) * 2;
    for i in 0..old_vec_byte_count {
	let new_pages = new_vec_pages.get_ptr() as *mut Blob;
	let old_pages = (*r).vec_pages.get_ptr() as *mut Blob;
	new_pages.set(i, old_pages.get(i));
    }
    //  ## choose and record new block IDs:
    //  ### update meta data:
    //    - let old_total_blocks = meta_data::total_allocated_blocks::get();
    //    - let new_total_blocks = old_total_blocks + new_block_count;
    //    - call meta_data::total_allocated_blocks::set(new_total_blocks)
    //    - call meta_data::block_region_table::set_block_region(r.id, block_id)
    //             for each block_id in old_total_blocks..new_total_blocks-1
    //  ### save new block IDs into new_vec_pages
    //    - call new_vec_pages.set(byte_offset)
    //            for byte_offset in old_vec_byte_count..new_vec_byte_count
    //              if the byte_offset is even, vs odd, ...

    (*r).vec_pages = new_vec_pages;
    old_page_count.into()
}

#[ic_mem_fn]
pub unsafe fn region_load_blob<M: Memory>(_mem: &mut M, _r: Value, _start: Value, _len: Value) -> Value {
    rts_trap_with("TODO region_load_blob");
}

#[ic_mem_fn]
pub unsafe fn region_store_blob<M: Memory>(_mem: &mut M, _r: Value, _start: Value, _blob: Value) {
    rts_trap_with("TODO region_store_blob");
}


#[ic_mem_fn]
pub unsafe fn region_next_id<M: Memory>(_mem: &mut M) -> Value {
    Value::from_scalar(NEXT_REGION_ID as u32)
}
