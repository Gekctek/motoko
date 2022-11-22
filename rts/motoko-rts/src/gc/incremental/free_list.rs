//! Segregated free list used by the incremental garbage collector.
//! Will be later replaced by moving evacuating collection.
//! 
//! Constant amount of free lists, each with a defined size class.
//! 
//! Memory representation per free list. 
//! Doubly linking for fast list removal when merging free neighours.
//! 
//! first ──> ┌────────┬────────┬──────────┬───────────────┐
//!           │ header │  next  | previous |    (free)     |
//!           └────────┴────────┴──────────┴───────────────┘
//! 
//! The header encodes the block size with a special added tag: 
//!   `TAG_FREE_BLOCK_MIN + word_size`. 
//! Free blocks must be unmarked.
//! 
//! The smallest free block size is 3 words (12 bytes).
//! 
//! * `header` is useful for the sweep phase to identify old free blocks.
//!    i.e. blocks already freed by a previous GC increment. 
//!    The header also encodes the block size (TAG_FREE_BLOCK_MIN + size). 
//!    This helps to determine the remainder that is split off on allocation. 
//!    Moreover, the size information supports heap traversal during the GC 
//!    sweep phase.
//! * `next` denotes the next block in the corresponding free list. 
//!    This supports fast removal of the first list item on allocation.
//! * `previous` denotes the previous block in the corresponding free list 
//!    This enables fast free neighbor block merging.
//! 
//! Allocation first consults the free list of the next higher size class.
//! E.g. `allocate(100)` looks in the free list of size class [128, 256].
//! *  If the free list is non-empty, the first block is a match and returned.
//! *  The remainder is cut off and replaced into the corresponding free list.
//! *  Small remainders (less than smallest size class) remain unused 
//!    and each unused word is identified by the specific Tag `ONE_WORD_FREE`.
//! *  If a free list is empty, allocation advances to the next larger list
//!    until reaching the last list.
//! *  The last list constitutes an overflow list for large memory blocks
//!    and requires linear search. 
//! *  If no more free memory, try to reserve more free WASM space.
//! 
//! Deallocation determines the free list with the size class that includes
//! the freed block size.
//! *  Free neighbors are merged by first removing the neighbor from its list.
//! 
//! Performance per block operation (`n` = number of blocks):
//! *  Allocation by the mutator: `O(1)` unless overflow list `O(n)`.
//! *  Free by the GC: `O(1)`, including merging.
//! 
//! Free blocks are never visited by the GC mark phase. 
//! 

use core::ptr::{null_mut, null};

use crate::{constants::WORD_SIZE, types::TAG_ONE_WORD_FILLER};

#[repr(C)] // See note in `types.rs`
pub struct FreeBlock {
    pub header: Obj,
    pub next: *mut FreeBlock,
    pub previous: *mut FreeBlock,
}

impl FreeBlock {
    pub unsafe fn initialize(self: *mut Self, size: Bytes<u32>) {
        assert(size.as_u32() >= size_of::<Self>().to_bytes().as_u32());
        assert_eq!(size.to_bytes() % WORD_SIZE, 0);
        let words = size.to_words();
        #[cfg(debug_assertions)]
        memzero(self, words);
        assert(words.as_u32() <= u32::MAX - TAG_FREE_BLOCK_MIN);
        (*self).header.raw_tag = TAG_FREE_BLOCK_MIN + words.as_u32();
        assert!(!(*self).header.is_marked());
        (*self).next = null_mut();
        (*self).previous = null_mut();
    }

    /// Size of the free entire free block (includes object header)
    pub unsafe fn size(self: *mut Self) -> Bytes<u32> {
        assert!(!(*self).header.is_marked());
        assert!((*self).header.raw_tag > TAG_FREE_BLOCK_MIN);
        let words = (*self).header.raw_tag - TAG_FREE_BLOCK_MIN;
        Words(words).to_bytes()
    }

    // Remainder block is returned unless it is too small (internal fragmentation)
    pub unsafe fn split(self: *mut Self, size: usize) -> *mut FreeBlock {
        let min_size = size_of::<Self>().to_bytes().as_usize();
        assert!(size >= min_size);
        assert!(size <= self.size());
        self.initialize(size);
        let remainder_address = self as usize + size;
        let remainder_size = self.size() - size;
        assert_eq!(remainder_size % WORD_SIZE, 0);
        if remainder_size < min_size() {
            for word in 0..remainder_size as u32 / WORD_SIZE {
                let address = remainder_address as u32 + word * WORD_SIZE;
                (address as *mut usize) = TAG_ONE_WORD_FILLER;
            }
            null_mut()
        } else {
            let remainder = remainder_address as *mut FreeBlock;
            remainder.initialize(remainder_size);
            remainder
        }
    }
}

struct Range {
    lower: usize, // inclusive
    upper: usize, // exclusive
}

impl Range {
    pub fn new(lower: usize, upper: usize) -> Range {
        assert!(lower <= upper);
        Range { lower, upper }
    }

