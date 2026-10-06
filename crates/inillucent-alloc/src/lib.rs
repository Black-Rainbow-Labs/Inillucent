//! A size-classed free list over the system allocator.
//!
//! Invariant: **it recycles rather than accumulating.** A bump arena that never
//! frees is the shortest thing to write and the wrong thing to ship: thirty
//! rounds of the gate's plan would grow it without bound, and what it measured
//! would be page faults rather than allocation. This keeps one intrusive list
//! per size class, capped, and hands a block back to the system allocator when
//! the class is full.
//!
//! ## Why the engine has one at all
//!
//! Because allocation is where a compile goes. A trivial compile was measured
//! at **25 heap allocations, with the Windows C runtime heap at 59% of the
//! time**, and a size-classed free list at **17% overall** on the same
//! plan - which is why Phase 3's Part E names it the cheapest first move rather
//! than one of the several structural changes beside it. The same shape is
//! visible outside compilation: `CREATE INDEX` over a hundred thousand rows
//! builds two allocations per row just to hold the key and the rowid, and the
//! gate's `schema.index` spends more time in its scan than SQLite spends on the
//! whole statement.
//!
//! What it removes is exactly what was in question: the size lookup, the
//! locking and the per-call bookkeeping the system allocator does. It does not
//! try to be a better allocator in general - above [`LARGEST`] and for any
//! alignment the system's own guarantee does not cover, the request is
//! forwarded unchanged.
//!
//! ## Why it never allocates
//!
//! An allocator that allocates re-enters itself, and a re-entrant allocator is
//! a deadlock or a stack overflow waiting for the right allocation pattern. So
//! the free lists are **intrusive**: a freed block holds the pointer to the
//! next free block of its class in its own first eight bytes, and the heads
//! live in a fixed-size array of `Cell`s in thread-local storage. Nothing here
//! calls `Vec`, `Box`, or anything that could.
//!
//! ## Why it is a crate of its own
//!
//! Because every other production crate in this workspace carries
//! `#![forbid(unsafe_code)]`, and an allocator cannot. Putting it here keeps
//! that true everywhere it is true today and confines the unsafe to one file
//! that has nothing else in it - no dependencies, first-party or otherwise, so
//! there is nothing it could re-enter itself through.
//!
//! ## Why it is per thread
//!
//! Because a shared list needs a lock, and the lock is most of what this exists
//! to remove. A block allocated on one thread and freed on another goes onto
//! the freeing thread's list, which is safe - the block is memory of a known
//! class, and the class is derived from the layout the caller hands back - and
//! at worst moves a block between threads. The per-class cap bounds what that
//! can cost.

#![deny(missing_docs)]
// **The one production crate in this workspace allowed to write `unsafe`, and
// it was outside every check until task-1932 (H9).** It was not in `GOVERNED`
// and not in `UNSAFE_CRATES`, which is not the same as being permitted: it
// means nothing read it. A `GlobalAlloc` is an unsafe trait and this crate is
// the boundary, so the four lints below are what say that the *rest* of it -
// the size-class arithmetic, the caps, the thread-local lists - is ordinary
// safe code held to the same standard as the engine.
#![deny(clippy::indexing_slicing)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

/// The largest allocation the free list handles itself.
///
/// Above this, up to 64 KiB, a block comes from one of four page sized lists
/// shared by every thread; see [`big_class_of`]. Above 64 KiB the system
/// allocator is asked directly.
pub const LARGEST: usize = 4_096;

/// The granularity of a size class, which is also the alignment every pooled
/// block is made with.
const GRAIN: usize = 16;

/// How many size classes the free list holds.
///
/// One per sixteen bytes up to [`LARGEST`], which is the granularity a `Vec<u8>`
/// of a row, a key or a name actually lands on.
const CLASSES: usize = LARGEST / GRAIN + 1;

/// How many **bytes** one class keeps before handing the rest back.
///
/// The cap is what makes this a recycler rather than a leak: a workload that
/// allocates a million blocks of one class and frees them all keeps a bounded
/// amount and returns the rest, so the process's footprint is bounded by the
/// classes rather than by the workload.
///
/// **Bytes rather than blocks.** The cap was a thousand and
/// twenty-four *blocks* per class - a fixed count over classes whose sizes
/// differ by two hundred and fifty-six times, so the same number meant sixteen
/// kilobytes in the smallest class and four megabytes in the largest. The
/// gate's write family paid for it: one round left 5.2 MiB of heap standing
/// that nothing live was using, and the process's high-water mark is exactly
/// what this ticket's memory bar reads.
///
/// Sixteen kilobytes per class keeps the small classes as deep as they were -
/// the sixteen-byte class still holds its thousand and twenty-four blocks,
/// which is where the free list's measured speed comes from - and bounds the
/// largest at four. The whole cache is at most `CLASSES * 16 KiB`, about
/// 4 MiB, rather than an unbounded function of which classes a workload
/// happened to touch.
const PER_CLASS_BYTES: usize = 64 << 10;

/// The block cap that was here before, kept as a ceiling.
///
/// **The byte cap only ever takes retention away.** Applying it alone would
/// have made the smallest class keep four thousand blocks where it used to keep
/// a thousand - more, not less - and the small classes are exactly where the
/// free list's measured speed comes from. Keeping the old count as a ceiling
/// means every class holds *at most* what it held before, and the large ones
/// hold far less.
const PER_CLASS_BLOCKS: usize = 1_024;

