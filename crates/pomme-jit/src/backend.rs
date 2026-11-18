//! A [`Backend`] that runs every block through the front end and the IR
//! evaluator. It is slow (several times slower than the interpreter) and
//! exists only to check the front end on whole apps: a run with it must
//! retire the same instructions and show the same screen as the
//! interpreter (`POMME_CPU=ir-eval`, compared by `scripts/cpu-compare.sh`).
//! The real back ends replace the evaluator with translated code and keep
//! the rest of this loop (see design decision 6).

use crate::{eval, ir::Block, lift::lift};
use pomme_cpu::{Backend, Budget, Cpu, Exit, HOST_RETURN};
use pomme_mem::{Memory, PAGE_SIZE, untag};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Default)]
pub struct IrEval {
    blocks: HashMap<u64, Rc<Block>>,
    /// The memory's code generation the blocks were lifted at.
    generation: u64,
}

impl IrEval {
    fn block(&mut self, pc: u64, mem: &mut Memory) -> Option<Rc<Block>> {
        if self.generation != mem.code_generation() {
            match mem.code_changes_since(self.generation) {
                Some(pages) => {
                    let pages: Vec<u64> = pages.to_vec();
                    self.blocks.retain(|at, _| !pages.contains(&(untag(*at) / PAGE_SIZE)));
                }
                None => self.blocks.clear(),
            }
            self.generation = mem.code_generation();
        }
        if let Some(b) = self.blocks.get(&pc) {
            return Some(b.clone());
        }
        let ops = pomme_cpu::fetch_block(pc, mem)?;
        mem.mark_code(pc);
        let b = Rc::new(lift(pc, ops));
        self.blocks.insert(pc, b.clone());
        Some(b)
    }
}

impl Backend for IrEval {
    fn run(&mut self, cpu: &mut Cpu, mem: &mut Memory, budget: Budget) -> Exit {
        // The same stops as `Cpu::run`: the budget, or the next profiler sample.
        let limit = cpu.steps.saturating_add(budget.steps);
        let stop = if cpu.sample_at == 0 { limit } else { limit.min(cpu.sample_at) };
        loop {
            if cpu.pc == HOST_RETURN {
                return Exit::Returned;
            }
            if cpu.steps >= stop {
                return if cpu.sample_at != 0 && cpu.steps >= cpu.sample_at { Exit::Sample } else { Exit::StepLimit };
            }
            let room = stop - cpu.steps;
            let r = match self.block(cpu.pc, mem) {
                Some(b) if b.len() as u64 <= room => eval::run(&b, cpu, mem),
                _ => cpu.step(mem),
            };
            if let Err(exit) = r {
                return exit;
            }
        }
    }

    fn name(&self) -> &'static str {
        "ir-eval"
    }

    fn box_clone(&self) -> Box<dyn Backend> {
        Box::new(IrEval::default())
    }
}
