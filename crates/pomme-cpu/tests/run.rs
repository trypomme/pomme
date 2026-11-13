//! CPU conformance: compile `tests/fixtures/cpu.c` for iOS at several optimization
//! levels, load it with pomme-loader, call each function on the interpreter, and
//! compare against a Rust reference. Needs Xcode + the iOS SDK; skipped otherwise.

use pomme_cpu::{Cpu, Exit, HOST_RETURN};
use pomme_loader::{Process, Prot, load};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const STACK_TOP: u64 = 0x3_0000_0000;
const STACK_SIZE: u64 = 1 << 20;
const STEP_LIMIT: u64 = 50_000_000;

const VARIANTS: &[(&str, &[&str])] = &[
    ("O0", &["-O0"]),
    ("O2-novec", &["-O2", "-fno-vectorize", "-fno-slp-vectorize"]),
    ("Os", &["-Os"]),
    ("O2", &["-O2"]),
];

fn build(variant: &str, flags: &[&str]) -> Option<PathBuf> {
    let ok = Command::new("xcrun").args(["--sdk", "iphoneos", "--show-sdk-path"]).output();
    if !ok.is_ok_and(|o| o.status.success()) {
        return None;
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cpu.c");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("libcpu-{variant}.dylib"));
    let status = Command::new("xcrun")
        .args(["--sdk", "iphoneos", "clang", "-target", "arm64-apple-ios16.0", "-dynamiclib"])
        .args(flags)
        .arg(&src)
        .arg("-o")
        .arg(&out)
        .args(["-install_name", "@rpath/libcpu.dylib"])
        .status()
        .expect("run clang");
    assert!(status.success(), "building {variant} failed");
    Some(out)
}

/// Build each variant once per test binary.
fn dylibs() -> &'static [(String, PathBuf)] {
    static CELL: OnceLock<Vec<(String, PathBuf)>> = OnceLock::new();
    CELL.get_or_init(|| {
        VARIANTS
            .iter()
            .filter_map(|(name, flags)| build(name, flags).map(|p| (name.to_string(), p)))
            .collect()
    })
}

struct Guest {
    p: Process,
    cpu: Cpu,
}

#[derive(Clone, Copy)]
enum Arg {
    I(u64),
    F(f32),
    D(f64),
}

impl Guest {
    fn new(path: &Path) -> Self {
        let mut p = load(path).expect("load");
        p.memory.map(STACK_TOP - STACK_SIZE, STACK_SIZE, Prot::RW, "[stack]").unwrap();
        Guest { p, cpu: Cpu::new() }
    }

    fn sym(&self, name: &str) -> u64 {
        self.p.main().symbol(&format!("_{name}")).unwrap_or_else(|| panic!("no symbol {name}"))
    }

    /// Set up an AAPCS64 call and run until it returns or traps.
    fn start(&mut self, f: &str, args: &[Arg]) -> Exit {
        let mut cpu = Cpu::new();
        cpu.sp = STACK_TOP - 0x1000;
        let (mut ni, mut nf, mut stack) = (0, 0, Vec::new());
        for a in args {
            match *a {
                Arg::I(v) if ni < 8 => {
                    cpu.x[ni] = v;
                    ni += 1;
                }
                Arg::I(v) => stack.push(v),
                Arg::F(v) => {
                    cpu.q[nf] = v.to_bits() as u128;
                    nf += 1;
                }
                Arg::D(v) => {
                    cpu.q[nf] = v.to_bits() as u128;
                    nf += 1;
                }
            }
        }
        for (i, v) in stack.iter().enumerate() {
            self.p.memory.write_u64(cpu.sp + 8 * i as u64, *v).unwrap();
        }
        cpu.x[30] = HOST_RETURN;
        cpu.pc = self.sym(f);
        self.cpu = cpu;
        self.cpu.run(&mut self.p.memory, STEP_LIMIT)
    }

