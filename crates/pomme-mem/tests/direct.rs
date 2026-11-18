//! Loads and stores that go straight to a page's bytes (the TLB's fast path)
//! see the same memory as the slow path: across pages, at region edges, in a
//! region that doesn't fill its pages, and in code that was written after
//! the CPU cached it.

use pomme_mem::{Memory, PAGE_SIZE, Prot};

const A: u64 = 0x1_0000_0000;

#[test]
fn values_round_trip_on_and_across_pages() {
    let mut m = Memory::default();
    m.map(A, 4 * PAGE_SIZE, Prot::RW, "a").unwrap();
    for &at in &[A, A + 8, A + PAGE_SIZE - 4, A + PAGE_SIZE - 1, A + 2 * PAGE_SIZE + 3, A + 4 * PAGE_SIZE - 8] {
        m.write_u64(at, 0x0102_0304_0506_0708).unwrap();
        assert_eq!(m.read_u64(at).unwrap(), 0x0102_0304_0506_0708, "{at:#x}");
        // The bulk path sees what the direct path wrote, and the other way round.
        assert_eq!(m.read(at, 8).unwrap(), &0x0102_0304_0506_0708u64.to_le_bytes());
        m.write(at, &7u64.to_le_bytes()).unwrap();
        assert_eq!(m.read_u64(at).unwrap(), 7);
    }
    // Past the end faults, also when only some of the bytes are past it.
    assert!(m.read_u64(A + 4 * PAGE_SIZE - 4).is_err());
    assert!(m.write_u32(A + 4 * PAGE_SIZE, 1).is_err());
}

#[test]
fn regions_smaller_than_a_page_and_new_maps() {
    let mut m = Memory::default();
    // Two small regions sharing one page.
    m.map(A, 0x100, Prot::RW, "a").unwrap();
    m.map(A + 0x200, 0x100, Prot::RW, "b").unwrap();
    m.write_u32(A + 0x10, 1).unwrap();
    m.write_u32(A + 0x210, 2).unwrap();
    assert_eq!((m.read_u32(A + 0x10).unwrap(), m.read_u32(A + 0x210).unwrap()), (1, 2));
    assert!(m.read_u32(A + 0x150).is_err());
    assert!(m.read_u32(A + 0xfe).is_err()); // runs off the end of "a"
    // A region mapped into the gap afterwards is found.
    m.map(A + 0x100, 0x100, Prot::RW, "c").unwrap();
    m.write_u32(A + 0x150, 3).unwrap();
    assert_eq!(m.read_u32(A + 0x150).unwrap(), 3);
}

#[test]
fn stores_to_cached_code_change_the_code_generation() {
    let mut m = Memory::default();
    m.map(A, 2 * PAGE_SIZE, Prot::RW, "code").unwrap();
    // A store first (so the page is cached for direct stores), then the CPU caches code there.
    m.write_u32(A + 4, 0).unwrap();
    m.mark_code(A);
    let before = m.code_generation();
    m.write_u32(A + 4, 0xd503_201f).unwrap();
    assert!(m.code_generation() > before);
    assert_eq!(m.read_u32(A + 4).unwrap(), 0xd503_201f);
}

#[test]
fn top_byte_is_ignored() {
    let mut m = Memory::default();
    m.map(A, PAGE_SIZE, Prot::RW, "a").unwrap();
    m.write_u64(A | 0xab00_0000_0000_0000, 5).unwrap();
    assert_eq!(m.read_u64(A).unwrap(), 5);
    assert_eq!(m.read_u64(A | 0x1200_0000_0000_0000).unwrap(), 5);
}