/// How many blocks of each class the free list keeps, worked out once.
///
/// **A table rather than the arithmetic, because `dealloc` is the hot path.**
/// The cap is a division and a pair of clamps, and computing it per free put a
/// divide on every deallocation the program makes. It is a constant of the
/// class, so it is a constant of the build.
const CAPS: [usize; CLASSES] = caps();

/// Builds [`CAPS`] at compile time.
///
/// The lower of the two caps, and at least one so no class is barred from
/// recycling by arithmetic. At 64 KiB and a 1,024-block ceiling: the 16-byte
/// class keeps its full thousand and twenty-four, unchanged, and the
/// 4,096-byte class keeps sixteen where it used to keep a thousand - which is
/// four megabytes of one class's free list that a write workload was leaving
/// standing.
#[allow(clippy::indexing_slicing)]
const fn caps() -> [usize; CLASSES] {
    let mut caps = [1usize; CLASSES];
    let mut class = 0usize;
    while class < CLASSES {
        let size = if class * GRAIN > GRAIN {
            class * GRAIN
        } else {
            GRAIN
        };
        let mut cap = PER_CLASS_BYTES / size;
        if cap > PER_CLASS_BLOCKS {
            cap = PER_CLASS_BLOCKS;
        }
        if cap < 1 {
            cap = 1;
        }
        caps[class] = cap;
        class += 1;
    }
    caps
}

/// Returns how many blocks of one class the free list keeps.
///
/// @param class - the size class
#[inline]
fn per_class(class: usize) -> usize {
    CAPS.get(class).copied().unwrap_or(1)
}

thread_local! {
    /// The head of each class's intrusive free list, or null.
    ///
    /// `const` on the whole initialiser, not only on the elements: it removes
    /// the lazy-initialisation check from every access, and an allocator's
    /// per-call cost is the thing this crate exists to keep small.
    static HEADS: [Cell<*mut u8>; CLASSES] =
        const { [const { Cell::new(std::ptr::null_mut()) }; CLASSES] };
    /// How many blocks each class is holding.
    static HELD: [Cell<usize>; CLASSES] = const { [const { Cell::new(0) }; CLASSES] };
}

/// Returns the size class an allocation falls in, if any.
///
/// `None` means "not ours": too large, too aligned, or empty.
///
/// **A request under eight bytes is the sixteen byte class's (task-2191).** It
/// used to be refused, as too small to hold the link a freed block stores in
/// itself. But the block a class hands out is always [`layout_of`] the class,
/// sixteen bytes at least, so the link fits whatever was asked for. Refusing
/// sent every short text value, such as `t7` or `3.5`, to the system heap one
/// allocation and one free at a time. In a profile of forty `.import` runs of
/// 50,000 rows, the system heap's allocations and frees were 29% of the
/// samples, and they are gone from the same profile with this.
///
/// @param layout - the allocation's layout
#[inline]
fn class_of(layout: Layout) -> Option<usize> {
    if layout.size() > LARGEST || layout.align() > GRAIN || layout.size() == 0 {
        return None;
    }
    Some(layout.size().div_ceil(GRAIN))
}

/// Returns the layout a size class's blocks are allocated with.
///
/// @param class - the size class
#[inline]
fn layout_of(class: usize) -> Layout {
    // Every class is a multiple of the grain and aligned to it, so a block is
    // always at least as large and as aligned as any request in its class.
    Layout::from_size_align(class.saturating_mul(GRAIN).max(GRAIN), GRAIN)
        .unwrap_or_else(|_| Layout::new::<u128>())
}

/// The smallest block the page sized lists hold.
const BIG_SMALLEST: usize = 8 << 10;

/// How many page sized classes there are: 8, 16, 32 and 64 KiB.
const BIG_CLASSES: usize = 4;

/// How many bytes one page sized class keeps before handing the rest back.
///
/// Half a megabyte: eight 64 KiB blocks, up to sixty four 8 KiB ones, and at
/// most 2 MiB across the four classes.
const BIG_PER_CLASS_BYTES: usize = 512 << 10;

/// Returns the page sized class an allocation above [`LARGEST`] falls in.
///
/// **Page sized blocks are asked for again and again (task-2191).** A leaf is
/// 32 KiB, and a write copies it, encodes a new image of it and reads its rows
/// into buffers of about its size, then frees all of them. The doc comment on
/// [`LARGEST`] assumed a big block is rare; on an insert into an indexed table
/// they were the most frequent allocation the system heap still served, and
/// the Windows heap returns a freed block of that size to the operating system
/// and commits it again on the next request, so each one also cost page
/// faults. A request is rounded up to a power of two, so a buffer that grows by
/// doubling stays in its block.
///
/// @param layout - the allocation's layout
#[inline]
fn big_class_of(layout: Layout) -> Option<usize> {
    let largest = BIG_SMALLEST << (BIG_CLASSES - 1);
    if layout.size() <= LARGEST || layout.size() > largest || layout.align() > GRAIN {
        return None;
    }
    let rounded = layout.size().next_power_of_two().max(BIG_SMALLEST);
    Some((rounded.trailing_zeros() - BIG_SMALLEST.trailing_zeros()) as usize)
}

