#![allow(dead_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use std::alloc::System;

struct CountingAllocator;

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

fn record_success(pointer: *mut u8) {
    if !pointer.is_null() {
        ACTIVE.with(|active| {
            if active.get() {
                COUNT.with(|count| count.set(count.get().saturating_add(1)));
            }
        });
    }
}

// SAFETY: every allocator operation delegates to `System` with its arguments unchanged.
// The additional bookkeeping only observes returned pointers and updates `Cell`s belonging
// to the calling thread; it neither dereferences pointers nor changes allocation lifetimes.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies `layout` under `GlobalAlloc::alloc`'s contract, and it
        // is forwarded unchanged to the system allocator.
        let pointer = unsafe { System.alloc(layout) };
        record_success(pointer);
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies `layout` under `GlobalAlloc::alloc_zeroed`'s contract,
        // and it is forwarded unchanged to the system allocator.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        record_success(pointer);
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the caller guarantees that `pointer` and `layout` identify a live allocation
        // accepted by this allocator; both are forwarded unchanged to `System`.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller supplies all arguments under `GlobalAlloc::realloc`'s contract,
        // and they are forwarded unchanged to the system allocator.
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        record_success(replacement);
        replacement
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct Measurement;

impl Measurement {
    fn start() -> Self {
        COUNT.with(|count| count.set(0));
        ACTIVE.with(|active| {
            assert!(!active.replace(true), "nested allocation measurement");
        });
        Self
    }
}

impl Drop for Measurement {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.set(false));
    }
}

pub fn allocations<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    let guard = Measurement::start();
    let result = operation();
    let count = COUNT.with(Cell::get);
    drop(guard);
    (result, count)
}

pub fn zero_allocations<T>(operation: impl FnOnce() -> T) -> T {
    let (result, count) = allocations(operation);
    assert_eq!(count, 0, "measured operation allocated {count} times");
    result
}

pub fn assert_instrumentation_works() {
    let (value, count) = allocations(|| Box::new(7_u8));
    assert!(
        count > 0,
        "counting allocator did not observe a known allocation"
    );
    drop(value);
}