    pub fn lower(&self) -> usize {
        self.lower
    }

    pub fn upper(&self) -> usize {
        self.upper
    }

    pub fn includes(&self, value: usize) -> bool {
        self.lower <= value && value < self.upper
    }
}

struct FreeList {
    size_class: Range,
    first: *mut FreeBlock
}

impl FreeList {
    pub fn new(size_class: Range) -> FreeList {
        assert!(size_class.lower() >= size_of::<FreeBlock>().to_bytes().as_usize());
        FreeList { 
            size_class,
            first: null_mut()
        }
    }

    unsafe fn fits(block: *mut FreeBlock) -> bool {
        self.size_class.includes(block.size())
    }

    pub fn size_class(&self) -> Range {
        self.size_class
    }
    
    pub fn is_overflow_list() -> bool {
        self.size_class.upper = usize::MAX
    }

    pub unsafe fn insert(&mut self, block: *mut FreeBlock) {
        assert_ne!(block, null_mut());
        assert!(self.fits(block));
        assert_eq!(block.next, null_mut());
        assert_eq!(block.previous, null_mut());
        block.next = self.first;
        if self.first != null_mut() {
            self.first.previous = block;
        }
        self.first = block;
    }

    /// returns null if empty
    pub unsafe fn remove_first(&mut self) -> *mut FreeBlock {
        let block = self.first;
        if block != null {
            assert!(block.previous == null_mut());
            if block.next != null_mut() {
                block.next.previous = null_mut();
            }
            self.first = block.next;
            block.next = null_mut();
        }
        assert!(self.fits(block));
        block
    }

    pub unsafe fn remove(&mut self, block: *mut FreeBlock) {
        assert_ne!(block, null_mut());
        assert!(self.fits(block));
        if block.next != null_mut() {
            block.next.previous = block.previous;
        }
        if block.previous != null_mut() {
            block.previous.next = block.next;
        }
        if block == self.first {
            assert_eq!(block.previous, null_mut());
            self.first = block.next;
        }
        block.next = null_mut();
        block.previous = null_mut();
    }
}

const KB: usize = 1024;
const MB: usize = 1024 * KB;
const LIST_COUNT: usize = 8;
const SIZE_CLASSES: [usize; LIST_COUNT] = [12, 48, 128, 512, 4 * KB, MB, 32 * MB, 256 * MB];

struct SegregatedFreeList {
    lists: [FreeList; LIST_COUNT],
}

impl SegregatedFreeList {
    pub fn initialize() -> SegregatedFreeList {
        SegregatedFreeList {
            lists: [
                Self::free_list(0), Self::free_list(1), Self::free_list(2), Self::free_list(3),
                Self::free_list(4), Self::free_list(5), Self::free_list(6), Self::free_list(7)
            ]
        }
    }

    fn free_list(index: usize) -> FreeList {
        let lower = SIZE_CLASSES[index];
        let upper = if index + 1 < SIZE_CLASSES.len() { SIZE_CLASSES[index + 1] } else { usize::MAX };
        FreeList::new(Range(lower, upper))
    }

    fn allocation_list(&self, size: usize) -> &mut FreeList {
        for index in 0..lists.len() {
            let list = &mut self.lists[index];
            if size >= list.size_class().lower() && !list.is_empty() {
                return list;
            }
        }
        return &mut lists[lists.len() - 1]; // empty overflow list
    }

    fn insertion_list(&self, size: usize) -> &mut FreeList {
        for index in 0..lists.len() {
            let list = &mut self.lists[index];
            if list.size_class().includes(size) {
                return list;
            }
        }
        panic!("No matching free list");
    }

    pub unsafe fn allocate(&mut self, size: usize) -> *mut FreeBlock {
        let list = self.allocation_list(size);
        let mut block = if list.is_overflow_list() {
            list.first_fit_search(size)
        } else {
            list.remove_first()
        };
        if block == null_mut() {
            block = Self::grow_memory(size);
        }
        assert!(block != null_mut());
        if block.size() > size {
            let remainder = block.split(size);
            self.free(remainder);
        }
        assert_eq!(block.size(), size);
        assert_eq!(block.next, null_mut());
        assert_eq!(block.previous, null_mut());
    }

    pub unsafe fn free(block: *mut FreeBlock) {
        let list = self.insertion_list(block.size());
        list.insert(block);
    }

    pub unsafe fn merge(left: *mut FreeBlock, right: *mut FreeBlock) {
        assert_eq!(left as usize + left.size(), right as usize);
        let left_list = self.insertion_list(left.size());
        left_list.remove(left);
        let right_list = self.insertion_list(right.size());
        right_list.remove(right);
        let merged_size = left.size() + right.size();
        let merged_block = left.initialize(new_size);
        free(merged_block);
    }

    fn grow_memory(size: usize) -> *mut FreeBlock {
        panic!("Not yet implemented");
    }
}
