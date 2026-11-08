//! Branches, exception generation and system instructions.

use crate::{Cpu, Exit, Step, bit, bits, sext};
use pomme_mem::Memory;

// MRS/MSR system register keys: bits 19:5 of the instruction (o0:op1:CRn:CRm:op2).
const NZCV: u32 = 0x5a10;
const FPCR: u32 = 0x5a20;
const FPSR: u32 = 0x5a21;
const DCZID_EL0: u32 = 0x5807;
const TPIDR_EL0: u32 = 0x5e82;
const TPIDRRO_EL0: u32 = 0x5e83;
const CNTFRQ_EL0: u32 = 0x5f00;
const CNTVCT_EL0: u32 = 0x5f02;

/// Guest timer frequency. The counter is derived from retired instructions, not host time.
pub const TIMER_HZ: u64 = 24_000_000;

impl Cpu {
    pub(crate) fn branch_sys(&mut self, insn: u32, mem: &mut Memory) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });

        // B / BL
        if insn & 0x7c00_0000 == 0x1400_0000 {
            let off = sext(bits(insn, 25, 0) as u64, 26) * 4;
            if bit(insn, 31) {
                self.x[30] = self.pc + 4;
            }
            self.next_pc = self.pc.wrapping_add(off as u64);
            return Ok(());
        }
        // CBZ / CBNZ
        if insn & 0x7e00_0000 == 0x3400_0000 {
            let v = self.rd(bits(insn, 4, 0), bit(insn, 31));
            if (v == 0) != bit(insn, 24) {
                self.next_pc = self.pc.wrapping_add((sext(bits(insn, 23, 5) as u64, 19) * 4) as u64);
            }
            return Ok(());
        }
        // TBZ / TBNZ
        if insn & 0x7e00_0000 == 0x3600_0000 {
            let b = (bits(insn, 31, 31) << 5) | bits(insn, 23, 19);
            let set = (self.xr(bits(insn, 4, 0)) >> b) & 1 == 1;
            if set == bit(insn, 24) {
                self.next_pc = self.pc.wrapping_add((sext(bits(insn, 18, 5) as u64, 14) * 4) as u64);
            }
            return Ok(());
        }
        // B.cond
        if insn & 0xff00_0010 == 0x5400_0000 {
            if self.cond(bits(insn, 3, 0)) {
                self.next_pc = self.pc.wrapping_add((sext(bits(insn, 23, 5) as u64, 19) * 4) as u64);
            }
            return Ok(());
        }
        // Exception generation
        if insn & 0xff00_0000 == 0xd400_0000 {
            let imm = bits(insn, 20, 5) as u16;
            return match (bits(insn, 23, 21), bits(insn, 4, 0)) {
                (0b000, 0b00001) => Err(Exit::Svc { imm }),
                (0b001, 0b00000) => Err(Exit::Brk { imm }),
                _ => unimpl,
            };
        }
        // BR / BLR / RET (non-authenticating forms)
        match insn & 0xffff_fc1f {
            0xd61f_0000 => {
                self.next_pc = self.xr(bits(insn, 9, 5));
                return Ok(());
            }
            0xd63f_0000 => {
                let target = self.xr(bits(insn, 9, 5));
                self.x[30] = self.pc + 4;
                self.next_pc = target;
                return Ok(());
            }
            0xd65f_0000 => {
                self.next_pc = self.xr(bits(insn, 9, 5));
                return Ok(());
            }
            _ => {}
        }

        // System
        if insn & 0xffc0_0000 == 0xd500_0000 {
            // HINT space: NOP, YIELD, BTI, PAC hints (no-ops on plain arm64), ...
            if insn & 0xffff_f01f == 0xd503_201f {
                return Ok(());
            }
            // Barriers (DSB/DMB/ISB/SB) and CLREX. Single core: barriers are no-ops.
            if insn & 0xffff_f01f == 0xd503_301f {
                if bits(insn, 7, 5) == 0b010 {
                    self.exclusive = None;
                }
                return Ok(());
            }
            let key = bits(insn, 19, 5);
            let rt = bits(insn, 4, 0);
            // MRS
            if insn & 0xfff0_0000 == 0xd530_0000 {
                let v = match key {
                    NZCV => self.nzcv(),
                    FPCR => self.fpcr,
                    FPSR => self.fpsr,
                    DCZID_EL0 => 1 << 4, // DC ZVA prohibited
                    TPIDR_EL0 => self.tpidr_el0,
                    TPIDRRO_EL0 => self.tpidrro_el0,
                    CNTFRQ_EL0 => TIMER_HZ,
                    // Deterministic time: one tick per retired instruction.
                    CNTVCT_EL0 => self.ticks(),
                    _ => return unimpl,
                };
                self.set_xr(rt, v);
                return Ok(());
            }
            // MSR (register)
            if insn & 0xfff0_0000 == 0xd510_0000 {
                let v = self.xr(rt);
                match key {
                    NZCV => self.set_nzcv(v),
                    FPCR => self.fpcr = v,
                    FPSR => self.fpsr = v,
                    TPIDR_EL0 => self.tpidr_el0 = v,
                    _ => return unimpl,
                }
                return Ok(());
            }
            // SYS: cache maintenance (DC CVAU, IC IVAU, ...) is a no-op for us.
            if insn & 0xfff8_0000 == 0xd508_0000 {
                let _ = mem;
                return Ok(());
            }
        }
        unimpl
    }
}
