// Liquid Glass views (iOS 26's UIGlassEffect in a UIVisualEffectView), as
// Expo's expo-glass-effect and Callstack's liquid-glass give React Native
// apps: regular, clear and tinted glass, rounded three ways, a container
// (UIGlassContainerEffect) of two, and glass with a label in its content
// view; over white and over stripes. The clear row is made the way
// Callstack's view does (a UIVisualEffectView subclass setting its effect
// as it's first laid out in a window), the corners row the way Expo's does
// (an effect view filling a plain view, its effect set after layout). The probe reports each effect view's
// structure (`VIEW` lines: the views UIKit puts in it, their frames,
// corners and colours) and frames (`METRIC`); what the glass looks like is
// sampled from a screenshot (scripts/swiftui/sample-pixels.swift).
// Recorded on the iOS Simulator (scripts/swiftui/metrics-simulator.sh glass
// → ios26.txt; VARIANT=dark: the dark appearance → ios26-dark.txt);
// tests/glass.rs holds pomme to it.

import UIKit

func say(_ s: String) {
  fputs(s + "\n", stderr)
}

func report(_ name: String, _ r: CGRect) {
  say("METRIC \(name) \(r.minX) \(r.minY) \(r.width) \(r.height)")
}

func f(_ x: CGFloat) -> String { String(format: "%.1f", x) }

func rgba(_ c: UIColor?, _ v: UIView) -> String {
  guard let c = c?.resolvedColor(with: v.traitCollection) else { return "-" }
  var (r, g, b, a): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
  c.getRed(&r, green: &g, blue: &b, alpha: &a)
  return [r, g, b, a].map { String(Int(($0 * 255).rounded())) }.joined(separator: ",")
}

/// Rows of glass, 350 by 80 pt from x 20, over stripes on the left half.
let rows: [(String, CGFloat)] = [("regular", 120), ("clear", 240), ("tinted", 360), ("corners", 480), ("container", 600), ("label", 720)]

/// Glass as Callstack's liquid-glass makes it: no effect until laid out in
/// a window; then cleared and set without animation, tinted and
/// interactive as asked, its corners the host layer's.
@available(iOS 26.0, *)
final class LaidOutGlass: UIVisualEffectView {
  var laidOut = false
  var style: UIGlassEffect.Style = .clear

  override func layoutSubviews() {
    super.layoutSubviews()
    guard window != nil, !laidOut else { return }
    laidOut = true
    UIView.performWithoutAnimation { self.effect = UIVisualEffect() }
    let glass = UIGlassEffect(style: style)
    glass.isInteractive = false
    glass.tintColor = nil
    UIView.performWithoutAnimation { self.effect = glass }
    contentView.isUserInteractionEnabled = true
  }
}

/// Glass as Expo's expo-glass-effect makes it: an effect view filling a
/// plain view, given its corners, and its effect once the view's laid out.
@available(iOS 26.0, *)
final class GlassHost: UIView {
  let effectView = UIVisualEffectView()
  var installed = false

  override init(frame: CGRect) {
    super.init(frame: frame)
    effectView.frame = bounds
    effectView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    addSubview(effectView)
    effectView.cornerConfiguration = .corners(topLeftRadius: 24, topRightRadius: 24, bottomLeftRadius: 24, bottomRightRadius: 24)
  }

  required init?(coder: NSCoder) { fatalError() }

  override func layoutSubviews() {
    super.layoutSubviews()
    guard !installed, window != nil else { return }
    installed = true
    effectView.effect = UIVisualEffect()
    effectView.effect = UIGlassEffect(style: .regular)
  }
}

