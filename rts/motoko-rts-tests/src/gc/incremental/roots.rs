use std::{array::from_fn, ptr::null_mut};

use motoko_rts::{
    gc::incremental::object_table::OBJECT_TABLE,
    gc::incremental::roots::visit_roots,
    memory::{alloc_array, Memory, Roots},
    remembered_set::{RememberedSet, INITIAL_TABLE_LENGTH},
    types::{Array, Tag, Value, Words, TAG_ARRAY},
};

use crate::{
    gc::{
        heap::MotokoHeap,
        utils::{ObjectIdx, GC, WORD_SIZE},
    },
    memory::TestMemory,
};

pub unsafe fn test() {
    println!("  Testing roots...");

    check_regular_roots();
    check_visit_remembered_set();
}

unsafe fn check_regular_roots() {
    let object_map: [(ObjectIdx, Vec<ObjectIdx>); 16] = from_fn(|id| (id as u32, vec![]));
    let root_indices = [2, 4, 6, 8, 10, 12, 14];
    let continuation_indices = [3, 5, 11, 13];

    let heap = MotokoHeap::new(
        &object_map,
        &root_indices,
        &continuation_indices,
        GC::Incremental,
    );
    check_visit_static_roots(&heap, &root_indices);
    check_visit_continuation_table(&heap, &continuation_indices);
    OBJECT_TABLE = null_mut();
}

unsafe fn check_visit_static_roots(heap: &MotokoHeap, root_indices: &[ObjectIdx]) {
    let roots = get_roots(heap);
    let mut visited_static_roots = vec![];
    visit_roots(
        roots,
        heap.heap_base_address(),
        None,
        &mut visited_static_roots,
        |context, value| {
            let array = value.as_array();
            if array.len() == 1 {
                let id = address_to_index(value.get_object_address());
                context.push(id);
            }
        },
    );
    assert_eq!(visited_static_roots, root_indices);
}

unsafe fn check_visit_continuation_table(heap: &MotokoHeap, continuation_indices: &[ObjectIdx]) {
    let roots = get_roots(heap);
    let mut visited_continuations = vec![];
    visit_roots(
        roots,
        heap.heap_base_address(),
        None,
        &mut visited_continuations,
        |context, value| {
            let array = value.as_array();
            if array.len() != 1 {
                assert_eq!(context.len(), 0);
                for index in 0..array.len() {
                    let element = array.get(index);
                    let id = address_to_index(element.get_object_address());
                    context.push(id);
                }
            }
        },
    );
    assert_eq!(visited_continuations, continuation_indices);
}

unsafe fn check_visit_remembered_set() {
    assert_eq!(OBJECT_TABLE, null_mut());
    let mut mem = TestMemory::new(Words(WORD_SIZE as u32 * INITIAL_TABLE_LENGTH));
    let static_roots = alloc_array(&mut mem, 0);

    let remembered_set_values: [Value; 4] = from_fn(|_| alloc_array(&mut mem, 0));

    let mut remembered_set = RememberedSet::new(&mut mem);
    for value in remembered_set_values {
        remembered_set.insert(&mut mem, value);
    }

    let roots = Roots {
        static_roots,
        continuation_table_location: null_mut(),
    };
    let mut visited_remembered_values = vec![];
    visit_roots(
        roots,
        mem.get_heap_base(),
        Some(&remembered_set),
        &mut visited_remembered_values,
        |context, value| {
            context.push(value);
        },
    );
    assert!(visited_remembered_values == remembered_set_values);
}

unsafe fn get_roots(heap: &MotokoHeap) -> Roots {
    let static_roots = heap.static_root_array_id();
    let continuation_table_location = heap.continuation_table_ptr_address() as *mut Value;
    assert_ne!(continuation_table_location, null_mut());
    Roots {
        static_roots,
        continuation_table_location,
    }
}

unsafe fn address_to_index(address: usize) -> u32 {
    let tag = address as *mut Tag;
    debug_assert_eq!(*tag, TAG_ARRAY);
    let array = tag as *mut Array;
    debug_assert_eq!(array.len(), 1);
    let index = array.get(0);
    index.get_scalar()
}
