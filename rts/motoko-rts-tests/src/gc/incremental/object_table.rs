use std::mem::size_of;

use motoko_rts::gc::incremental::write_barrier::{
    create_young_remembered_set, take_young_remembered_set, using_incremental_barrier,
};
use motoko_rts::{
    gc::incremental::object_table::ObjectTable,
    memory::{alloc_blob, Memory},
    types::{Blob, Bytes, Value, Words, NULL_OBJECT_ID, OBJECT_TABLE},
};
use oorandom::Rand32;

use crate::{gc::utils::WORD_SIZE, memory::TestMemory};

pub unsafe fn test() {
    println!("  Testing object table ...");
    assert!(OBJECT_TABLE.is_none());
    test_allocate();
    test_remove_realloc();
    test_move();
    test_table_growth();
}

const TEST_SIZE: usize = 10_000;

fn test_allocate() {
    let mut mem = TestMemory::new(Words(TEST_SIZE as u32));
    let mut object_table = create_object_table(&mut mem, TEST_SIZE);
    let mut expected_table = [(NULL_OBJECT_ID, 0); TEST_SIZE];
    allocate_entries(&mut mem, &mut object_table, &mut expected_table);
    check_all_entries(&object_table, &expected_table);
    free_all_entries(&mut object_table, &expected_table);
}

fn create_object_table(mem: &mut TestMemory, length: usize) -> ObjectTable {
    let size = Words(length as u32);
    let base = unsafe { mem.alloc_words(size) } as *mut usize;
    ObjectTable::new(base, length)
}

fn allocate_entries(
    mem: &mut TestMemory,
    object_table: &mut ObjectTable,
    expected_table: &mut [(Value, usize)],
) {
    for count in 0..expected_table.len() {
        let address = object_table.end() + count * WORD_SIZE;
        let object_id = object_table.new_object_id(mem, address);
        assert_eq!(object_table.get_object_address(object_id), address);
        expected_table[count] = (object_id, address);
        assert_eq!(object_table.get_object_address(object_id), address);
    }
}

fn check_all_entries(object_table: &ObjectTable, expected_table: &[(Value, usize)]) {
    for (object_id, address) in expected_table.iter() {
        assert_eq!(object_table.get_object_address(*object_id), *address);
    }
}

fn free_all_entries(object_table: &mut ObjectTable, expected_table: &[(Value, usize)]) {
    for (object_id, _) in expected_table.iter() {
        object_table.free_object_id(*object_id);
    }
}

fn delete_random_half(
    object_table: &mut ObjectTable,
    expected_table: &mut [(Value, usize)],
) -> usize {
    const RANDOM_SEED: u64 = 4711;
    let mut random = Rand32::new(RANDOM_SEED);
    let mut deleted = 0;
    for index in 0..expected_table.len() {
        if random.rand_u32() % 2 == 0 {
            let object_id = expected_table[index].0;
            object_table.free_object_id(object_id);
            expected_table[index].0 = NULL_OBJECT_ID;
            deleted += 1;
        }
    }
    deleted
}

fn reallocate(
    mem: &mut TestMemory,
    object_table: &mut ObjectTable,
    expected_table: &mut [(Value, usize)],
) {
    let mut free_index = 0;
    while free_index < expected_table.len() && expected_table[free_index].0 != NULL_OBJECT_ID {
        free_index += 1;
    }
    assert!(free_index < expected_table.len());
    let address = expected_table[free_index].1;
    expected_table[free_index].0 = object_table.new_object_id(mem, address);
}

fn test_remove_realloc() {
    let mut mem = TestMemory::new(Words(TEST_SIZE as u32));
    let mut object_table = create_object_table(&mut mem, TEST_SIZE);
    let mut expected_table = [(NULL_OBJECT_ID, 0); TEST_SIZE];
    allocate_entries(&mut mem, &mut object_table, &mut expected_table);
    check_all_entries(&object_table, &expected_table);
    let deleted = delete_random_half(&mut object_table, &mut expected_table);
    for _ in 0..deleted {
        reallocate(&mut mem, &mut object_table, &mut expected_table);
    }
    check_all_entries(&object_table, &expected_table);
    free_all_entries(&mut object_table, &expected_table);
}

fn move_all_objects(object_table: &mut ObjectTable, expected_table: &mut [(Value, usize)]) {
    for index in 0..expected_table.len() {
        let (object_id, old_address) = expected_table[index];
        let new_address = old_address + 3 * WORD_SIZE;
        object_table.move_object(object_id, new_address);
        expected_table[index].1 = new_address;
    }
}

fn test_move() {
    let mut mem = TestMemory::new(Words(TEST_SIZE as u32));
    let mut object_table = create_object_table(&mut mem, TEST_SIZE);
    let mut expected_table = [(NULL_OBJECT_ID, 0); TEST_SIZE];
    allocate_entries(&mut mem, &mut object_table, &mut expected_table);
    check_all_entries(&object_table, &expected_table);
    move_all_objects(&mut object_table, &mut expected_table);
    check_all_entries(&object_table, &expected_table);
}

unsafe fn test_table_growth() {
    const TEST_ALLOCATIONS: usize = 100;
    let size = TEST_SIZE * WORD_SIZE + TEST_ALLOCATIONS * size_of::<Blob>();
    let mut mem = TestMemory::new(Bytes(size as u32).to_words());
    let object_table = create_object_table(&mut mem, 1);
    assert!(!using_incremental_barrier());
    OBJECT_TABLE = Some(object_table);
    mem.set_last_heap_pointer(mem.get_heap_pointer());
    create_young_remembered_set(&mut mem);
    for _ in 0..1000 {
        let blob = alloc_blob(&mut mem, Bytes(0u32));
        assert!(blob.is_object_id());
    }
    take_young_remembered_set();
    assert!(!using_incremental_barrier());
    OBJECT_TABLE = None;
}
