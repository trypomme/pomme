//! Swift's `String(format:)` with every kind of argument: Int, Double,
//! CGFloat, Float, String and NSString, Int64 and UInt
//! (tests/fixtures/stringformat). Each goes through `withVaList` to
//! CoreFoundation's formatter, as on iOS.

mod common;

#[test]
fn string_format_takes_every_kind_of_argument() {
    let Some(mut emu) = common::boot("stringformat") else { return };
    emu.time_limit_ns = Some(2_000_000_000);
    let r = emu.run_main();
    let out: Vec<&str> = emu.console.lines().filter(|l| l.starts_with("FMT ")).collect();
    eprintln!("{r:?}\n{}", out.join("\n"));
    if let Err(pomme_hle::Stop::Cpu { pc, .. }) = &r {
        panic!("crashed at {} (lr {})\n{}", emu.symbolize(*pc), emu.symbolize(emu.cpu.x[30]), out.join("\n"));
    }
    assert_eq!(
        out,
        [
            "FMT int 42|    7|ff",
            "FMT double 1.2|3.142|0.5",
            "FMT cgfloat 92.3",
            "FMT float 2.50",
            "FMT string a and b",
            "FMT mixed x is 3 (42.5%)",
            "FMT int64 -9 9",
            "FMT done",
        ]
    );
}
