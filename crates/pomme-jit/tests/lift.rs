//! The front end does exactly what the interpreter does: random instructions
//! and random blocks, from random register states, run once on the
//! interpreter and once as IR (through the evaluator), must give the same
//! registers, flags, pc, step count, exit and memory.

use pomme_cpu::fast::{Op, decode};
use pomme_cpu::{Cpu, Exit};
use pomme_jit::{eval, ir::Inst, lift::lift};
use pomme_mem::{Memory, Prot};

const BASE: u64 = 0x1000_0000;
const SIZE: u64 = 0x10000;
const PC: u64 = 0x2000_0000;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Encodings random bits rarely hit: (fixed bits, bits left random).
const TEMPLATES: &[(u32, u32)] = &[
    (0x2a00_03e0, 0x801f_001f), // mov
    (0x1100_0000, 0xe07f_ffff), // add/sub imm
    (0x3900_0000, 0xc0ff_ffff), // ldr/str imm
    (0x2800_0000, 0x81ff_ffff), // ldp/stp
    (0x1280_0000, 0xe07f_ffff), // move wide
    (0x1000_0000, 0xff7f_ffff), // adr/adrp
    (0x1200_0000, 0xe07f_ffff), // logical imm
    (0x1300_0000, 0xe07f_ffff), // bitfield
    (0x0a00_0000, 0xe0ff_ffff), // logical reg
    (0x0b00_0000, 0xe0df_ffff), // add/sub reg
    (0x0b20_0000, 0xe0df_ffff), // add/sub ext
    (0x1a40_0800, 0xc01f_f00f), // ccmp/ccmn (immediate)
    (0x1a40_0000, 0xc01f_f00f), // ccmp/ccmn (register)
    (0x1a80_0000, 0xc01f_f7ff), // csel family
    (0x3800_0000, 0xc4ff_ffff), // ldr/str single, imm9 and register
    (0x3900_0000, 0xc4ff_ffff), // ldr/str single, unsigned offset, all kinds
    (0x3820_0800, 0xc4df_f3ff), // ldr/str register offset
    (0x9b00_0000, 0x00ff_ffff), // madd/msub and friends (helpers)
    (0x1e20_0000, 0x00ff_ffff), // scalar floating point (helpers)
    (0xd503_201f, 0x0000_0fe0), // hints
];

/// Branches, to end a block.
const BRANCHES: &[(u32, u32)] = &[
    (0xd65f_0000, 0x0000_03e0), // ret
    (0x1400_0000, 0x83ff_ffff), // b/bl
    (0x5400_0000, 0x00ff_ffef), // b.cond
    (0x3400_0000, 0x83ff_ffff), // cbz/cbnz
    (0x3600_0000, 0x81ff_ffff), // tbz/tbnz
    (0xd61f_0000, 0x0020_03e0), // br/blr
    (0xd420_0000, 0x001f_ffe0), // brk (a helper that exits)
];

fn pick(r: &mut Rng, from: &[(u32, u32)]) -> u32 {
    let (fixed, free) = from[r.next() as usize % from.len()];
    fixed | (r.next() as u32 & free)
}

fn memory(seed: u64) -> Memory {
    let mut mem = Memory::default();
    mem.map(BASE, SIZE, Prot::RW, "data").unwrap();
    mem.map(PC, 0x4000, Prot::RX, "code").unwrap();
    let mut r = Rng(seed | 1);
    let bytes: Vec<u8> = (0..SIZE / 8).flat_map(|_| r.next().to_le_bytes()).collect();
    mem.write(BASE, &bytes).unwrap();
    mem
}

fn cpu(r: &mut Rng) -> Cpu {
    let mut c = Cpu::new();
    for x in c.x.iter_mut() {
        // Mostly addresses in the data region (so loads and stores land), sometimes anything.
        *x = if r.next().is_multiple_of(4) { r.next() } else { (BASE + (r.next() % (SIZE / 2))) & !7 };
    }
    c.sp = (BASE + (r.next() % (SIZE / 2))) & !15;
    c.set_nzcv(r.next() & 0xf000_0000);
    for q in c.q.iter_mut() {
        *q = (r.next() as u128) << 64 | r.next() as u128;
    }
    c.pc = PC;
    c.steps = 1000;
    c
}

