use libc::{getcontext, pthread_attr_getstack, pthread_getattr_np, pthread_self};
use log::trace;
use std::mem::{size_of, MaybeUninit};
use std::pin::{pin, Pin};
use std::slice::from_raw_parts;

/// Stack growth orientation
/// It either grows up or down
#[derive(PartialEq)]
enum Grows {
    Up,
    Down,
}

/// System stack information for an OS thread.
/// For conservative stack scanning we need to know the stack base, which may
/// be a high address for downward growing stacks, or a low address for upward
/// growing stacks.  The growth orientation is noted.
pub struct SystemStackInfo {
    base: usize,
    orientation: Grows,
}

impl SystemStackInfo {
    pub fn new() -> SystemStackInfo {
        let mut attr = MaybeUninit::zeroed();
        let result = unsafe { pthread_getattr_np(pthread_self(), attr.as_mut_ptr()) };
        if result != 0 {
            panic!("pthread_getattr_np() returned {}", result);
        }

        let mut stack_base = MaybeUninit::zeroed();
        let mut stack_size = MaybeUninit::zeroed();

        let result = unsafe {
            pthread_attr_getstack(
                attr.as_mut_ptr(),
                stack_base.as_mut_ptr(),
                stack_size.as_mut_ptr(),
            )
        };

        if result != 0 {
            panic!("pthread_attr_getstack() returned {}", result);
        }

        let some_local: usize = 0;
        let orientation = Self::stack_orientation(&some_local);

        let stack_start = if orientation == Grows::Down {
            unsafe { stack_base.assume_init() as usize + stack_size.assume_init() as usize }
        } else {
            unsafe { stack_base.assume_init() as usize }
        };

        SystemStackInfo {
            base: stack_start,
            orientation: orientation,
        }
    }

    // Should be able to determine whether the stack grows up or down by comparing
    // a local arg to the address of a local arg in the caller's stack frame
    #[inline(never)]
    fn stack_orientation(arg: &usize) -> Grows {
        // stack might grow up or down: set the base to where the stack grows from
        let some_local: usize = 1;
        if (&some_local as *const usize).addr() < (arg as *const usize).addr() {
            Grows::Down
        } else {
            Grows::Up
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
        let stack_ptr_marker = pin!(context);

        let mut stack_ptr = (&stack_ptr_marker as *const Pin<_> as *const ()).addr();
        let mut stack_base = self.base;

        let word_size = size_of::<usize>();

        // swap top and base when stack is downward growing for correct slice
        // value ordering
        if self.orientation == Grows::Down {
            (stack_ptr, stack_base) = (stack_base, stack_ptr);
        }

        let stack_len = (stack_ptr - stack_base) / word_size;
        trace!("[stack_scan] top={:x} base={:x}", stack_ptr, stack_base);

        let slice = unsafe { from_raw_parts(stack_base as *const usize, stack_len) };

        for stack_item in slice {
            // take a copy of the value: we're doing things that may incur undefined
            // behavior by scanning a slice of the stack - the compiler can rearrange
            // things such that values in the slice might change unexpectedly
            let potential_ptr = *stack_item;

            if potential_ptr != 0 && filter(potential_ptr) {
                results.push(potential_ptr);
                trace!("[stack_scan] {:x}", potential_ptr);
            }
        }
    }
}

impl Default for SystemStackInfo {
    fn default() -> SystemStackInfo {
        SystemStackInfo::new()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_sanity() {
        let stack = SystemStackInfo::new();
        let mut stack_scan = Vec::new();

        let local: usize = 0xdeadbeef;

        // simply shouldn't cause any segfaults, bounds errors etc
        stack.scan(&mut stack_scan, |_| true);

        assert!(stack_scan.contains(&local));
    }
}