    /// Emulate a few libSystem functions on the host (a preview of the HLE layer).
    /// Returns false if the stub isn't one we know.
    fn hle(&mut self) -> bool {
        let Some(stub) = self.p.stubs.at(self.cpu.pc) else { return false };
        let (a, b, n) = (self.cpu.x[0], self.cpu.x[1], self.cpu.x[2]);
        let mem = &mut self.p.memory;
        match stub.symbol.as_str() {
            "_memcpy" | "_memmove" => {
                let bytes = mem.read(b, n as usize).unwrap().to_vec();
                mem.write(a, &bytes).unwrap();
            }
            "_memset" => mem.write(a, &vec![b as u8; n as usize]).unwrap(),
            "_bzero" => mem.write(a, &vec![0; b as usize]).unwrap(),
            _ => return false,
        }
        // Return to the caller; memcpy/memmove/memset return their first argument.
        self.cpu.pc = self.cpu.x[30];
        true
    }

    fn call(&mut self, f: &str, args: &[Arg]) -> &Cpu {
        let mut exit = self.start(f, args);
        while matches!(exit, Exit::Brk { .. }) && self.hle() {
            exit = self.cpu.run(&mut self.p.memory, STEP_LIMIT);
        }
        if exit != Exit::Returned {
            let insn = self.p.memory.read_u32(self.cpu.pc).unwrap_or(0);
            panic!("{f}: {exit:x?} at pc {:#x} (insn {insn:#010x})", self.cpu.pc);
        }
        &self.cpu
    }
}

fn each_variant(mut f: impl FnMut(&str, &Path)) {
    let libs = dylibs();
    if libs.is_empty() {
        eprintln!("skipping: no iOS SDK");
        return;
    }
    for (name, path) in libs {
        f(name, path);
    }
}

/// Call an integer-returning function in a fresh process for every variant.
fn check_int(f: &str, args: &[Arg], expect: u64) {
    each_variant(|v, path| {
        let got = Guest::new(path).call(f, args).x[0];
        assert_eq!(got, expect, "{f}{args_dbg} [{v}]: got {got:#x}, want {expect:#x}", args_dbg = dbg_args(args));
    });
}

fn check_f64(f: &str, args: &[Arg], expect: f64) {
    each_variant(|v, path| {
        let got = f64::from_bits(Guest::new(path).call(f, args).q[0] as u64);
        assert!((got - expect).abs() <= expect.abs() * 1e-12, "{f} [{v}]: got {got}, want {expect}");
    });
}

fn check_f32(f: &str, args: &[Arg], expect: f32) {
    each_variant(|v, path| {
        let got = f32::from_bits(Guest::new(path).call(f, args).q[0] as u32);
        assert!((got - expect).abs() <= expect.abs() * 1e-6, "{f} [{v}]: got {got}, want {expect}");
    });
}

fn dbg_args(args: &[Arg]) -> String {
    let parts: Vec<String> = args
        .iter()
        .map(|a| match a {
            Arg::I(v) => format!("{v:#x}"),
            Arg::F(v) => format!("{v}f"),
            Arg::D(v) => format!("{v}"),
        })
        .collect();
    format!("({})", parts.join(", "))
}

use Arg::{D, F, I};

#[test]
fn integer_loops() {
    let fib = |n: u32| (0..n).fold((0u64, 1u64), |(a, b), _| (b, a.wrapping_add(b))).0;
    for n in [0, 1, 2, 10, 50, 93, 100] {
        check_int("fib", &[I(n as u64)], fib(n));
    }
    let sum_sq = |n: i32| (0..n.min(64)).map(|i| ((i - 20) as i64).pow(2)).sum::<i64>() as u64;
    for n in [0, 1, 7, 33, 64, 100] {
        check_int("sum_squares", &[I(n as u64)], sum_sq(n));
    }
    for x in [0u64, 1, 0xff, 0x8000_0000_0000_0001, u64::MAX] {
        check_int("popcount_loop", &[I(x)], x.count_ones() as u64);
    }
}

