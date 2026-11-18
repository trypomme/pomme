//! The front end does exactly what the interpreter does: random instructions
//! and random blocks, from random register states, run once on the
//! interpreter and once as IR (through the evaluator), must give the same
//! registers, flags, pc, step count, exit and memory.

mod common;

use common::*;
use pomme_cpu::Exit;
use pomme_jit::{eval, ir::Inst, lift::lift};

/// Write `code` at PC, then run it once on the interpreter (one instruction
/// at a time, at most `code.len()`) and once lifted, and compare.
fn check(code: &[u32], seed: u64) -> usize {
    let (start, mut mem) = setup(code, seed);
    let (mut a, mut b) = (start.clone(), start);
    let mut mb = mem.clone();

    let Some(ops) = pomme_cpu::fetch_block(PC, &mem) else {
        return 0;
    };
    let ra: Result<(), Exit> = interpret(&mut a, &mut mem, ops.len());
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
        let code = random_block(&mut r);
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
