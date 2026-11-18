//! Cached blocks behave exactly like executing one instruction at a time:
//! code rewritten after it ran is fetched again, a budget that ends inside a
//! block stops at that exact instruction, and a fault mid-block leaves pc and
//! the count at the faulting instruction.

use pomme_cpu::{Cpu, Exit, HOST_RETURN};
use pomme_mem::{Memory, Prot};

const CODE: u64 = 0x1_0000_0000;
const MOVZ_X0_1: u32 = 0xd280_0020; // mov x0, #1
const MOVZ_X0_5: u32 = 0xd280_00a0; // mov x0, #5
const ADD_X0_2: u32 = 0x9100_0800; // add x0, x0, #2
const LDR_X1_X2: u32 = 0xf940_0041; // ldr x1, [x2]
const RET: u32 = 0xd65f_03c0;

fn setup(code: &[u32]) -> (Cpu, Memory) {
    let mut mem = Memory::default();
    mem.map(CODE, 0x4000, Prot::RX, "code").unwrap();
    for (i, insn) in code.iter().enumerate() {
        mem.write_u32(CODE + i as u64 * 4, *insn).unwrap();
    }
    (Cpu::new(), mem)
}

fn call(cpu: &mut Cpu, mem: &mut Memory, budget: u64) -> Exit {
    cpu.pc = CODE;
    cpu.x[30] = HOST_RETURN;
    cpu.run(mem, budget)
}

#[test]
fn rewritten_code_is_fetched_again() {
    let (mut cpu, mut mem) = setup(&[MOVZ_X0_1, ADD_X0_2, RET]);
    assert!(matches!(call(&mut cpu, &mut mem, 100), Exit::Returned));
    assert_eq!(cpu.x[0], 3);
    // As pomme does when it hooks a function: the cached block must go.
    mem.write_u32(CODE, MOVZ_X0_5).unwrap();
    assert!(matches!(call(&mut cpu, &mut mem, 100), Exit::Returned));
    assert_eq!(cpu.x[0], 7);
}

#[test]
fn a_budget_inside_a_block_stops_at_that_instruction() {
    let (mut cpu, mut mem) = setup(&[MOVZ_X0_1, ADD_X0_2, ADD_X0_2, ADD_X0_2, RET]);
    // Warm the cache, then run with room for only two instructions.
    call(&mut cpu, &mut mem, 100);
    let before = cpu.steps;
    assert!(matches!(call(&mut cpu, &mut mem, 2), Exit::StepLimit));
    assert_eq!((cpu.steps - before, cpu.pc, cpu.x[0]), (2, CODE + 8, 3));
}

#[test]
fn a_fault_mid_block_stops_at_the_faulting_instruction() {
    let (mut cpu, mut mem) = setup(&[MOVZ_X0_1, LDR_X1_X2, ADD_X0_2, RET]);
    cpu.x[2] = 0xdead_0000; // unmapped
    let before = cpu.steps;
    assert!(matches!(call(&mut cpu, &mut mem, 100), Exit::Fault { .. }));
    assert_eq!((cpu.steps - before, cpu.pc, cpu.x[0]), (1, CODE + 4, 1));
}