#[test]
fn division_and_multiplication() {
    for (a, b) in [(100i64, 7i64), (-100, 7), (100, -7), (i64::MAX, 3), (5, 10)] {
        check_int("divmod", &[I(a as u64), I(b as u64)], ((a / b).wrapping_mul(1000).wrapping_add(a % b)) as u64);
    }
    for (a, b) in [(100u32, 7u32), (u32::MAX, 10), (3, 9)] {
        check_int("udivmod32", &[I(a as u64), I(b as u64)], (a / b).wrapping_mul(1000).wrapping_add(a % b) as u64);
    }
    for (a, b) in [(u64::MAX, u64::MAX), (0x1234_5678_9abc_def0, 0xfedc_ba98_7654_3210), (3, 5)] {
        check_int("mulhi", &[I(a), I(b)], ((a as u128 * b as u128) >> 64) as u64);
        let (sa, sb) = (a as i64, b as i64);
        check_int("smulhi", &[I(a), I(b)], ((sa as i128 * sb as i128) >> 64) as u64);
    }
}

#[test]
fn switch_jump_table() {
    let table = [11, 23, 37, 41, 53, 67, 71, 89, 97, 101];
    for x in -2i32..12 {
        let want = if (0..10).contains(&x) { table[x as usize] } else { -1 };
        check_int("classify", &[I(x as u32 as u64)], want as i64 as u32 as u64);
    }
}

#[test]
fn bit_manipulation() {
    let mix = |x: u64| {
        let mut r = ((x | 1).leading_zeros() as u64).wrapping_add(((x | (1 << 63)).trailing_zeros() as u64) << 8);
        r ^= x.swap_bytes();
        r = r.wrapping_add((x as u32).reverse_bits() as u64);
        r = r.wrapping_add(x.rotate_right(13));
        r = r.wrapping_add((x >> 3) as u8 as i8 as i64 as u64);
        r = r.wrapping_add((x >> 7) as u16 as i16 as i64 as u64);
        r = r.wrapping_add((x >> 1) as u32 as i32 as i64 as u64);
        r
    };
    for x in [0u64, 1, 0xdead_beef_cafe_babe, u64::MAX, 0x8000_0000_0000_0000] {
        check_int("bits_mix", &[I(x)], mix(x));
    }
    let bf = |seed: u32| {
        let a = seed & 7;
        let b = (seed >> 3) & 0x7f;
        let c = seed.wrapping_mul(7) & 0x7ff;
        let d = (a + b + c) & 0x7ff;
        a.wrapping_mul(1_000_000).wrapping_add(b * 10_000).wrapping_add(c * 10).wrapping_add(d)
    };
    for s in [0u32, 1, 12345, u32::MAX] {
        check_int("bitfields", &[I(s as u64)], bf(s) as u64);
    }
}

#[test]
fn floating_point() {
    for x in [0.0, 1.0, -3.5, 1e10, 0.1] {
        check_f64("fp_poly", &[D(x)], 3.0 * x * x - 2.5 * x + 1.25);
    }
    for (a, b) in [(3.0f32, 2.0f32), (1.0, 4.0), (-2.0, -8.0)] {
        check_f32("fp_mix", &[F(a), F(b)], if a > b { a / b + 0.5 } else { b * a - 1.0 });
    }
    for x in [0.0f64, 1.9, 123456.75, 4e9] {
        let want = (x as i64).wrapping_add((x * 2.0) as u32 as i64).wrapping_add((x as f32) as i64);
        check_int("fp_to_int", &[D(x)], want as u64);
    }
    for (a, b) in [(0i64, 0u32), (-7, 3), (1 << 40, u32::MAX)] {
        check_f64("int_to_fp", &[I(a as u64), I(b as u64)], a as f64 / 3.0 + b as f64 * 0.25 + (a as f32) as f64);
    }
    for x in [4.0, -9.0, 2.0] {
        check_f64("fp_sqrt_abs", &[D(x)], x.abs().sqrt());
    }
}