/// Returns the layout a page sized class's blocks are allocated with.
///
/// @param class - the page sized class
#[inline]
fn big_layout_of(class: usize) -> Layout {
    Layout::from_size_align(BIG_SMALLEST << class.min(BIG_CLASSES - 1), GRAIN)
        .unwrap_or_else(|_| Layout::new::<u128>())
}

/// The head of each page sized class's free list, shared by every thread.
static BIG_HEADS: [std::sync::atomic::AtomicPtr<u8>; BIG_CLASSES] =
    [const { std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()) }; BIG_CLASSES];
/// How many blocks each page sized class holds.
static BIG_HELD: [std::sync::atomic::AtomicUsize; BIG_CLASSES] =
    [const { std::sync::atomic::AtomicUsize::new(0) }; BIG_CLASSES];
/// One lock per page sized class, held only while a head and its count change.
static BIG_LOCKS: [std::sync::atomic::AtomicBool; BIG_CLASSES] =
    [const { std::sync::atomic::AtomicBool::new(false) }; BIG_CLASSES];

/// Runs `work` with one page sized class's lock held.
///
/// One list for every thread and every allocator in this file, because these
/// blocks are few and taking a lock costs little next to filling 32 KiB.
///
/// @param class - the page sized class
/// @param work - what to do while the lock is held
#[inline]
fn with_big_lock<R>(class: usize, work: impl FnOnce() -> R) -> Option<R> {
    use std::sync::atomic::Ordering;
    let lock = BIG_LOCKS.get(class)?;
    while lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        std::hint::spin_loop();
    }
    let result = work();
    lock.store(false, Ordering::Release);
    Some(result)
}

/// Allocates a block too large for the small classes: from a page sized list
/// when one fits, or from the system.
///
/// @param layout - the allocation's layout
///
/// # Safety
///
/// As for [`GlobalAlloc::alloc`].
#[inline]
unsafe fn big_alloc(layout: Layout) -> *mut u8 {
    use std::sync::atomic::Ordering;
    let Some(class) = big_class_of(layout) else {
        // SAFETY: forwarded unchanged to the system allocator.
        return unsafe { System.alloc(layout) };
    };
    if let (Some(head), Some(held)) = (BIG_HEADS.get(class), BIG_HELD.get(class)) {
        if !head.load(Ordering::Relaxed).is_null() {
            let taken = with_big_lock(class, || {
                let block = head.load(Ordering::Relaxed);
                if block.is_null() {
                    return block;
                }
                // SAFETY: the block is on the list, so its first word is a link.
                let next = unsafe { block.cast::<*mut u8>().read() };
                head.store(next, Ordering::Relaxed);
                held.store(
                    held.load(Ordering::Relaxed).saturating_sub(1),
                    Ordering::Relaxed,
                );
                block
            })
            .unwrap_or(std::ptr::null_mut());
            if !taken.is_null() {
                return taken;
            }
        }
    }
    // SAFETY: a fresh block of the class's own layout, freed with it in
    // `big_dealloc`.
    unsafe { System.alloc(big_layout_of(class)) }
}

/// Frees a block [`big_alloc`] made: onto its page sized list while the list
/// is under its cap, or back to the system.
///
/// @param pointer - the block
/// @param layout - the layout it was asked for with
///
/// # Safety
///
/// As for [`GlobalAlloc::dealloc`], for a block from [`big_alloc`].
#[inline]
unsafe fn big_dealloc(pointer: *mut u8, layout: Layout) {
    use std::sync::atomic::Ordering;
    let Some(class) = big_class_of(layout) else {
        // SAFETY: forwarded unchanged to the allocator that made it.
        return unsafe { System.dealloc(pointer, layout) };
    };
    let cap = BIG_PER_CLASS_BYTES / big_layout_of(class).size();
    let kept = match (BIG_HEADS.get(class), BIG_HELD.get(class)) {
        (Some(head), Some(held)) => with_big_lock(class, || {
            let count = held.load(Ordering::Relaxed);
            if count >= cap {
                return false;
            }
            // SAFETY: the caller has freed the block, so its first word is ours.
            unsafe {
                pointer
                    .cast::<*mut u8>()
                    .write(head.load(Ordering::Relaxed))
            };
            head.store(pointer, Ordering::Relaxed);
            held.store(count.saturating_add(1), Ordering::Relaxed);
            true
        })
        .unwrap_or(false),
        _ => false,
    };
    if !kept {
        // SAFETY: freed with the layout `big_alloc` made it with.
        unsafe { System.dealloc(pointer, big_layout_of(class)) };
    }
}

/// Whether a realloc can keep the block it has: the old and new sizes fall in
/// the same small class, or in the same page sized class.
///
/// @param layout - the block's layout
/// @param new_size - the size asked for
#[inline]
fn same_block(layout: Layout, new_size: usize) -> bool {
    let Ok(wanted) = Layout::from_size_align(new_size, layout.align()) else {
        return false;
    };
    match (class_of(layout), class_of(wanted)) {
        (Some(old), Some(new)) => old == new,
        (None, None) => {
            let old = big_class_of(layout);
            old.is_some() && old == big_class_of(wanted)
        }
        _ => false,
    }
}

