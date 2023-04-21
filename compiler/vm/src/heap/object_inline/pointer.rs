use super::{InlineObject, InlineObjectTrait};
use crate::{
    heap::{object_heap::HeapObject, Heap},
    utils::{impl_debug_display_via_debugdisplay, impl_eq_hash_via_get, DebugDisplay},
};
use derive_more::Deref;
use rustc_hash::FxHashMap;
use std::{
    fmt::{self, Formatter},
    ptr::NonNull,
};

#[derive(Clone, Copy, Deref)]
pub struct InlinePointer(InlineObject);
impl InlinePointer {
    pub fn new_unchecked(object: InlineObject) -> Self {
        Self(object)
    }

    pub fn get(self) -> HeapObject {
        let pointer = unsafe { NonNull::new_unchecked(self.0 .0 as *mut u64) };
        HeapObject(pointer)
    }
}

impl DebugDisplay for InlinePointer {
    fn fmt(&self, f: &mut Formatter, is_debug: bool) -> fmt::Result {
        self.get().fmt(f, is_debug)
    }
}
impl_debug_display_via_debugdisplay!(InlinePointer);

impl_eq_hash_via_get!(InlinePointer);

impl From<HeapObject> for InlinePointer {
    fn from(value: HeapObject) -> Self {
        Self(value.into())
    }
}
impl From<HeapObject> for InlineObject {
    fn from(value: HeapObject) -> Self {
        let address = value.address().addr().get() as u64;
        debug_assert_eq!(address & Self::KIND_MASK, Self::KIND_POINTER);
        Self(address)
    }
}

impl InlineObjectTrait for InlinePointer {
    fn clone_to_heap_with_mapping(
        self,
        heap: &mut Heap,
        address_map: &mut FxHashMap<HeapObject, HeapObject>,
    ) -> Self {
        self.get()
            .clone_to_heap_with_mapping(heap, address_map)
            .into()
    }
}
