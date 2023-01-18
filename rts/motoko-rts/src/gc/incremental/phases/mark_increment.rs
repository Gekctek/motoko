use crate::{
    gc::incremental::{
        array_slicing::slice_array,
        mark_stack::MarkStack,
        partitioned_heap::PartitionedHeap,
        roots::{visit_roots, Roots},
        time::BoundedTime,
        MarkState,
    },
    memory::Memory,
    types::*,
    visitor::visit_pointer_fields,
};

pub struct MarkIncrement<'a, M: Memory> {
    mem: &'a mut M,
    time: &'a mut BoundedTime,
    heap: &'a mut PartitionedHeap,
    mark_stack: &'a mut MarkStack,
    complete: &'a mut bool,
}

impl<'a, M: Memory + 'a> MarkIncrement<'a, M> {
    pub unsafe fn instance(
        mem: &'a mut M,
        time: &'a mut BoundedTime,
        state: &'a mut MarkState,
        heap: &'a mut PartitionedHeap,
    ) -> MarkIncrement<'a, M> {
        MarkIncrement {
            mem,
            time,
            heap,
            mark_stack: &mut state.mark_stack,
            complete: &mut state.complete,
        }
    }

    pub unsafe fn mark_roots(&mut self, roots: Roots) {
        visit_roots(roots, self.heap.base_address(), self, |gc, field| {
            gc.mark_object(*field);
            gc.time.tick();
        });
    }

    pub unsafe fn run(&mut self) {
        if *self.complete {
            // Allocation after complete marking: Wait until the next empty call stack increment.
            debug_assert!(self.mark_stack.is_empty());
            return;
        }
        while let Some(value) = self.mark_stack.pop() {
            debug_assert!(value.is_ptr());
            debug_assert!(value.as_obj().is_marked());
            self.mark_fields(value.as_obj());

            self.time.tick();
            if self.time.is_over() {
                return;
            }
        }
        self.complete_marking();
    }

    pub unsafe fn mark_object(&mut self, value: Value) {
        self.time.tick();
        debug_assert!(!*self.complete);
        debug_assert!((value.get_ptr() >= self.heap.base_address()));
        debug_assert!(!value.is_forwarded());
        let object = value.as_obj();
        if object.is_marked() {
            return;
        }
        object.mark();
        self.heap.record_marked_space(object);
        debug_assert!(
            object.tag() >= crate::types::TAG_OBJECT && object.tag() <= crate::types::TAG_NULL
        );
        self.mark_stack.push(self.mem, value);
    }

    unsafe fn mark_fields(&mut self, object: *mut Obj) {
        debug_assert!(object.is_marked());
        visit_pointer_fields(
            self,
            object,
            object.tag(),
            self.heap.base_address(),
            |gc, field_address| {
                let field_value = *field_address;
                gc.mark_object(field_value);
            },
            |gc, _, array| {
                debug_assert!(array.is_marked());
                let length = slice_array(array);
                if array.tag() >= TAG_ARRAY_SLICE_MIN {
                    gc.mark_stack.push(gc.mem, Value::from_ptr(array as usize));
                }
                gc.time.advance(length as usize);
                length
            },
        );
    }

    unsafe fn complete_marking(&mut self) {
        debug_assert!(!*self.complete);
        *self.complete = true;

        #[cfg(debug_assertions)]
        self.mark_stack.assert_is_garbage();
    }
}