/// Whether neither size of a realloc is one this file keeps, so the system
/// allocator made the block and can grow it in place.
///
/// @param layout - the block's layout
/// @param new_size - the size asked for
#[inline]
fn neither_kept(layout: Layout, new_size: usize) -> bool {
    let Ok(wanted) = Layout::from_size_align(new_size, layout.align()) else {
        return false;
    };
    class_of(layout).is_none()
        && class_of(wanted).is_none()
        && big_class_of(layout).is_none()
        && big_class_of(wanted).is_none()
}

/// A size-classed free list over the system allocator.
///
/// Install it in a binary with
///
/// ```ignore
/// #[global_allocator]
/// static ALLOCATOR: inillucent_base::alloc::Pooled = inillucent_base::alloc::Pooled;
/// ```
///
/// It is per binary rather than per library because only a binary can name a
/// global allocator, and because the choice belongs to whoever is running the
/// program.
pub struct Pooled;

// SAFETY: every path either forwards to the system allocator unchanged, or
// hands back a block this allocator obtained from `System` with its class's own
// layout and has not handed out since. The class is derived from the layout on
// both sides, so a block is only ever reused for a request it is large enough
// and aligned enough for.
unsafe impl GlobalAlloc for Pooled {
    // SAFETY: a pooled block was allocated by `System.alloc` with the class's
    // layout, which is at least this request's size and alignment. The link
    // read out of the block was written by `dealloc` below and nothing has
    // touched the block since - it is not reachable by any other path while it
    // is on the list.
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let Some(class) = class_of(layout) else {
            // SAFETY: as for this function; `big_alloc` forwards anything it
            // does not keep to the system allocator unchanged.
            return unsafe { big_alloc(layout) };
        };
        // `try_with`, because a thread tearing down has already dropped its
        // storage and a panic inside an allocator aborts the process. A request
        // that finds no list is simply a fresh block.
        let taken = HEADS
            .try_with(|heads| {
                let Some(head) = heads.get(class) else {
                    return std::ptr::null_mut();
                };
                let block = head.get();
                if block.is_null() {
                    return std::ptr::null_mut();
                }
                // SAFETY: the block is one this allocator made and put on the
                // list, and its first word is the link `dealloc` wrote.
                let next = unsafe { block.cast::<*mut u8>().read() };
                head.set(next);
                let _ = HELD.try_with(|held| {
                    if let Some(count) = held.get(class) {
                        count.set(count.get().saturating_sub(1));
                    }
                });
                block
            })
            .unwrap_or(std::ptr::null_mut());
        if !taken.is_null() {
            return taken;
        }
        // SAFETY: a fresh block of the class's own layout, which is at least as
        // large and as aligned as the request. It is freed with the same
        // layout, in `dealloc` below.
        unsafe { System.alloc(layout_of(class)) }
    }

    // SAFETY: the pointer and layout are the ones handed out above, so a
    // pointer with a pooled class is a block of at least `GRAIN` bytes and can
    // hold the link. Anything else goes back to the system allocator with the
    // layout it was made with.
    #[inline]
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let Some(class) = class_of(layout) else {
            // SAFETY: as for this function; `big_dealloc` hands anything it
            // does not keep back to the system allocator with this layout.
            return unsafe { big_dealloc(pointer, layout) };
        };
        let kept = HELD
            .try_with(|held| {
                let Some(count) = held.get(class) else {
                    return false;
                };
                if count.get() >= per_class(class) {
                    return false;
                }
                HEADS
                    .try_with(|heads| {
                        let Some(head) = heads.get(class) else {
                            return false;
                        };
                        // SAFETY: the block is at least eight bytes and is not
                        // reachable by anything else once the caller has freed
                        // it, so its first word is ours to use as the link.
                        unsafe { pointer.cast::<*mut u8>().write(head.get()) };
                        head.set(pointer);
                        count.set(count.get().saturating_add(1));
                        true
                    })
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if kept {
            return;
        }
        // SAFETY: freed with the layout it was allocated with in `alloc`.
        unsafe { System.dealloc(pointer, layout_of(class)) };
    }

    // SAFETY: a realloc that stays inside one class is the same block, because
    // every block in a class is the full class size. Anything else goes through
    // the default alloc-copy-free, which is what `GlobalAlloc` does when this
    // is not overridden.
    #[inline]
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if same_block(layout, new_size) {
            return pointer;
        }
        // SAFETY: the default behaviour, spelled out: a fresh block, the old
        // bytes copied into it, and the old block freed. Every argument is the
        // caller's or derived from it.
        unsafe {
            let Ok(wanted) = Layout::from_size_align(new_size, layout.align()) else {
                return std::ptr::null_mut();
            };
            let fresh = self.alloc(wanted);
            if !fresh.is_null() {
                std::ptr::copy_nonoverlapping(pointer, fresh, layout.size().min(new_size));
                self.dealloc(pointer, layout);
            }
            fresh
        }
    }
}

/// How many bytes [`Carved`] takes from the system at a time for one class.
const CHUNK_BYTES: usize = 64 << 10;

