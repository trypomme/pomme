//! Layer shadows match iOS's (tests/fixtures/shadows/main.m, the scene
//! scripts/measure/shadows screenshotted in the iOS 26 Simulator). There a
//! shadow's edge fell off as a Gaussian of standard deviation shadowRadius
//! (fitted within 0.4%), at shadowOpacity, moved by shadowOffset, following
//! what the layer draws. Built with the iOS SDK; skipped without Xcode or the sysroot.

mod common;

/// What iOS showed: shadow alpha `d` pt outside a straight edge.
fn ios(d: f64, radius: f64, opacity: f64) -> f64 {
    opacity * 0.5 * erfc(d / (radius * std::f64::consts::SQRT_2))
}

/// The complementary error function (Abramowitz and Stegun 7.1.26, to 1.5e-7).
fn erfc(x: f64) -> f64 {
    if x < 0.0 {
        return 2.0 - erfc(-x);
    }
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t * (0.254_829_592 + t * (-0.284_496_736 + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    poly * (-x * x).exp()
}

#[test]
fn shadows_match_ios() {
    let Some(mut emu) = common::boot("shadows") else { return };
    common::views(&mut emu, 1.0);
    let scale = 3.0;
    let pix = pomme_hle::render::render(&mut emu, scale as f32).unwrap();
    // Shadow alpha at a point (pt) on white: 1 − red/255 (the squares are red, so only outside them).
    let shade = |x: f64, y: f64| 1.0 - pix.pixel((x * scale) as u32, (y * scale) as u32).unwrap().red() as f64 / 255.0;
    // Above each square's top edge, at its middle: (centre x, top y, radius, opacity).
    for (cx, top, radius, opacity) in [(60.0, 120.0, 4.0, 1.0), (190.0, 120.0, 10.0, 1.0), (320.0, 120.0, 20.0, 1.0), (320.0, 320.0, 10.0, 0.5)] {
        for d in [1.0, 2.0, 4.0, 8.0, 12.0, 20.0, 30.0] {
            if d > 3.0 * radius {
                continue;
            }
            let (ours, theirs) = (shade(cx, top - d), ios(d, radius, opacity));
            assert!((ours - theirs).abs() <= 0.03, "radius {radius} opacity {opacity}, {d} pt out: {ours:.3}, iOS {theirs:.3}");
        }
    }
    // Offset (0, 10): the shadow moved down 10 pt, so its top edge is 10 pt inside the square.
    for d in [3.0, 6.0, 10.0, 20.0] {
        let (ours, theirs) = (shade(60.0, 320.0 - d), ios(d + 10.0, 10.0, 1.0));
        assert!((ours - theirs).abs() <= 0.03, "offset shadow {d} pt above: {ours:.3}, iOS {theirs:.3}");
    }
    assert!(shade(60.0, 405.0) > 0.6, "below: {:.3}", shade(60.0, 405.0));
    // Corner radius 20: the shadow is round there too (none off the corner, just outside the box).
    assert!(shade(152.0, 322.0) > shade(151.0, 318.0) - 0.5);
    // A view without a background shadows only what it draws: its 40 pt subview, not its 80 pt box.
    assert!(shade(190.0, 538.0) > 0.4, "{}", shade(190.0, 538.0));
    assert!(shade(190.0, 522.0) < 0.15, "{}", shade(190.0, 522.0));
}
