//! GrapheneOS's [hardened_malloc] as the Rust global allocator.
//!
//! ```
//! use mcsapi_hardened_malloc::HardenedMalloc;
//!
//! #[global_allocator]
//! static GLOBAL: HardenedMalloc = HardenedMalloc;
//!
//! fn main() {
//!     assert_eq!(mcsapi_hardened_malloc::binding(), mcsapi_hardened_malloc::Binding::Hardened);
//! }
//! ```
//!
//! The crate links the system's `libhardened_malloc.so` and builds no copy of
//! its own, so a process that the preload file already puts it in (LosOS
//! names this same library in `/etc/ld-nix.so.preload`) keeps one heap. A
//! process it is not preloaded into still gets it, for C libraries too: the
//! link puts it ahead of libc, so their `malloc` binds to it as well.
//!
//! Over std's [`System`], which would reach the same `malloc` by then,
//! [`HardenedMalloc`] adds sized deallocation. Rust knows every allocation's
//! size when it frees it, and this passes it to hardened_malloc's
//! `free_sized`, which aborts when it does not match the size class it
//! allocated from. A double
//! free or a free of the wrong pointer is caught by hardened_malloc either way;
//! a free with a corrupted length is only caught this way.
//!
//! [`sys`] is the raw C interface, for callers that need `malloc_object_size`
//! or `malloc_trim`.
//!
//! hardened_malloc needs a 64-bit target and, by default, a 48-bit address
//! space: it reserves 32 GiB per size class per arena up front, so an arm64
//! kernel built with 39-bit virtual addresses (most Android kernels) fails its
//! first allocation.
//!
//! [hardened_malloc]: https://github.com/GrapheneOS/hardened_malloc
//! [`System`]: https://doc.rust-lang.org/std/alloc/struct.System.html

#![no_std]

#[cfg(not(all(target_os = "linux", target_pointer_width = "64")))]
compile_error!("hardened_malloc supports 64-bit Linux only");

use core::alloc::{GlobalAlloc, Layout};
use core::ffi::{c_char, c_int, c_void};
use core::ptr;

/// The C interface `libhardened_malloc.so` exports, from upstream's
/// `include/h_malloc.h`.
///
/// The library exports the standard names, not `h_`-prefixed ones (that
/// prefix exists only for builds that define `H_MALLOC_PREFIX`), so these
/// bind to whichever object comes first in the process's symbol scope. The
/// link order makes that hardened_malloc, and [`binding`] checks it.
pub mod sys {
    use core::ffi::{c_int, c_void};

    unsafe extern "C" {
        pub fn malloc(size: usize) -> *mut c_void;
        pub fn calloc(nmemb: usize, size: usize) -> *mut c_void;
        pub fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
        pub fn aligned_alloc(alignment: usize, size: usize) -> *mut c_void;
        pub fn posix_memalign(memptr: *mut *mut c_void, alignment: usize, size: usize) -> c_int;
        pub fn free(ptr: *mut c_void);

        /// Frees `ptr`, aborting unless `expected_size` falls in the size
        /// class `ptr` was allocated from (C23 `free_sized`).
        pub fn free_sized(ptr: *mut c_void, expected_size: usize);

        pub fn malloc_usable_size(ptr: *mut c_void) -> usize;
        /// The bytes left from `ptr` to the end of its allocation, or
        /// `usize::MAX` when hardened_malloc cannot tell. Checks the
        /// allocator's metadata, so it can abort on a corrupted heap.
        pub fn malloc_object_size(ptr: *const c_void) -> usize;
        /// [`malloc_object_size`] without the metadata checks.
        pub fn malloc_object_size_fast(ptr: *const c_void) -> usize;

        /// Returns freed slabs and quarantined regions to the kernel.
        pub fn malloc_trim(pad: usize) -> c_int;
        /// Only `M_PURGE` does anything; it is `malloc_trim` plus the
        /// thread caches.
        pub fn mallopt(param: c_int, value: c_int) -> c_int;
    }

    /// `mallopt`'s purge request, from h_malloc.h.
    pub const M_PURGE: c_int = -101;
}

/// hardened_malloc's `min_align` (h_malloc.c): every size class is a multiple
/// of 16, so `malloc` covers any alignment up to it.
const MIN_ALIGN: usize = 16;

/// hardened_malloc as a [`GlobalAlloc`].
///
/// Allocations aligned to 16 bytes or less go through `malloc`, `calloc` and
/// `realloc` and are freed with `free_sized`. Larger alignments go through
/// `posix_memalign` and plain `free`: hardened_malloc moves those to a bigger
/// size class than their length, which `free_sized` would reject.
#[derive(Clone, Copy, Debug, Default)]
pub struct HardenedMalloc;