thread_local! {
    /// [`Carved`]'s free list heads, one per class.
    static CARVED_HEADS: [Cell<*mut u8>; CLASSES] =
        const { [const { Cell::new(std::ptr::null_mut()) }; CLASSES] };
    /// Where [`Carved`] carves the next block of each class, and where that
    /// chunk ends.
    static CARVED_SPANS: [Cell<(usize, usize)>; CLASSES] =
        const { [const { Cell::new((0, 0)) }; CLASSES] };
}

/// A size-classed free list that carves its blocks out of larger chunks, for a
/// program that runs and ends.
///
/// **The blocks a program keeps alive are carved, not asked for one at a
/// time** (task-2191). [`Pooled`] asks the system for every block its lists do
/// not hold, and a statement that keeps what it builds, such as an `.import` of
/// 50,000 rows held for one bulk build, empties every list at once and sends
/// each value to the Windows heap: the heap was 38% of the import. This takes
/// 64 KiB from the system for a class whose list is empty and hands out blocks
/// from it, so a block costs a pointer bump.
///
/// **Freed blocks are kept, not given back.** A carved block is part of a chunk
/// and the system cannot take it alone, so a class keeps every block it has
/// carved, which is at most the most of that class the program held at once.
/// That is why the command line and the shell install it and the MCP server,
/// which runs for as long as its client does, keeps [`Pooled`].
pub struct Carved;

impl Carved {
    /// Carves one block of a class, taking a new chunk when the last is spent.
    ///
    /// @param class - the size class
    #[inline]
    fn carve(class: usize) -> *mut u8 {
        let size = layout_of(class).size();
        CARVED_SPANS
            .try_with(|spans| {
                let Some(span) = spans.get(class) else {
                    return std::ptr::null_mut();
                };
                let (mut next, mut end) = span.get();
                if next.saturating_add(size) > end || next == 0 {
                    let Ok(chunk) = Layout::from_size_align(CHUNK_BYTES.max(size), GRAIN) else {
                        return std::ptr::null_mut();
                    };
                    // SAFETY: a fresh allocation with a valid, nonzero layout.
                    let base = unsafe { System.alloc(chunk) };
                    if base.is_null() {
                        return std::ptr::null_mut();
                    }
                    next = base as usize;
                    end = next.saturating_add(chunk.size());
                }
                span.set((next.saturating_add(size), end));
                next as *mut u8
            })
            .unwrap_or(std::ptr::null_mut())
    }
}

// SAFETY: a block of a class is either carved from a chunk this allocator
// took from `System` with room for it, or one it handed out before and was
// given back with a layout of the same class. Blocks above `LARGEST` or more
// aligned than the grain go to `System` unchanged in both directions.
unsafe impl GlobalAlloc for Carved {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let Some(class) = class_of(layout) else {
            // SAFETY: as for this function; `big_alloc` forwards anything it
            // does not keep to the system allocator unchanged.
            return unsafe { big_alloc(layout) };
        };
        let taken = CARVED_HEADS
            .try_with(|heads| {
                let Some(head) = heads.get(class) else {
                    return std::ptr::null_mut();
                };
                let block = head.get();
                if block.is_null() {
                    return std::ptr::null_mut();
                }
                // SAFETY: a block on the list holds the next link in its
                // first eight bytes, written by `dealloc`.
                head.set(unsafe { block.cast::<*mut u8>().read() });
                block
            })
            .unwrap_or(std::ptr::null_mut());
        if !taken.is_null() {
            return taken;
        }
        Carved::carve(class)
    }

    // SAFETY: the pointer and layout are the ones `alloc` handed out, so a
    // block of a class holds at least the link written into its first word.
    #[inline]
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let Some(class) = class_of(layout) else {
            // SAFETY: as for this function; `big_dealloc` hands anything it
            // does not keep back to the system allocator with this layout.
            return unsafe { big_dealloc(pointer, layout) };
        };
        let _ = CARVED_HEADS.try_with(|heads| {
            if let Some(head) = heads.get(class) {
                // SAFETY: the block is at least eight bytes and aligned to
                // the grain, and nothing else holds it any more.
                unsafe { pointer.cast::<*mut u8>().write(head.get()) };
                head.set(pointer);
            }
        });
    }

    // SAFETY: as for `Pooled`: a realloc inside one class keeps the block, and
    // anything else is a fresh block, a copy and a free.
    #[inline]
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if same_block(layout, new_size) {
            return pointer;
        }
        // SAFETY: a fresh block, the old bytes copied into it and the old block
        // freed, as `GlobalAlloc`'s own default does.
        unsafe {
            let Ok(wanted) = Layout::from_size_align(new_size, layout.align()) else {
                return std::ptr::null_mut();
            };
            let fresh = self.alloc(wanted);
            if !fresh.is_null() {
                std::ptr::copy_nonoverlapping(pointer, fresh, layout.size().min(new_size));
                self.dealloc(pointer, layout);
            }
            fresh
        }
    }
}

/// A size-classed free list shared by every thread, for a library.
///
/// [`Pooled`] keeps its lists per thread, which is right for a program that
/// owns its threads. A library loaded into Python or Node does not: the host
/// starts and ends threads, and every thread that ends would leave its lists
/// behind, up to [`CLASSES`] times the per class cap. These lists belong to the
/// process, so nothing is left behind when a thread ends.
///
/// **Why a lock per class is still cheaper than the system heap** (task-2191).
/// The C library spent about 15% of a single row insert into an indexed table
/// in the Windows heap. A free list here takes one uncontended compare and
/// swap to lock and one store to unlock, and a host calls the library from one
/// thread at a time far more often than from several.
pub struct Shared;

