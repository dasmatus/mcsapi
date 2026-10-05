// Links the system's libhardened_malloc.so; nothing here compiles a copy.
//
// hardened_malloc reserves its whole address space up front and keeps its
// metadata in that reservation, so two copies in one process (a static one
// here and the one /etc/ld.so.preload puts in every process) would be two
// separate heaps, and a pointer from one freed through the other aborts. With
// the shared library, a process the system already preloads it into maps the
// same file once, and a process it doesn't gets it through this link.

use std::env;

fn main() {
    // Where libhardened_malloc.so is, when the linker would not find it
    // itself: CI builds it from source, and Nix's cc wrapper already passes
    // -L for the package.
    println!("cargo::rerun-if-env-changed=HARDENED_MALLOC_LIB_DIR");
    if let Some(dir) = env::var_os("HARDENED_MALLOC_LIB_DIR") {
        println!(
            "cargo::rustc-link-search=native={}",
            dir.to_str().expect("HARDENED_MALLOC_LIB_DIR is not UTF-8")
        );
    }

    let lib = if env::var_os("CARGO_FEATURE_LIGHT").is_some() {
        "hardened_malloc-light"
    } else {
        "hardened_malloc"
    };
    // dylib, and named before libc on the link line because std's own native
    // libraries come last. That puts it ahead of libc.so.6 in DT_NEEDED and so
    // in the global symbol scope: every malloc in the process, C libraries'
    // included, binds to it, not only the Rust heap.
    println!("cargo::rustc-link-lib=dylib={lib}");
}
