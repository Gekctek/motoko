use crate::{
    gc::incremental::{
        partition_map::{Partition, PartitionMap, MAX_PARTITIONS},
        Phase, PARTITION_MAP, PHASE,
    },
    mem_utils::memcpy_words,
    memory::Memory,
    types::*,
};

use super::INCREMENT_LIMIT;

pub struct EvacuateIncrement<'a, M: Memory> {
    mem: &'a mut M,
    steps: usize,
    partition_map: &'a mut PartitionMap,
    partition_index: &'a mut usize,
    sweep_address: &'a mut Option<usize>,
}

impl<'a, M: Memory + 'a> EvacuateIncrement<'a, M> {
    pub unsafe fn instance(mem: &'a mut M) -> EvacuateIncrement<'a, M> {
        if let Phase::Evacuate(state) = &mut PHASE {
            EvacuateIncrement {
                mem,
                steps: 0,
                partition_map: PARTITION_MAP.as_mut().unwrap(),
                partition_index: &mut state.partition_index,
                sweep_address: &mut state.sweep_address,
            }
        } else {
            panic!("Invalid phase");
        }
    }

    pub unsafe fn initiate_evacuations(&mut self) {
        self.partition_map.plan_evacuations();
    }

    pub unsafe fn run(&mut self) {
        while *self.partition_index < MAX_PARTITIONS {
            if self.current_partition().to_be_evacuated() {
                if self.sweep_address.is_none() {
                    *self.sweep_address = Some(self.current_partition().evacuation_start());
                }
                self.evacuate_partition();
                if self.steps > INCREMENT_LIMIT {
                    return;
                }
            }
            *self.partition_index += 1;
            *self.sweep_address = None;
        }
    }

    unsafe fn current_partition(&mut self) -> &Partition {
        self.partition_map.get_partition(*self.partition_index)
    }

    unsafe fn evacuate_partition(&mut self) {
        let end_address = self.current_partition().end_address();
        while self.sweep_address.unwrap() < end_address {
            let block = Value::from_ptr(self.sweep_address.unwrap());
            if block.is_obj() {
                let original = self.sweep_address.unwrap() as *mut Obj;
                if original.is_marked() {
                    self.evacuate_object(original);
                }
            }
            let size = block_size(self.sweep_address.unwrap());
            *self.sweep_address.as_mut().unwrap() += size.to_bytes().as_usize();
            assert!(self.sweep_address.unwrap() <= end_address);
            self.steps += 1;
            if self.steps > INCREMENT_LIMIT {
                return;
            }
        }
    }

    unsafe fn evacuate_object(&mut self, original: *mut Obj) {
        debug_assert!(original.tag() >= TAG_OBJECT && original.tag() <= TAG_NULL);
        assert!(!original.is_forwarded());
        assert!(original.is_marked());
        let size = block_size(original as usize);
        let new_address = self.mem.alloc_words(size);
        let copy = new_address.get_ptr() as *mut Obj;
        memcpy_words(copy as usize, original as usize, size);
        (*copy).forward = new_address;
        (*original).forward = new_address;
        assert!(!copy.is_forwarded());
        assert!(original.is_forwarded());
    }
}