/// Each class's free list head, shared by every thread.
static SHARED_HEADS: [std::sync::atomic::AtomicPtr<u8>; CLASSES] =
    [const { std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()) }; CLASSES];
/// How many blocks each shared class holds.
static SHARED_HELD: [std::sync::atomic::AtomicUsize; CLASSES] =
    [const { std::sync::atomic::AtomicUsize::new(0) }; CLASSES];
/// One lock per class, held only while a head and its count change.
static SHARED_LOCKS: [std::sync::atomic::AtomicBool; CLASSES] =
    [const { std::sync::atomic::AtomicBool::new(false) }; CLASSES];

/// Runs `work` with one class's lock held, and returns what it returned.
///
/// A spin rather than an operating system lock, because the work it guards is
/// three loads and stores and an operating system lock could allocate.
///
/// @param class - the size class whose lock to take
/// @param work - what to do while the lock is held
#[inline]
fn with_class_lock<R>(class: usize, work: impl FnOnce() -> R) -> Option<R> {
    use std::sync::atomic::Ordering;
    let lock = SHARED_LOCKS.get(class)?;
    while lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        std::hint::spin_loop();
    }
    let result = work();
    lock.store(false, Ordering::Release);
    Some(result)
}

// SAFETY: as for `Pooled`. Every block on a list was obtained from `System`
// with its class's layout, and the class lock makes the head, the link and the
// count change together, so no two threads take the same block.
unsafe impl GlobalAlloc for Shared {
    // SAFETY: a block taken from a list is one `dealloc` put there, whose first
    // word is the link it wrote; the class lock is held while it is read.
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        use std::sync::atomic::Ordering;
        let Some(class) = class_of(layout) else {
            // SAFETY: as for this function; `big_alloc` forwards anything it
            // does not keep to the system allocator unchanged.
            return unsafe { big_alloc(layout) };
        };
        let (Some(head), Some(held)) = (SHARED_HEADS.get(class), SHARED_HELD.get(class)) else {
            // SAFETY: forwarded with the class's layout, freed with it below.
            return unsafe { System.alloc(layout_of(class)) };
        };
        // Read without the lock first: an empty list needs no lock at all.
        if !head.load(Ordering::Relaxed).is_null() {
            let taken = with_class_lock(class, || {
                let block = head.load(Ordering::Relaxed);
                if block.is_null() {
                    return block;
                }
                // SAFETY: the block is on the list, so its first word is a link.
                let next = unsafe { block.cast::<*mut u8>().read() };
                head.store(next, Ordering::Relaxed);
                held.store(
                    held.load(Ordering::Relaxed).saturating_sub(1),
                    Ordering::Relaxed,
                );
                block
            })
            .unwrap_or(std::ptr::null_mut());
            if !taken.is_null() {
                return taken;
            }
        }
        // SAFETY: a fresh block of the class's own layout, freed with it below.
        unsafe { System.alloc(layout_of(class)) }
    }

    // SAFETY: the pointer and layout are the ones handed out above, so a block
    // of a pooled class holds at least the link.
    #[inline]
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        use std::sync::atomic::Ordering;
        let Some(class) = class_of(layout) else {
            // SAFETY: as for this function; `big_dealloc` hands anything it
            // does not keep back to the system allocator with this layout.
            return unsafe { big_dealloc(pointer, layout) };
        };
        let (Some(head), Some(held)) = (SHARED_HEADS.get(class), SHARED_HELD.get(class)) else {
            // SAFETY: freed with the layout it was allocated with.
            return unsafe { System.dealloc(pointer, layout_of(class)) };
        };
        let kept = with_class_lock(class, || {
            let count = held.load(Ordering::Relaxed);
            if count >= per_class(class) {
                return false;
            }
            // SAFETY: the caller has freed the block, so its first word is ours.
            unsafe {
                pointer
                    .cast::<*mut u8>()
                    .write(head.load(Ordering::Relaxed))
            };
            head.store(pointer, Ordering::Relaxed);
            held.store(count.saturating_add(1), Ordering::Relaxed);
            true
        })
        .unwrap_or(false);
        if !kept {
            // SAFETY: freed with the layout it was allocated with in `alloc`.
            unsafe { System.dealloc(pointer, layout_of(class)) };
        }
    }

    // SAFETY: as for `Pooled`: a realloc inside one class keeps the block, and
    // anything else is a fresh block, a copy and a free.
    #[inline]
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if same_block(layout, new_size) {
            return pointer;
        }
        if neither_kept(layout, new_size) {
            // SAFETY: neither size is pooled, so the system allocator made the
            // block and can grow it in place.
            return unsafe { System.realloc(pointer, layout, new_size) };
        }
        // SAFETY: a fresh block, the old bytes copied, and the old block freed.
        unsafe {
            let Ok(wanted) = Layout::from_size_align(new_size, layout.align()) else {
                return std::ptr::null_mut();
            };
            let fresh = self.alloc(wanted);
            if !fresh.is_null() {
                std::ptr::copy_nonoverlapping(pointer, fresh, layout.size().min(new_size));
                self.dealloc(pointer, layout);
            }
            fresh
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The classes cover what they claim to and refuse what they do not.
    #[test]
    fn the_classes_are_the_ones_documented() {
        assert_eq!(class_of(Layout::from_size_align(8, 8).unwrap()), Some(1));
        assert_eq!(class_of(Layout::from_size_align(16, 16).unwrap()), Some(1));
        assert_eq!(class_of(Layout::from_size_align(17, 8).unwrap()), Some(2));
        assert_eq!(
            class_of(Layout::from_size_align(LARGEST, 16).unwrap()),
            Some(CLASSES - 1)
        );
        // Smaller than the link, which the class's sixteen byte block holds.
        assert_eq!(class_of(Layout::from_size_align(4, 4).unwrap()), Some(1));
        assert_eq!(class_of(Layout::from_size_align(1, 1).unwrap()), Some(1));
        // Too large, too aligned, and empty.
        assert_eq!(
            class_of(Layout::from_size_align(LARGEST + 1, 8).unwrap()),
            None
        );
        assert_eq!(class_of(Layout::from_size_align(32, 32).unwrap()), None);
        assert_eq!(class_of(Layout::from_size_align(0, 1).unwrap()), None);
    }

    /// Every class's block is at least as large and as aligned as any request
    /// in it, which is the whole of why reuse is safe.
    #[test]
    fn a_class_block_covers_every_request_in_it() {
        for size in 1..=LARGEST {
            let Some(layout) = Layout::from_size_align(size, 8).ok() else {
                continue;
            };
            let Some(class) = class_of(layout) else {
                continue;
            };
            let block = layout_of(class);
            assert!(block.size() >= layout.size(), "size {size}");
            assert!(block.align() >= layout.align(), "size {size}");
        }
    }

    /// A block goes onto its class's list and comes back off it.
    ///
    /// Driven through the allocator itself rather than through the lists,
    /// because what has to hold is that a pointer handed back is one that was
    /// handed out - the intrusive link is an implementation detail and asserting
    /// on it would pin the implementation rather than the behaviour.
    #[test]
    fn a_freed_block_is_the_one_handed_back() {
        let layout = Layout::from_size_align(64, 8).expect("a layout");
        // SAFETY: every pointer here comes from this allocator and is freed
        // with the layout it was allocated with.
        unsafe {
            let first = Pooled.alloc(layout);
            assert!(!first.is_null());
            Pooled.dealloc(first, layout);
            let second = Pooled.alloc(layout);
            assert_eq!(first, second, "the freed block was not recycled");
            Pooled.dealloc(second, layout);
        }
    }

    /// A block that is written to and recycled does not carry its old bytes
    /// into a caller's hands as anything but uninitialised memory.
    ///
    /// The link is written over the first word of a freed block, so a caller
    /// that assumed a fresh allocation was zeroed would be reading it. Nothing
    /// may assume that of `alloc` - `alloc_zeroed` is the one that promises -
    /// and this pins that the link is confined to the block itself and does not
    /// run past its end.
    #[test]
    fn recycling_stays_inside_the_block() {
        let layout = Layout::from_size_align(16, 8).expect("a layout");
        // SAFETY: as above; the guard bytes are a second allocation this test
        // owns for the length of the check.
        unsafe {
            let guard = Pooled.alloc(layout);
            let block = Pooled.alloc(layout);
            std::ptr::write_bytes(guard, 0xAB, layout.size());
            Pooled.dealloc(block, layout);
            let again = Pooled.alloc(layout);
            assert_eq!(block, again);
            for at in 0..layout.size() {
                assert_eq!(guard.add(at).read(), 0xAB, "the link ran past its block");
            }
            Pooled.dealloc(again, layout);
            Pooled.dealloc(guard, layout);
        }
    }

    /// The shared lists recycle a block freed on one thread for a request on
    /// another, which is what lets a library leave nothing behind when a host
    /// thread ends.
    #[test]
    fn a_shared_block_freed_on_one_thread_serves_another() {
        // An odd size no other test in this binary asks for, so the class's
        // list holds only what this test put there.
        let layout = Layout::from_size_align(3_000, 8).expect("a layout");
        // SAFETY: the block comes from `Shared` and is freed with its layout.
        let freed = std::thread::spawn(move || unsafe {
            let block = Shared.alloc(layout);
            assert!(!block.is_null());
            Shared.dealloc(block, layout);
            block as usize
        })
        .join()
        .expect("the thread ran");
        // SAFETY: as above.
        unsafe {
            let again = Shared.alloc(layout);
            assert_eq!(
                again as usize, freed,
                "the shared list did not recycle the block"
            );
            let grown = Shared.realloc(again, layout, 3_001);
            assert_eq!(grown, again, "a realloc inside one class moved the block");
            Shared.dealloc(grown, Layout::from_size_align(3_001, 8).expect("a layout"));
        }
    }

    /// A carved block is recycled, carved blocks of one chunk do not overlap,
    /// and a class carves a second chunk when the first is spent.
    ///
    /// Driven on a thread of its own so its lists hold only what it put there.
    #[test]
    fn carved_blocks_are_distinct_and_recycled() {
        std::thread::spawn(|| {
            // An odd size no other test asks for.
            let layout = Layout::from_size_align(1_000, 8).expect("a layout");
            let block = layout_of(class_of(layout).expect("a class")).size();
            let per_chunk = CHUNK_BYTES / block;
            // SAFETY: every pointer comes from `Carved` and is freed with the
            // layout it was made with.
            unsafe {
                let mut held = Vec::new();
                for nth in 0..per_chunk.saturating_add(3) {
                    let pointer = Carved.alloc(layout);
                    assert!(!pointer.is_null());
                    std::ptr::write_bytes(pointer, (nth % 251) as u8, layout.size());
                    held.push(pointer);
                }
                // No two blocks share a byte: each still holds what was written
                // into it, after every later block was written.
                for (nth, pointer) in held.iter().enumerate() {
                    for at in [0, layout.size() - 1] {
                        assert_eq!(pointer.add(at).read(), (nth % 251) as u8, "block {nth}");
                    }
                }
                let last = held.pop().expect("a block");
                Carved.dealloc(last, layout);
                let again = Carved.alloc(layout);
                assert_eq!(again, last, "the freed block was not recycled");
                held.push(again);
                for pointer in held {
                    Carved.dealloc(pointer, layout);
                }
            }
        })
        .join()
        .expect("the thread ran");
    }

    /// A request above the small classes is rounded to a page sized class, and
    /// one above 64 KiB is not kept.
    #[test]
    fn page_sized_requests_fall_in_power_of_two_classes() {
        let class = |size| big_class_of(Layout::from_size_align(size, 8).unwrap());
        assert_eq!(class(LARGEST), None, "the small classes hold this");
        assert_eq!(class(LARGEST + 1), Some(0));
        assert_eq!(class(8 << 10), Some(0));
        assert_eq!(class((8 << 10) + 1), Some(1));
        assert_eq!(class(32 << 10), Some(2));
        assert_eq!(class(64 << 10), Some(3));
        assert_eq!(
            class((64 << 10) + 1),
            None,
            "the system allocator holds this"
        );
        assert_eq!(
            big_class_of(Layout::from_size_align(32 << 10, 64).unwrap()),
            None,
            "more aligned than the grain"
        );
    }

    /// A page sized block grows inside its class without moving, keeps its
    /// bytes when it moves to the next class, and is handed out again after it
    /// is freed. A block above 64 KiB is never put on a list.
    ///
    /// The only test that touches the page sized lists, which every thread
    /// shares, so nothing else takes the freed block between the free and the
    /// next request.
    #[test]
    fn page_sized_blocks_are_recycled() {
        let small = Layout::from_size_align(20_000, 8).unwrap();
        // SAFETY: every pointer comes from the allocator it is freed to, with
        // the layout it was made or last grown to.
        unsafe {
            for allocator in [&Shared as &dyn GlobalAlloc, &Carved, &Pooled] {
                let pointer = allocator.alloc(small);
                assert!(!pointer.is_null());
                std::ptr::write_bytes(pointer, 9, small.size());
                let grown = allocator.realloc(pointer, small, 30_000);
                assert_eq!(grown, pointer, "30,000 bytes is still the 32 KiB class");
                let moved =
                    allocator.realloc(grown, Layout::from_size_align(30_000, 8).unwrap(), 40_000);
                assert!(!moved.is_null());
                assert_eq!(moved.add(19_999).read(), 9, "the bytes were not kept");
                let at_forty = Layout::from_size_align(40_000, 8).unwrap();
                allocator.dealloc(moved, at_forty);
                let again = allocator.alloc(Layout::from_size_align(64 << 10, 8).unwrap());
                assert_eq!(
                    again, moved,
                    "the freed 64 KiB block was not handed out again"
                );
                allocator.dealloc(again, Layout::from_size_align(64 << 10, 8).unwrap());
            }
            let held = BIG_HELD
                .iter()
                .map(|count| count.load(std::sync::atomic::Ordering::Relaxed))
                .sum::<usize>();
            let huge = Layout::from_size_align(128 << 10, 8).unwrap();
            let pointer = Shared.alloc(huge);
            assert!(!pointer.is_null());
            Shared.dealloc(pointer, huge);
            let after = BIG_HELD
                .iter()
                .map(|count| count.load(std::sync::atomic::Ordering::Relaxed))
                .sum::<usize>();
            assert_eq!(after, held, "a block above 64 KiB was kept");
        }
    }

    /// A realloc inside one class is the same block, and one that crosses a
    /// class keeps the bytes.
    #[test]
    fn realloc_keeps_the_bytes() {
        // SAFETY: every pointer comes from this allocator, and every layout is
        // the one the block was made with.
        unsafe {
            let small = Layout::from_size_align(16, 8).expect("a layout");
            let block = Pooled.alloc(small);
            std::ptr::write_bytes(block, 0x5A, small.size());
            let inside = Pooled.realloc(block, small, 12);
            assert_eq!(inside, block, "a realloc inside one class moved the block");
            let across = Pooled.realloc(inside, small, 200);
            assert!(!across.is_null());
            for at in 0..small.size() {
                assert_eq!(across.add(at).read(), 0x5A, "realloc lost a byte");
            }
            Pooled.dealloc(across, Layout::from_size_align(200, 8).expect("a layout"));
        }
    }
}
