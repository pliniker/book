use crate::memory::HeapStorage;
use crate::safeptr::MutatorScope;
use crate::taggedptr::Value;
use immixcons::{AllocRaw, HeapTracer, RawPtr, TraceVisitor};
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
        let header = HeapStorage::get_header(object);
        let typed_object = unsafe { header.as_ref().get_object_fatptr() };
        let value = typed_object.as_value(self.guard);

        trace!("[trace_visit] {:x} {:?}", object.addr(), value);

        match value {
            Value::ArrayOpcode(a) => a.trace(&mut self.tracer, self.guard),
            Value::ArrayU8(a) => a.trace(&mut self.tracer, self.guard),
            Value::ArrayU16(a) => a.trace(&mut self.tracer, self.guard),
            Value::ArrayU32(a) => a.trace(&mut self.tracer, self.guard),
            Value::ByteCode(a) => a.trace(&mut self.tracer, self.guard),
            Value::CallFrameList(a) => a.trace(&mut self.tracer, self.guard),
            Value::Dict(d) => d.trace(&mut self.tracer, self.guard),
            Value::Function(f) => f.trace(&mut self.tracer, self.guard),
            Value::InstructionStream(i) => i.trace(&mut self.tracer, self.guard),
            Value::List(a) => a.trace(&mut self.tracer, self.guard),
            Value::Nil => (),
            Value::Number(_) => (),
            Value::NumberObject(_) => panic!("no number objects"),
            Value::Pair(p) => p.trace(&mut self.tracer, self.guard),
            Value::Symbol(_) => panic!("should never be tracing symbols!"),
            Value::Thread(t) => t.trace(&mut self.tracer, self.guard),
            Value::Partial(p) => p.trace(&mut self.tracer, self.guard),
            Value::Text(_) => panic!("no text"),
            Value::Upvalue(v) => v.trace(&mut self.tracer, self.guard),
        }
    }

    fn pop(&mut self) -> Option<RawPtr<()>> {
        self.tracer.pop()
    }
}
