//! Run IR directly, in Rust. Slow, and only for tests: it checks the front
//! end against the interpreter without any back end. A back end must give
//! exactly what this gives.

use crate::ir::{BinOp, Block, ExitPoint, Inst, Loc};
use pomme_cpu::{Cpu, Exit};
use pomme_mem::Memory;

/// Run `block` on `cpu` (whose pc must be the block's). On Ok the whole
/// block ran: pc is the next pc and the step count grew by the block's length.
pub fn run(block: &Block, cpu: &mut Cpu, mem: &mut Memory) -> Result<(), Exit> {
    debug_assert_eq!(cpu.pc, block.pc);
    let entry = cpu.steps;
    let mut v = vec![0u64; block.values as usize];
    // Stop at instruction `at.insn` with `exit`, after writing back what the block holds.
    let stop = |cpu: &mut Cpu, v: &[u64], at: &ExitPoint, exit: Exit| {
        for &(loc, val) in &at.dirty {
            set(cpu, loc, v[val.0 as usize]);
        }
        cpu.pc = block.pc.wrapping_add(4 * at.insn as u64);
        cpu.steps = entry + at.insn as u64;
        Err(exit)
    };
    for inst in &block.insts {
        match inst {
            Inst::Const { dst, value } => v[dst.0 as usize] = *value,
            Inst::Get { dst, loc } => v[dst.0 as usize] = get(cpu, *loc),
            Inst::Set { loc, src } => set(cpu, *loc, v[src.0 as usize]),
            Inst::Bin { dst, op, a, b } => {
                let (a, b) = (v[a.0 as usize], v[b.0 as usize]);
                v[dst.0 as usize] = match op {
                    BinOp::Add => a.wrapping_add(b),
                    BinOp::Sub => a.wrapping_sub(b),
                    BinOp::Mul => a.wrapping_mul(b),
                    BinOp::And => a & b,
                    BinOp::Or => a | b,
                    BinOp::Xor => a ^ b,
                    BinOp::Shl => a << (b & 63),
                    BinOp::Lshr => a >> (b & 63),
                    BinOp::Ashr => ((a as i64) >> (b & 63)) as u64,
                    BinOp::Eq => (a == b) as u64,
                    BinOp::Ne => (a != b) as u64,
                    BinOp::Ult => (a < b) as u64,
                };
            }
            Inst::Not { dst, a } => v[dst.0 as usize] = !v[a.0 as usize],
            Inst::Select { dst, cond, a, b } => {
                v[dst.0 as usize] = if v[cond.0 as usize] != 0 { v[a.0 as usize] } else { v[b.0 as usize] };
            }
            Inst::Load { dst, addr, bytes, at } => {
                let a = v[addr.0 as usize];
                let r = match bytes {
                    1 => mem.read_u8(a).map(|x| x as u64),
                    2 => mem.read_u16(a).map(|x| x as u64),
                    4 => mem.read_u32(a).map(|x| x as u64),
                    _ => mem.read_u64(a),
                };
                match r {
                    Ok(x) => v[dst.0 as usize] = x,
                    Err(e) => return stop(cpu, &v, at, e.into()),
                }
            }
            Inst::Store { addr, value, bytes, at } => {
                let (a, x) = (v[addr.0 as usize], v[value.0 as usize]);
                let r = match bytes {
                    1 => mem.write_u8(a, x as u8),
                    2 => mem.write_u16(a, x as u16),
                    4 => mem.write_u32(a, x as u32),
                    _ => mem.write_u64(a, x),
                };
                if let Err(e) = r {
                    return stop(cpu, &v, at, e.into());
                }
            }
            Inst::Helper { insn } => {
                cpu.pc = block.pc.wrapping_add(4 * *insn as u64);
                cpu.steps = entry + *insn as u64;
                cpu.exec_op(&block.ops[*insn as usize], mem)?;
            }
        }
    }
    cpu.pc = v[block.next.0 as usize];
    cpu.steps = entry + block.len() as u64;
    Ok(())
}

fn get(cpu: &Cpu, loc: Loc) -> u64 {
    match loc {
        Loc::X(r) => cpu.x[r as usize],
        Loc::Sp => cpu.sp,
        Loc::N => cpu.n as u64,
        Loc::Z => cpu.z as u64,
        Loc::C => cpu.c as u64,
        Loc::Vf => cpu.v as u64,
        Loc::Pc => cpu.pc,
    }
}

fn set(cpu: &mut Cpu, loc: Loc, x: u64) {
    match loc {
        Loc::X(r) => cpu.x[r as usize] = x,
        Loc::Sp => cpu.sp = x,
        Loc::N => cpu.n = x != 0,
        Loc::Z => cpu.z = x != 0,
        Loc::C => cpu.c = x != 0,
        Loc::Vf => cpu.v = x != 0,
        Loc::Pc => cpu.pc = x,
    }
}