#[test]
fn rewriting_one_page_keeps_the_other_pages_code_right() {
    // Two functions on different pages; one is rewritten, both still run right.
    let (mut cpu, mut mem) = setup(&[MOVZ_X0_1, ADD_X0_2, RET]);
    const OTHER: u64 = CODE + 0x4000;
    mem.map(OTHER, 0x4000, Prot::RX, "other").unwrap();
    for (i, insn) in [MOVZ_X0_5, ADD_X0_2, RET].iter().enumerate() {
        mem.write_u32(OTHER + i as u64 * 4, *insn).unwrap();
    }
    let run = |cpu: &mut Cpu, mem: &mut Memory, at: u64| {
        cpu.pc = at;
        cpu.x[30] = HOST_RETURN;
        assert!(matches!(cpu.run(mem, 100), Exit::Returned));
        cpu.x[0]
    };
    assert_eq!((run(&mut cpu, &mut mem, CODE), run(&mut cpu, &mut mem, OTHER)), (3, 7));
    mem.write_u32(OTHER, MOVZ_X0_1).unwrap();
    assert_eq!((run(&mut cpu, &mut mem, CODE), run(&mut cpu, &mut mem, OTHER)), (3, 3));
    mem.write_u32(CODE + 4, MOVZ_X0_5).unwrap(); // mov x0, #5 replaces the add
    assert_eq!((run(&mut cpu, &mut mem, CODE), run(&mut cpu, &mut mem, OTHER)), (5, 3));
}

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
    (0x1a40_0800, 0xc01f_f00f), // ccmp/ccmn
    (0x1a80_0000, 0xc01f_f7ff), // csel family
    (0x3800_0000, 0xc4ff_ffff), // ldr/str single, imm9 and register
    (0x3820_0800, 0xc4df_f3ff), // ldr/str register offset
    (0x5800_0000, 0x00ff_ffff), // ldr literal (reads pc; slow path)
    (0x9b00_0000, 0x00ff_ffff), // madd/msub and friends (slow path)
    (0x1e20_0000, 0x00ff_ffff), // scalar floating point (slow path)
    (0x5400_0000, 0x0000_01ef), // b.cond, a few instructions on
    (0xb400_0000, 0x0100_01ff), // cbz/cbnz, a few instructions on
    (0x1400_0000, 0x0000_0007), // b, a few instructions on
];

/// Random code, run in blocks with random budgets, stops where running it
/// one instruction at a time does, with the same registers, count and memory.
#[test]
fn blocks_match_single_steps() {
    const DATA: u64 = 0x2000_0000;
    let mut r = Rng(0x2545_f491_4f6c_dd1d);
    for _ in 0..20_000 {
        let n = 1 + r.next() as usize % 40;
        let mut code: Vec<u32> = (0..n)
            .map(|_| {
                let (fixed, free) = TEMPLATES[r.next() as usize % TEMPLATES.len()];
                fixed | (r.next() as u32 & free)
            })
            .collect();
        code.push(RET);
        let (mut a, mut ma) = setup(&code);
        ma.map(DATA, 0x8000, Prot::RW, "data").unwrap();
        for i in 0..30 {
            // Mostly addresses in the data, sometimes anything (a fault).
            a.x[i] = if r.next() % 8 == 0 { r.next() } else { DATA + (r.next() % 0x7000) & !7 };
        }
        a.sp = DATA + 0x4000;
        a.set_nzcv(r.next() & 0xf000_0000);
        let (mut b, mut mb) = (a.clone(), ma.clone());
        let budget = 1 + r.next() % 120;
        // Warm the cache sometimes, so blocks run from it.
        if r.next() % 2 == 0 {
            let (mut c, mut mc) = (a.clone(), ma.clone());
            call(&mut c, &mut mc, budget);
            b.blocks = c.blocks;
        }

        a.pc = CODE;
        a.x[30] = HOST_RETURN;
        let stop = a.steps + budget;
        let ea = loop {
            if a.pc == HOST_RETURN {
                break Exit::Returned;
            }
            if a.steps >= stop {
                break Exit::StepLimit;
            }
            if let Err(e) = a.step(&mut ma) {
                break e;
            }
        };
        let eb = call(&mut b, &mut mb, budget);
        let what = format!("{code:08x?} budget {budget}");
        assert_eq!(ea, eb, "{what}");
        assert_eq!((a.x, a.sp, a.pc, a.nzcv(), a.steps), (b.x, b.sp, b.pc, b.nzcv(), b.steps), "{what}");
        assert_eq!(a.q, b.q, "{what}");
        assert_eq!(ma.read(DATA, 0x8000).unwrap(), mb.read(DATA, 0x8000).unwrap(), "{what}");
    }
}
