//! The crate as a test binary's global allocator: every allocation the test
//! harness and std make goes through it.

use std::alloc::{GlobalAlloc, Layout};
use std::process::Command;

use mcsapi_hardened_malloc::{Binding, HardenedMalloc, sys};

#[global_allocator]
static GLOBAL: HardenedMalloc = HardenedMalloc;

#[test]
fn binds_to_hardened_malloc() {
    assert_eq!(mcsapi_hardened_malloc::binding(), Binding::Hardened);
}

#[test]
fn allocations_are_hardened_mallocs() {
    let v: Box<[u8]> = Box::from(&[7u8; 100][..]);
    // malloc_object_size only knows hardened_malloc's own pointers; anything
    // else answers usize::MAX. 100 bytes plus the canary land in the 112 class.
    let size = unsafe { sys::malloc_object_size(v.as_ptr().cast()) };
    assert!((100..usize::MAX).contains(&size), "{size}");
}

#[test]
fn grows_shrinks_and_zeroes() {
    let mut v: Vec<u64> = Vec::new();
    for i in 0..200_000 {
        v.push(i);
    }
    assert_eq!(v.iter().sum::<u64>(), 199_999 * 200_000 / 2);
    v.truncate(3);
    v.shrink_to_fit();
    assert_eq!(v, [0, 1, 2]);

    let z = vec![0u8; 1 << 20];
    assert!(z.iter().all(|&b| b == 0));
}

#[test]
fn over_aligned() {
    #[repr(align(4096))]
    struct Page([u8; 4096]);
    impl Page {
        fn last(&self) -> u8 {
            self.0[4095]
        }
    }
    #[repr(align(64))]
    #[derive(Clone)]
    struct Line([u8; 8]);

    let p = Box::new(Page([1; 4096]));
    assert_eq!((&*p as *const Page as usize) % 4096, 0);
    assert_eq!(p.last(), 1);

    let mut lines = vec![Line([2; 8]); 3];
    for _ in 0..1000 {
        lines.push(Line([3; 8]));
    }
    assert_eq!(lines.as_ptr() as usize % 64, 0);
    assert_eq!(lines[1002].0, [3; 8]);

    // A 1 MiB block with 2 MiB alignment goes down hardened_malloc's large path.
    let layout = Layout::from_size_align(1 << 20, 2 << 20).unwrap();
    unsafe {
        let big = GLOBAL.alloc_zeroed(layout);
        assert!(!big.is_null());
        assert_eq!(big as usize % (2 << 20), 0);
        assert_eq!(*big.add((1 << 20) - 1), 0);
        let big = GLOBAL.realloc(big, layout, 3 << 20);
        assert_eq!(big as usize % (2 << 20), 0);
        GLOBAL.dealloc(big, Layout::from_size_align(3 << 20, 2 << 20).unwrap());
    }
}

#[test]
fn threads() {
    let handles: Vec<_> = (0..8)
        .map(|t| {
            std::thread::spawn(move || {
                let mut strings = Vec::new();
                for i in 0..10_000 {
                    strings.push(format!("{t}-{i}"));
                }
                strings.len()
            })
        })
        .collect();
    assert!(handles.into_iter().all(|h| h.join().unwrap() == 10_000));
}

/// Freeing with a length from the wrong size class is what `free_sized` adds
/// over `free`: hardened_malloc aborts the process. Run in a child so the
/// abort is observable.
#[test]
fn wrong_size_free_aborts() {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "wrong_size_free_child",
            "--ignored",
            "--nocapture",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("sized deallocation mismatch"), "{stderr}");
}

#[test]
#[ignore = "run by wrong_size_free_aborts"]
fn wrong_size_free_child() {
    unsafe {
        let p = GLOBAL.alloc(Layout::from_size_align(24, 8).unwrap());
        GLOBAL.dealloc(p, Layout::from_size_align(4000, 8).unwrap());
    }
}
