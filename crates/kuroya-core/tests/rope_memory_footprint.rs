//! Measures the real memory footprint of a 500k-line buffer in Kuroya's
//! storage layer (rope-backed TextBuffer), using a counting global allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            if new_size > layout.size() {
                ALLOCATED.fetch_add(new_size - layout.size(), Ordering::Relaxed);
            } else {
                ALLOCATED.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        new_ptr
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn allocated_now() -> usize {
    ALLOCATED.load(Ordering::Relaxed)
}

#[test]
fn rope_buffer_memory_footprint_for_500k_lines() {
    // Build a realistic 500k-line source file (~48 MB on disk).
    let mut content = String::with_capacity(50_000_000);
    for i in 0..500_000 {
        content.push_str(&format!(
            "fn demo_{i}() {{ let value = {i} * 2; // some comment text to give the line realistic width {}}}\n",
            i % 97
        ));
    }
    let text_bytes = content.len();

    // Clone so the source string stays alive: the delta then measures the
    // buffer's own (gross) allocations, not a move-and-free wash.
    let before = allocated_now();
    let buffer = kuroya_core::TextBuffer::from_text(1, None, content.clone());
    let after = allocated_now();

    let delta_mb = (after.saturating_sub(before)) as f64 / 1_048_576.0;
    let text_mb = text_bytes as f64 / 1_048_576.0;
    let overhead_ratio = delta_mb / text_mb;

    println!(
        "500k-line buffer: text {text_mb:.1} MB, buffer allocated {delta_mb:.1} MB \
         (overhead {overhead_ratio:.2}x)"
    );

    // The rope should stay close to the raw text size: well under 2x, which
    // keeps a 48 MB source file's footprint in the tens of MB, not hundreds.
    assert!(
        delta_mb < text_mb * 2.0,
        "rope overhead too high: {delta_mb:.1} MB for {text_mb:.1} MB of text"
    );
    assert_eq!(buffer.text().len(), text_bytes);
}