#[test]
fn strings_and_stack() {
    let fnv = |s: &[u8]| s.iter().fold(2166136261u32, |h, &c| (h ^ c as u32).wrapping_mul(16777619));
    for n in [0u32, 1, 2] {
        let s: Vec<u8> = b"pomme runs arm64 code".iter().map(|&c| c.wrapping_add((n % 3) as u8)).collect();
        check_int("hash_local_string", &[I(n as u64)], fnv(&s) as u64);
    }
    check_int("ackermann", &[I(2), I(3)], 9);
    check_int("ackermann", &[I(3), I(3)], 61);

    let sort = |mut seed: u32| {
        let mut a: Vec<i32> = (0..24)
            .map(|_| {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                (seed >> 8) as i32 - (1 << 22)
            })
            .collect();
        a.sort();
        a.iter().fold(0i64, |s, &v| s.wrapping_mul(31).wrapping_add(v as i64))
    };
    for seed in [1u32, 42, 0xdead_beef] {
        check_int("insertion_sort_checksum", &[I(seed as u64)], sort(seed) as u64);
    }
    let sc = |x: u64| {
        let w = [x, x.wrapping_add(1), x.wrapping_mul(3), x ^ 0xff, x << 4, !x];
        w[0].wrapping_add(w[1])
            .wrapping_add(w[2].wrapping_add(w[5]))
            .wrapping_add(w[3])
            .wrapping_add(w[4])
            .wrapping_add(w[5])
    };
    for x in [0u64, 7, u64::MAX] {
        check_int("struct_copy", &[I(x)], sc(x));
    }
}

#[test]
fn atomics_and_calling_convention() {
    check_int("atomics", &[I(0)], 1);
    check_int("atomics", &[I(10)], 45);
    let args: Vec<i64> = (1..=10).map(|i| i * 3 - 7).collect();
    let [a, b, c, d, e, f, g, h, i, j] = args[..] else { unreachable!() };
    let want = a - b + c * d - e + f * g - h + i * j;
    check_int("many_args", &args.iter().map(|&v| I(v as u64)).collect::<Vec<_>>(), want as u64);
}

#[test]
fn vectorized_struct_math() {
    // Mirrors corner_insets_sum in cpu.c (React Native's RCTGetCornerInsets).
    let reference = |r: f64, b: f64| {
        let (top, left, bottom, right) = (b, b * 2.0, b * 3.0, b * 4.0);
        let m = |x: f64| x.max(0.0);
        m(r - left) + 2.0 * m(r + 1.0 - top) + 4.0 * m(r + 2.0 - right) + 8.0 * m(r + 3.0 - top)
            + 16.0 * m(r + 4.0 - left) + 32.0 * m(r + 5.0 - bottom) + 64.0 * m(r + 6.0 - right) + 128.0 * m(r + 7.0 - bottom)
    };
    for (r, b) in [(12.0, 1.0), (0.0, 0.0), (3.0, 2.5), (100.0, 9.0)] {
        check_f64("corner_insets_sum", &[D(r), D(b)], reference(r, b));
    }
}

#[test]
fn vectorized_float_radii() {
    // Mirrors no_overlap_sum in cpu.c (React Native Fabric's ensureNoOverlap).
    let reference = |radius: f64, w: f64, h: f64| {
        let x = radius as f32;
        let edge = x + x;
        let s = |len: f64| if edge > 0.0 { ((len / edge as f64) as f32).min(1.0) } else { 0.0 };
        let (ls, ts) = (s(h), s(w));
        let k = ts.min(ls);
        // The C sums in float, left to right.
        (0..8).fold(0f32, |acc, i| acc + (x * k) * (1u32 << i) as f32) as f64
    };
    for (r, w, h) in [(12.0, 300.0, 109.67), (100.0, 50.0, 30.0), (0.0, 10.0, 10.0)] {
        check_f64("no_overlap_sum", &[D(r), D(w), D(h)], reference(r, w, h));
    }
}

