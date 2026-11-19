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