unsafe impl GlobalAlloc for HardenedMalloc {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.align() <= MIN_ALIGN {
            unsafe { sys::malloc(layout.size()).cast() }
        } else {
            unsafe { aligned(layout) }
        }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if layout.align() <= MIN_ALIGN {
            // calloc knows fresh slots are already zero (hardened_malloc
            // zeroes on free) and skips the memset for them.
            unsafe { sys::calloc(1, layout.size()).cast() }
        } else {
            let p = unsafe { aligned(layout) };
            if !p.is_null() {
                unsafe { ptr::write_bytes(p, 0, layout.size()) };
            }
            p
        }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if layout.align() <= MIN_ALIGN {
            unsafe { sys::free_sized(ptr.cast(), layout.size()) }
        } else {
            unsafe { sys::free(ptr.cast()) }
        }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if layout.align() <= MIN_ALIGN {
            // The block's size class follows new_size, which is what Rust
            // passes to dealloc from now on.
            return unsafe { sys::realloc(ptr.cast(), new_size).cast() };
        }
        // C has no aligned realloc.
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        let new = unsafe { self.alloc(new_layout) };
        if !new.is_null() {
            unsafe {
                ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size));
                self.dealloc(ptr, layout);
            }
        }
        new
    }
}

unsafe fn aligned(layout: Layout) -> *mut u8 {
    let mut p = ptr::null_mut();
    // posix_memalign rather than aligned_alloc: it takes any power of two
    // from the pointer size up, and Layout guarantees a power of two.
    let align = layout.align().max(size_of::<usize>());
    match unsafe { sys::posix_memalign(&mut p, align, layout.size()) } {
        0 => p.cast(),
        _ => ptr::null_mut(),
    }
}

/// Where the process's allocator symbols bind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    /// Every symbol [`HardenedMalloc`] calls resolves into the object that
    /// defines `malloc_object_size`, which only hardened_malloc exports.
    Hardened,
    /// At least one resolves elsewhere: another allocator was preloaded
    /// ahead of hardened_malloc, so the Rust heap would mix two allocators.
    Mixed,
    /// hardened_malloc is not loaded at all. A program that links this crate
    /// but never calls into it gets this: the linker drops a library nothing
    /// references.
    Absent,
}

#[repr(C)]
struct DlInfo {
    dli_fname: *const c_char,
    dli_fbase: *mut c_void,
    dli_sname: *const c_char,
    dli_saddr: *mut c_void,
}

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dladdr(addr: *const c_void, info: *mut DlInfo) -> c_int;
    fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
    fn abort() -> !;
}

/// glibc's `RTLD_DEFAULT`: look a symbol up in the global scope, the way the
/// calls above are bound.
const RTLD_DEFAULT: *mut c_void = ptr::null_mut();

fn object_of(symbol: &core::ffi::CStr) -> *mut c_void {
    let mut info = DlInfo {
        dli_fname: ptr::null(),
        dli_fbase: ptr::null_mut(),
        dli_sname: ptr::null(),
        dli_saddr: ptr::null_mut(),
    };
    unsafe {
        let addr = dlsym(RTLD_DEFAULT, symbol.as_ptr());
        if addr.is_null() || dladdr(addr, &mut info) == 0 {
            return ptr::null_mut();
        }
    }
    info.dli_fbase
}

/// Checks where `malloc` and the rest bind in this process.
pub fn binding() -> Binding {
    let hardened = object_of(c"malloc_object_size");
    let all = [
        c"malloc",
        c"calloc",
        c"realloc",
        c"posix_memalign",
        c"free",
        c"free_sized",
    ];
    if hardened.is_null() {
        Binding::Absent
    } else if all.iter().all(|s| object_of(s) == hardened) {
        Binding::Hardened
    } else {
        Binding::Mixed
    }
}

/// Runs [`binding`] before `main` in every program that links this crate, and
/// aborts if it is [`Binding::Mixed`]: a heap split between two allocators
/// crashes later, somewhere unrelated, and silently loses the hardening until
/// then. LD_PRELOAD of another allocator (jemalloc, a leak checker) is how a
/// process gets there. [`Binding::Absent`] is fine: nothing calls in.
extern "C" fn check_binding() {
    if binding() == Binding::Mixed {
        const MSG: &[u8] = b"mcsapi-hardened-malloc: malloc does not bind to hardened_malloc \
            (another allocator is preloaded ahead of it); refusing to run with two heaps\n";
        unsafe {
            write(2, MSG.as_ptr().cast(), MSG.len());
            abort();
        }
    }
}

#[used]
#[unsafe(link_section = ".init_array")]
static CHECK_BINDING: extern "C" fn() = check_binding;
