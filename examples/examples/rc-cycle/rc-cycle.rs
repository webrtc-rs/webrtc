use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
struct Cycle {
    cell: RefCell<Option<Rc<Cycle>>>,
}

impl Drop for Cycle {
    fn drop(&mut self) {
        println!("freed");
    }
}

#[tokio::main]
async fn main() {
    let cycle = Rc::new(Cycle {
        cell: RefCell::new(None),
    });
    *cycle.cell.borrow_mut() = Some(cycle.clone());
}

// use nightly rust
// RUSTFLAGS="-Z sanitizer=leak" cargo build --example rc-cycle
// ./target/debug/example/rc-cycle
// =================================================================
// ==1457719==ERROR: LeakSanitizer: detected memory leaks
//
// Direct leak of 32 byte(s) in 1 object(s) allocated from:
//     #0 0x55d4688e1b58 in malloc /rustc/llvm/src/llvm-project/compiler-rt/lib/lsan/lsan_interceptors.cpp:56:3
//     #1 0x55d4689db6cb in alloc::alloc::alloc::h1ab42fe6949393de /rustc/e269e6bf47f40c9046cd44ab787881d700099252/library/alloc/src/alloc.rs:86:14
//
// SUMMARY: LeakSanitizer: 32 byte(s) leaked in 1 allocation(s).