@available(iOS 26.0, *)
final class Probe: UIViewController {
  var effects: [(String, UIVisualEffectView)] = []

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .white
    // Stripes: 25-pt black and orange bars across the left half, top to bottom.
    var x: CGFloat = 20
    var i = 0
    while x < 195 {
      let bar = UIView(frame: CGRect(x: x, y: 100, width: 25, height: 720))
      bar.backgroundColor = i % 2 == 0 ? .black : .systemOrange
      view.addSubview(bar)
      x += 25
      i += 1
    }
    func glass(_ name: String, _ effect: UIVisualEffect?, _ frame: CGRect) -> UIVisualEffectView {
      let v = UIVisualEffectView(effect: effect)
      v.frame = frame
      view.addSubview(v)
      effects.append((name, v))
      return v
    }
    func row(_ name: String) -> CGRect {
      CGRect(x: 20, y: rows.first { $0.0 == name }!.1, width: 350, height: 80)
    }
    // Regular, as made: no corners asked for.
    _ = glass("regular", UIGlassEffect(style: .regular), row("regular"))
    // Clear, with the layer's corner radius, made as Callstack's view is.
    let clear = LaidOutGlass(effect: nil)
    clear.frame = row("clear")
    clear.layer.cornerRadius = 20
    view.addSubview(clear)
    effects.append(("clear", clear))
    // Tinted, interactive, a capsule.
    let tinted = UIGlassEffect(style: .regular)
    tinted.tintColor = .systemBlue
    tinted.isInteractive = true
    let t = glass("tinted", tinted, row("tinted"))
    t.cornerConfiguration = .capsule()
    // Corners, made as Expo's view is.
    let host = GlassHost(frame: row("corners"))
    view.addSubview(host)
    effects.append(("corners", host.effectView))
    // A container: two 80-pt circles 20 pt apart, merging within 40.
    let container = UIGlassContainerEffect()
    container.spacing = 40
    let holder = glass("container", container, row("container"))
    for (j, cx) in [CGFloat(60), 160].enumerated() {
      let one = UIVisualEffectView(effect: UIGlassEffect(style: .regular))
      one.frame = CGRect(x: cx, y: 0, width: 80, height: 80)
      one.cornerConfiguration = .capsule()
      holder.contentView.addSubview(one)
      effects.append(("container.\(j)", one))
    }
    // Glass holding a label.
    let l = glass("label", UIGlassEffect(style: .regular), row("label"))
    l.cornerConfiguration = .capsule()
    let label = UILabel(frame: CGRect(x: 24, y: 28, width: 300, height: 24))
    label.text = "On glass"
    l.contentView.addSubview(label)
  }

  override func viewDidAppear(_ animated: Bool) {
    super.viewDidAppear(animated)
    DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [self] in
      for (name, v) in effects {
        report("\(name).frame", v.convert(v.bounds, to: view))
        report("\(name).content", v.contentView.convert(v.contentView.bounds, to: view))
        say("EFFECT \(name) \(type(of: v.effect.map { $0 as AnyObject } ?? NSNull())) corner \(f(v.layer.cornerRadius)) clips \(v.clipsToBounds) style \(v.traitCollection.userInterfaceStyle.rawValue)")
        walk(v, name, 0)
      }
      for (name, y) in rows {
        say("ROW \(name) \(y)")
      }
    }
  }

  func walk(_ v: UIView, _ name: String, _ depth: Int) {
    let r = v.convert(v.bounds, to: view)
    say("VIEW \(name) \(String(repeating: "  ", count: depth))\(type(of: v)) \(f(r.minX)) \(f(r.minY)) \(f(r.width))×\(f(r.height)) bg \(rgba(v.backgroundColor, v)) alpha \(f(v.alpha)) corner \(f(v.layer.cornerRadius)) hidden \(v.isHidden)")
    for s in v.subviews { walk(s, name, depth + 1) }
  }
}

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
  var window: UIWindow?

  func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
    let window = UIWindow(windowScene: scene as! UIWindowScene)
    if ProcessInfo.processInfo.environment["PROBE_VARIANT"] == "dark" {
      window.overrideUserInterfaceStyle = .dark
    }
    if #available(iOS 26.0, *) {
      window.rootViewController = Probe()
    }
    window.makeKeyAndVisible()
    self.window = window
  }
}

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
  func application(_ application: UIApplication, configurationForConnecting session: UISceneSession, options: UIScene.ConnectionOptions) -> UISceneConfiguration {
    let config = UISceneConfiguration(name: nil, sessionRole: session.role)
    config.delegateClass = SceneDelegate.self
    return config
  }
}