#[test]
fn float_to_double_widening() {
    // Mirrors widen_sum in cpu.c: FCVTL must fill both doubles.
    let reference = |a: f64, b: f64| {
        let (a32, b32) = (a as f32, b as f32);
        let pairs = [(a32, b32), (b32, a32), ((a + 1.0) as f32, (b + 1.0) as f32), ((b - 1.0) as f32, (a - 1.0) as f32)];
        pairs.iter().fold(0.0, |s, &(v, h)| s * 3.0 + h as f64 * 5.0 + v as f64)
    };
    for (a, b) in [(12.0, 7.5), (0.0, 3.0), (-2.25, 100.0)] {
        check_f64("widen_sum", &[D(a), D(b)], reference(a, b));
    }
}

#[test]
fn shift_left_long() {
    // Mirrors shll_mix in cpu.c: SHLL / SHLL2 on 32- and 16-bit lanes.
    let reference = |a: u32, b: u32, c: u16| {
        let v = [a as i32, b as i32, (a ^ b) as i32, -7];
        let shl = |x: i32| ((x as i64) << 32) as u64;
        let mut s: u64 = 0;
        s = s.wrapping_mul(31).wrapping_add(shl(v[0])).wrapping_add(shl(v[1]));
        s = s.wrapping_mul(31).wrapping_add(shl(v[2])).wrapping_add(shl(v[3]));
        let w = |i: usize| if i == 5 { c.wrapping_add(3) } else { c } as u64;
        s.wrapping_mul(31).wrapping_add(w(0) << 16).wrapping_add(w(5) << 16)
    };
    for (a, b, c) in [(1, 2, 3), (0xdead_beef, 0x1234_5678, 0xfff0), (0, u32::MAX, 0)] {
        check_int("shll_mix", &[I(a as u64), I(b as u64), I(c as u64)], reference(a, b, c));
    }
}

#[test]
fn scalar_multiply_by_element() {
    // Mirrors scale_by_lane in cpu.c (the C computes in float, then double).
    let reference = |a: f32, b: f32, c: f32| {
        let x = a.mul_add(a + b, c * b);
        (x as f64 * 4.0) as f32
    };
    for (a, b, c) in [(1.5f32, 2.0f32, -3.0f32), (0.25, 100.0, 7.0)] {
        check_f32("scale_by_lane", &[F(a), F(b), F(c)], reference(a, b, c));
    }
}

/// The HLE loop in miniature: trap on a system-library stub, emulate it on the
/// host, and resume the guest.
#[test]
fn hle_stub_trap_and_resume() {
    each_variant(|v, path| {
        let mut g = Guest::new(path);
        let exit = g.start("calls_puts", &[]);
        assert!(matches!(exit, Exit::Brk { .. }), "[{v}] expected a stub trap, got {exit:?}");
        let stub = g.p.stubs.at(g.cpu.pc).expect("trap should be in a stub").clone();
        assert_eq!(stub.symbol, "_puts", "[{v}]");
        let msg = g.p.memory.read_cstr(g.cpu.x[0], 256).unwrap();
        assert_eq!(msg, "hello from the guest", "[{v}]");

        // "Implement" puts: return a non-negative value to the caller.
        g.cpu.x[0] = msg.len() as u64 + 1;
        g.cpu.pc = g.cpu.x[30];
        let exit = g.cpu.run(&mut g.p.memory, STEP_LIMIT);
        assert_eq!(exit, Exit::Returned, "[{v}]");
        assert_eq!(g.cpu.x[0] as u32, msg.len() as u32 + 2, "[{v}]");
    });
}

#[test]
fn half_precision_conversions() {
    // 1.0, -2.5, 65504 (the largest half), 0.1 (rounded) → their binary16 bits.
    let packed = 0x3c00u64 | (0xc100 << 16) | (0x7bff << 32) | (0x2e66 << 48);
    check_int("to_halves", &[F(1.0), F(-2.5), F(65504.0), F(0.1)], packed);
    // Overflow is infinity; 1e-7 is the subnormal 2·2⁻²⁴; -0 keeps its sign.
    let packed = 0x7c00u64 | (0x0002 << 16) | (0x8000 << 32) | (0x3555 << 48);
    check_int("to_halves", &[F(70000.0), F(1e-7), F(-0.0), F(1.0 / 3.0)], packed);
    // And back: 1 + 2·(-2.5) + 4·65504 + 8·0.0999755859375.
    let back = 1.0f32 + 2.0 * -2.5 + 4.0 * 65504.0 + 8.0 * 0.099_975_586;
    check_f32("from_halves", &[I(0x3c00 | (0xc100 << 16) | (0x7bff << 32) | (0x2e66 << 48))], back);
}