fn state(c: &Cpu) -> (Vec<u64>, u64, u64, u64, u64, Vec<u128>, u64) {
    (c.x.to_vec(), c.sp, c.pc, c.nzcv(), c.steps, c.q.to_vec(), c.fpsr)
}

/// Write `code` at PC, then run it once on the interpreter (one instruction
/// at a time, at most `code.len()`) and once lifted, and compare.
fn check(code: &[u32], seed: u64) -> usize {
    let mut r = Rng(seed | 1);
    let mut mem = memory(seed);
    for (i, insn) in code.iter().enumerate() {
        mem.write_u32(PC + 4 * i as u64, *insn).unwrap();
    }
    let start = cpu(&mut r);
    let (mut a, mut b) = (start.clone(), start);
    let mut mb = mem.clone();

    let Some(ops) = pomme_cpu::fetch_block(PC, &mem) else { return 0 };
    let n = ops.len();
    let mut ra: Result<(), Exit> = Ok(());
    for _ in 0..n {
        ra = a.step(&mut mem);
        if ra.is_err() {
            break;
        }
    }
    let block = lift(PC, ops);
    let rb = eval::run(&block, &mut b, &mut mb);
    let what = || format!("{code:08x?}\n{:?}", block.ops);
    assert_eq!(format!("{ra:?}"), format!("{rb:?}"), "{}", what());
    assert_eq!(state(&a), state(&b), "{}", what());
    assert_eq!(mem.read(BASE, SIZE as usize).unwrap(), mb.read(BASE, SIZE as usize).unwrap(), "{}", what());
    block.len() - block.helpers()
}

/// One instruction at a time, from random bits and from templates.
#[test]
fn single_instructions_match_the_interpreter() {
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    let mut lifted = 0;
    for i in 0..200_000u32 {
        let insn = match i % 4 {
            0 => r.next() as u32,
            1 => pick(&mut r, BRANCHES),
            _ => pick(&mut r, TEMPLATES),
        };
        lifted += check(&[insn], r.next());
    }
    // Most template instructions are translated, not helpers.
    assert!(lifted > 80_000, "only {lifted} translated");
}

/// Blocks of up to 12 instructions, ending with a branch: values kept in
/// the block across instructions, faults and exits in the middle.
#[test]
fn blocks_match_the_interpreter() {
    let mut r = Rng(0x2545_f491_4f6c_dd1d);
    for _ in 0..50_000 {
        let len = 1 + r.next() as usize % 12;
        let mut code: Vec<u32> = (0..len)
            .map(|_| loop {
                let insn = pick(&mut r, TEMPLATES);
                // Only the last instruction may end the block.
                if !matches!(decode(insn, PC), Op::B { .. } | Op::BCond { .. } | Op::Cbz { .. } | Op::Tbz { .. } | Op::Ret { .. } | Op::Br { .. })
                    && (insn >> 25) & 0xf != 0b1010
                    && (insn >> 25) & 0xf != 0b1011
                {
                    break insn;
                }
            })
            .collect();
        code.push(pick(&mut r, BRANCHES));
        check(&code, r.next());
    }
}

/// The front end keeps values in the block: a block of plain arithmetic
/// reads each register once and writes it back once, at the end.
#[test]
fn registers_stay_in_the_block() {
    let mut mem = memory(1);
    // add x0, x0, #1 (three times); cmp x0, #3; ret
    for (i, insn) in [0x9100_0400u32, 0x9100_0400, 0x9100_0400, 0xf100_0c1f, 0xd65f_03c0].iter().enumerate() {
        mem.write_u32(PC + 4 * i as u64, *insn).unwrap();
    }
    let block = lift(PC, pomme_cpu::fetch_block(PC, &mem).unwrap());
    let gets = block.insts.iter().filter(|i| matches!(i, Inst::Get { .. })).count();
    let sets = block.insts.iter().filter(|i| matches!(i, Inst::Set { .. })).count();
    // Reads x0 and x30; writes x0 and the four flags.
    assert_eq!((gets, sets), (2, 5), "{:#?}", block.insts);
}
