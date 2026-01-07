use std::slice::from_raw_parts;
use std::mem::MaybeUninit;
use libc::{getcontext, pthread_attr_getstack};

pub struct SystemStackInfo {
    base: usize,
}

impl SystemStackInfo {
    pub fn new() -> SystemStackInfo {
        let mut context = MaybeUninit::zeroed();
        let mut stack_base = MaybeUninit::zeroed();
        let mut stack_size = MaybeUninit::zeroed();

        let result = unsafe { pthread_attr_getstack(context.as_mut_ptr(), stack_base.as_mut_ptr(), stack_size.as_mut_ptr()) };

        if result != 0 {
            panic!("could not get thread attributes!");
        }

        SystemStackInfo { base: unsafe { stack_size.assume_init() } } 
    }

    fn scan(&self) {
        let mut context = MaybeUninit::zeroed();
        let result = unsafe { getcontext(context.as_mut_ptr()) };
        if result != 0 {
            panic!("could not get thread context!");
        }

        let stack_top_marker: usize = 0xbeefd00d;

        let mut stack_top = (&stack_top_marker as *const usize).addr();
        let mut stack_base = (&self.base as *const usize).addr();

        let word_size = size_of::<usize>();

        if stack_top < stack_base {
            (stack_top, stack_base) = (stack_base + word_size, stack_top);
        }

        let stack_len = (stack_top - stack_base) / word_size;
        let slice = unsafe { from_raw_parts(stack_base as *const usize, stack_len) };

/*      TODO implement remainder of scan
        let stack_scan = &mut self.inner.borrow_mut().scan;

        for stack_item in slice {
            // if *stack_item != 0 {
            //     println!("[stack] {:x}", *stack_item);
            // }
            stack_scan.push(StackItem::new(*stack_item));
        }

        black_box(&context);
*/
    }
}

impl Default for SystemStackInfo {
    fn default() -> SystemStackInfo {
        SystemStackInfo::new()
    }
}
