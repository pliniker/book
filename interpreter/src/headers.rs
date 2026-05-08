/// Defines an `ObjectHeader` type to immediately preceed each heap allocated
/// object, which also contains a type tag but with space for many more types.
use immixcons::{
    AllocHeader, AllocObject, AllocRaw, AllocTypeId, ArraySize, Mark, RawPtr, SizeClass,
    TraceVisitor,
};

use crate::array::{ArrayU16, ArrayU32, ArrayU8};
use crate::bytecode::{ArrayOpcode, ByteCode, InstructionStream};
use crate::dict::Dict;
use crate::function::{Function, Partial};
use crate::list::List;
use crate::memory::HeapStorage;
use crate::number::NumberObject;
use crate::pair::Pair;
use crate::pointerops::{Tagged, TAG_MASK};
use crate::safeptr::MutatorScope;
use crate::symbol::Symbol;
use crate::taggedptr::{FatPtr, Value};
use crate::text::Text;
use crate::vm::{CallFrameList, Thread, Upvalue};
use std::cell::Cell;

/// Recognized heap-allocated types.
/// This should represent every type native to the runtime with the exception of tagged pointer inline value
/// types.
// ANCHOR: DefTypeList
#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TypeList {
    ArrayBackingBytes,
    ArrayOpcode,
    ArrayU8,
    ArrayU16,
    ArrayU32,
    ByteCode,
    CallFrameList,
    Dict,
    Function,
    InstructionStream,
    List,
    NumberObject,
    Pair,
    Partial,
    Symbol,
    Text,
    Thread,
    Upvalue,
}

// Mark this as a Stickyimmix type-identifier type
impl AllocTypeId for TypeList {}
// ANCHOR_END: DefTypeList

/// A heap-allocated object header
// ANCHOR: DefObjectHeader
pub struct ObjectHeader {
    mark: Cell<Mark>,
    size_class: SizeClass,
    type_id: TypeList,
    size_bytes: u32,
}
// ANCHOR_END: DefObjectHeader

impl ObjectHeader {
    /// Convert the ObjectHeader address to a FatPtr pointing at the object itself.
    // NOTE Any type that is a runtime dynamic type must be added to the below list
    // NOTE Be careful to match the correct TypeList discriminant with it's corresponding FatPtr discriminant
    // NOTE Be careful to untag the pointer before putting it into a `FatPtr`
    // ANCHOR: DefObjectHeaderGetObjectFatPtr
    pub unsafe fn get_object_fatptr(&self) -> FatPtr {
        let ptr_to_self = RawPtr::new(self);
        let object_addr = HeapStorage::get_object(ptr_to_self);

        match self.type_id {
            // Special case ArrayBackingBytes isn't directly converted because its a base
            // type for other type implementations
            TypeList::ArrayBackingBytes => FatPtr::Nil,
            TypeList::ArrayU8 => FatPtr::ArrayU8(RawPtr::untag(object_addr.cast::<ArrayU8>())),
            TypeList::ArrayU16 => FatPtr::ArrayU16(RawPtr::untag(object_addr.cast::<ArrayU16>())),
            TypeList::ArrayU32 => FatPtr::ArrayU32(RawPtr::untag(object_addr.cast::<ArrayU32>())),
            TypeList::ByteCode => FatPtr::ByteCode(RawPtr::untag(object_addr.cast::<ByteCode>())),
            TypeList::CallFrameList => {
                FatPtr::CallFrameList(RawPtr::untag(object_addr.cast::<CallFrameList>()))
            }
            TypeList::Dict => FatPtr::Dict(RawPtr::untag(object_addr.cast::<Dict>())),
            TypeList::Function => FatPtr::Function(RawPtr::untag(object_addr.cast::<Function>())),
            TypeList::InstructionStream => {
                FatPtr::InstructionStream(RawPtr::untag(object_addr.cast::<InstructionStream>()))
            }
            TypeList::List => FatPtr::List(RawPtr::untag(object_addr.cast::<List>())),
            TypeList::NumberObject => {
                FatPtr::NumberObject(RawPtr::untag(object_addr.cast::<NumberObject>()))
            }
            TypeList::Pair => FatPtr::Pair(RawPtr::untag(object_addr.cast::<Pair>())),
            TypeList::Partial => FatPtr::Partial(RawPtr::untag(object_addr.cast::<Partial>())),
            TypeList::Symbol => FatPtr::Symbol(RawPtr::untag(object_addr.cast::<Symbol>())),
            TypeList::Text => FatPtr::Text(RawPtr::untag(object_addr.cast::<Text>())),
            TypeList::Upvalue => FatPtr::Upvalue(RawPtr::untag(object_addr.cast::<Upvalue>())),
            TypeList::Thread => FatPtr::Thread(RawPtr::untag(object_addr.cast::<Thread>())),

            // Other types not represented by FatPtr are an error to id here
            _ => panic!("Invalid ObjectHeader type tag {:?}!", self.type_id),
        }
    }
    // ANCHOR_END: DefObjectHeaderGetObjectFatPtr
}

impl AllocHeader for ObjectHeader {
    type TypeId = TypeList;

    fn new<O: AllocObject<Self::TypeId>>(
        size: u32,
        size_class: SizeClass,
        mark: Mark,
    ) -> ObjectHeader {
        ObjectHeader {
            mark: Cell::new(mark),
            size_class,
            type_id: O::TYPE_ID,
            size_bytes: size,
        }
    }

    fn new_array(size: ArraySize, size_class: SizeClass, mark: Mark) -> ObjectHeader {
        ObjectHeader {
            mark: Cell::new(mark),
            size_class,
            type_id: TypeList::ArrayBackingBytes,
            size_bytes: size,
        }
    }

    fn mark(&self, value: Mark) {
        self.mark.set(value);
    }

    fn mark_is(&self, value: Mark) -> bool {
        self.mark.get() == value
    }

    fn size_class(&self) -> SizeClass {
        self.size_class
    }

    fn size(&self) -> u32 {
        self.size_bytes
    }

    fn type_id(&self) -> TypeList {
        self.type_id
    }

    const TAG_MASK: usize = !TAG_MASK;
}

/// Apply the type ID to each native type
macro_rules! declare_allocobject {
    ($T:ty, $I:tt) => {
        impl AllocObject<TypeList> for $T {
            const TYPE_ID: TypeList = TypeList::$I;
        }
    };
}

declare_allocobject!(ArrayOpcode, ArrayOpcode);
declare_allocobject!(ArrayU8, ArrayU8);
declare_allocobject!(ArrayU16, ArrayU16);
declare_allocobject!(ArrayU32, ArrayU32);
declare_allocobject!(ByteCode, ByteCode);
declare_allocobject!(CallFrameList, CallFrameList);
declare_allocobject!(Dict, Dict);
declare_allocobject!(Function, Function);
declare_allocobject!(InstructionStream, InstructionStream);
declare_allocobject!(List, List);
declare_allocobject!(NumberObject, NumberObject);
declare_allocobject!(Pair, Pair);
declare_allocobject!(Partial, Partial);
declare_allocobject!(Symbol, Symbol);
declare_allocobject!(Text, Text);
declare_allocobject!(Thread, Thread);
declare_allocobject!(Upvalue, Upvalue);
