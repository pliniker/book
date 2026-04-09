use crate::safeptr::MutatorScope;
use crate::taggedptr::Value;
use crate::{headers::TypeList, memory::HeapStorage};
use immixcons::{AllocObject, AllocRaw, HeapTracer, RawPtr, TraceVisitor};
use log::trace;

/// This empty struct will provide scope guarded access to Trace::trace()
/// so that safe access can be assured.
pub struct GcScope;
impl MutatorScope for GcScope {}

/// This trait must be implemented by all types that are heap allocated
pub trait Trace {
    fn trace<V: TraceVisitor>(&self, v: &mut V, guard: &'_ dyn MutatorScope);
}

/// This struct wraps HeapTracer, provided by ImmixCons, with the additional
/// scope guard that enables safe tracing
pub struct TraceVisitorProxy<'guard> {
    guard: &'guard dyn MutatorScope,
    tracer: HeapTracer,
}

impl<'guard> TraceVisitorProxy<'guard> {
    pub fn new(guard: &'guard GcScope) -> TraceVisitorProxy<'guard> {
        TraceVisitorProxy {
            guard,
            tracer: HeapTracer::new(),
        }
    }
}

impl<'guard> TraceVisitor for TraceVisitorProxy<'guard> {
    fn visit(&mut self, object: RawPtr<()>) {
        trace!("[trace_visit] {:x}", object.addr());
        let header = HeapStorage::get_header(object);
        let object = unsafe { header.as_ref().get_object_fatptr() };
        let value = object.as_value(self.guard);
        match value {
            Value::Pair(p) => p.trace(&mut self.tracer, self.guard),
            //Value::Text(t) => t.trace(&mut self.tracer, self.guard),
            //Value::List(a) => a.trace(self, f),
            //Value::ArrayU8(a) => a.trace(self, f),
            //Value::ArrayU16(a) => a.trace(self, f),
            //Value::ArrayU32(a) => a.trace(self, f),
            //Value::Dict(d) => d.trace(self, f),
            //Value::Function(n) => n.trace(self, f),
            //Value::Partial(p) => p.trace(self, f),
            //Value::Upvalue(_) => write!(f, "Upvalue"),
            _ => unimplemented!(),
        }
    }

    fn pop(&mut self) -> Option<RawPtr<()>> {
        self.tracer.pop()
    }
}