#[test]
fn rounding_conversions() {
    // floor(-1.5) = -2, floor(2.5) = 2, ceil(-1.5) = -1, nearest-even(2.5) = 2.
    check_int("round_lanes", &[F(-1.5), F(2.5)], (-2i64 * 1_000_000 + 2 * 10_000 - 100 + 2) as u64);
    // floor(3.7) = 3, floor(-0.5) = -1, ceil(3.7) = 4, nearest-even(-0.5) = -0.
    check_int("round_lanes", &[F(3.7), F(-0.5)], (3i64 * 1_000_000 - 10_000 + 4 * 100) as u64);
}

#[test]
fn dot_product_extension() {
    let reference = |mut seed: u64| {
        let (mut a, mut b) = ([0u8; 16], [0u8; 16]);
        for i in 0..16 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            a[i] = (seed >> 56) as u8;
            b[i] = (seed >> 48) as u8;
        }
        let lane = |acc: u32, k: usize, x: &dyn Fn(usize) -> u32| (0..4).fold(acc, |s, j| s.wrapping_add(x(4 * k + j)));
        let u: u32 = (0..4).map(|k| lane(7, k, &|i| a[i] as u32 * b[i] as u32)).fold(0, u32::wrapping_add);
        let s: u32 = (0..4).map(|k| lane(-3i32 as u32, k, &|i| (a[i] as i8 as i32 * b[i] as i8 as i32) as u32)).fold(0, u32::wrapping_add);
        // By element: every lane of a against b's third 32-bit group.
        let l: u32 = (0..4).map(|k| (0..4).fold(0u32, |acc, j| acc.wrapping_add((a[4 * k + j] as i8 as i32 * b[8 + j] as i8 as i32) as u32))).fold(0, u32::wrapping_add);
        // 64-bit form: a's low half against b's high half.
        let h: u32 = (0..2).map(|k| (0..4).fold(1u32, |acc, j| acc.wrapping_add(a[4 * k + j] as u32 * b[8 + 4 * k + j] as u32))).fold(0, u32::wrapping_add);
        (u as u64).wrapping_mul(3).wrapping_add((s as u64).wrapping_mul(5)).wrapping_add((l as u64).wrapping_mul(7)).wrapping_add(h as u64)
    };
    for seed in [0u64, 1, 0xdead_beef, u64::MAX] {
        check_int("dot_products", &[I(seed)], reference(seed));
    }
}

#[test]
fn crc32_extension() {
    // The bit-at-a-time definition, independent of the interpreter's.
    let step = |mut crc: u32, data: u64, bytes: u32, poly: u32| {
        for i in 0..bytes {
            crc ^= (data >> (8 * i)) as u8 as u32;
            for _ in 0..8 {
                crc = (crc >> 1) ^ if crc & 1 != 0 { poly } else { 0 };
            }
        }
        crc
    };
    let reference = |x: u64| {
        let (p, pc) = (0xEDB8_8320, 0x82F6_3B78);
        let a = step(step(step(step(0xffff_ffff, x & 0xff, 1, p), (x >> 8) & 0xffff, 2, p), (x >> 16) & 0xffff_ffff, 4, p), x, 8, p);
        let c = step(step(step(step(0, x & 0xff, 1, pc), x & 0xffff, 2, pc), x & 0xffff_ffff, 4, pc), !x, 8, pc);
        (a as u64) << 32 | c as u64
    };
    for x in [0u64, 1, 0x0123_4567_89ab_cdef, u64::MAX] {
        check_int("crc_mix", &[I(x)], reference(x));
    }
}
