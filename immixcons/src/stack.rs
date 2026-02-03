use libc::{getcontext, pthread_attr_getstack};
use log::trace;
use std::hint::black_box;
use std::mem::{size_of, MaybeUninit};
use std::slice::from_raw_parts;

pub struct SystemStackInfo {
    base: usize,
}

impl SystemStackInfo {
    pub fn new() -> SystemStackInfo {
        let mut context = MaybeUninit::zeroed();
        let mut stack_base = MaybeUninit::zeroed();
        let mut stack_size = MaybeUninit::zeroed();

        let result = unsafe {
            pthread_attr_getstack(
                context.as_mut_ptr(),
                stack_base.as_mut_ptr(),
                stack_size.as_mut_ptr(),
            )
        };

        if result != 0 {
            panic!("pthread_attr_getstack() returned {}", result);
        }

        SystemStackInfo {
            base: unsafe { stack_base.assume_init() as usize },
        }
    }

    pub fn scan<F>(&self, results: &mut Vec<usize>, filter: F)
    where
        F: Fn(usize) -> bool,
    {
        // call getcontext to put all register values on to the stack
        let mut context = MaybeUninit::zeroed();
        let result = unsafe { getcontext(context.as_mut_ptr()) };
        if result != 0 {
            panic!("could not get thread context!");
        }

        // there's nothing guaranteeing the ordering of these function
        // local vars on the stack, compiler is free to break all this horribly
        let stack_top_marker: usize = 0xbeefd00d;

        let mut stack_top = (&stack_top_marker as *const usize).addr();
        let mut stack_base = self.base;

        let word_size = size_of::<usize>();

        // swap top and base when
        if stack_top < stack_base {
            (stack_top, stack_base) = (stack_base + word_size, stack_top);
        }

        let stack_len = (stack_top - stack_base) / word_size;
        let slice = unsafe { from_raw_parts(stack_base as *const usize, stack_len) };

        for stack_item in slice {
            trace!("[stack_scan] {:x}", *stack_item);

            if filter(*stack_item) {
                results.push(*stack_item);
            }
        }

        black_box(&context);
        black_box(&stack_top_marker);
    }
}

impl Default for SystemStackInfo {
    fn default() -> SystemStackInfo {
        SystemStackInfo::new()
    }
}
