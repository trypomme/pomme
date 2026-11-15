// A tab bar controller with three tabs (SF Symbols, a badge on the third),
// as most apps have one. The probe reports the tab bar, a tab's safe area,
// each tab button's accessibility (label, value, traits, frame) and the
// labels and images drawn in the bar (`METRIC name x y width height`, in
// points; `AX`, `TEXT` and `IMAGE` lines for what's in it), first with the
// first tab selected, then after the second is selected. Recorded on the iOS
// Simulator in both designs (scripts/swiftui/metrics-simulator.sh tabbar →
// ios26.txt; COMPAT=1 → ios26-compat.txt, UIDesignRequiresCompatibility);
// tests/tab_bar.rs holds pomme to them.
//
// PROBE_VARIANT (an environment variable; metrics-simulator.sh VARIANT=…)
// tries other cases, each recorded in its own file (ios26[-compat]-VARIANT.txt):
// `more`: seven tabs, so UIKit adds its More tab; the seventh selected, then More.
// `push`: the first tab a navigation stack, pushing a screen that hides the bar.
// `dark`: the dark appearance. `landscape`: the phone turned left.
// `appearance`: colours from the bar's appearance. Variants combine: `landscape,more`.
// Without one, the lens is followed as the selection moves.

import Darwin
import UIKit

func say(_ s: String) {
  // (stderr: unbuffered, so each line is out at once, terminal or not.)
  fputs(s + "\n", stderr)
}

func report(_ name: String, _ r: CGRect) {
  say("METRIC \(name) \(r.minX) \(r.minY) \(r.width) \(r.height)")
}

let variant = ProcessInfo.processInfo.environment["PROBE_VARIANT"] ?? ""

func f(_ x: CGFloat) -> String { String(format: "%.1f", x) }

/// RGBA as 0–255, in the view's appearance.
func rgba(_ c: UIColor?, _ v: UIView) -> String {
  guard let c = c?.resolvedColor(with: v.traitCollection) else { return "-" }
  var (r, g, b, a): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
  c.getRed(&r, green: &g, blue: &b, alpha: &a)
  return [r, g, b, a].map { String(Int(($0 * 255).rounded())) }.joined(separator: ",")
}

final class Screen: UIViewController {
  init(_ title: String, _ symbol: String, badge: String? = nil) {
    super.init(nibName: nil, bundle: nil)
    self.title = title
    tabBarItem = UITabBarItem(title: title, image: UIImage(systemName: symbol), tag: 0)
    tabBarItem.badgeValue = badge
  }

  required init?(coder: NSCoder) { fatalError() }

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    let label = UILabel(frame: CGRect(x: 20, y: 200, width: 350, height: 40))
    label.text = "\(title!) screen"
    view.addSubview(label)
  }
}

final class Tabs: UITabBarController {
  var reported = false

  /// What's in the bar: its accessibility elements, and the text and images drawn in it.
  func describe(_ step: String) {
    let bar = tabBar
    report("\(step).bar", bar.frame)
    let child = selectedViewController!.view!
    report("\(step).insets", CGRect(x: child.safeAreaInsets.left, y: child.safeAreaInsets.top, width: child.safeAreaInsets.right, height: child.safeAreaInsets.bottom))
    var tab = 0
    func walk(_ v: UIView, _ depth: Int) {
      if v.isHidden || v.alpha < 0.01 { return }
      let r = v.convert(v.bounds, to: bar)
      say("VIEW \(step) \(String(repeating: "  ", count: depth))\(type(of: v)) \(f(r.minX)) \(f(r.minY)) \(f(r.width))×\(f(r.height)) bg \(rgba(v.backgroundColor, v)) alpha \(f(v.alpha))")
      if v.accessibilityLabel != nil || v.isAccessibilityElement {
        say("AXV \(step) \(type(of: v)) element \(v.isAccessibilityElement) label \(v.accessibilityLabel ?? "-") | value \(v.accessibilityValue ?? "-") | traits \(v.accessibilityTraits.rawValue) | frame \(f(r.minX)) \(f(r.minY)) \(f(r.width)) \(f(r.height))")
      }
      if v.isAccessibilityElement {
        let traits = v.accessibilityTraits.rawValue
        say("AX \(step).tab\(tab) label \(v.accessibilityLabel ?? "-") | value \(v.accessibilityValue ?? "-") | traits \(traits) | hint \(v.accessibilityHint ?? "-")")
        report("\(step).tab\(tab)", r)
        tab += 1
      }
      if let l = v as? UILabel, let t = l.text, !t.isEmpty {
        say("TEXT \(step) \(t) | size \(f(l.font.pointSize)) | weight \(l.font.fontDescriptor.object(forKey: .traits).map { "\($0)" } ?? "-") | color \(rgba(l.textColor, l)) | frame \(f(r.minX)) \(f(r.minY)) \(f(r.width)) \(f(r.height))")
        report("\(step).text.\(t)", r)
      }
      if let i = v as? UIImageView, i.image != nil {
        say("IMAGE \(step) tint \(rgba(i.tintColor, i)) | frame \(f(r.minX)) \(f(r.minY)) \(f(r.width)) \(f(r.height))")
      }
      for s in v.subviews { walk(s, depth + 1) }
    }
    walk(bar, 0)
    // As VoiceOver finds them (the bar may give its elements as a container).
    let n = bar.accessibilityElementCount()
    if n != NSNotFound {
      for i in 0..<n {
        if let e = bar.accessibilityElement(at: i) as? NSObject {
          say("AXC \(step).\(i) \(type(of: e)) label \(e.accessibilityLabel ?? "-") | value \(e.accessibilityValue ?? "-") | traits \(e.accessibilityTraits.rawValue) | frame \(e.accessibilityFrame)")
        }
      }
    }
    say("ITEMS \(step) \(bar.items?.count ?? 0) selected \(selectedIndex) tint \(rgba(bar.tintColor, bar)) unselected \(rgba(bar.unselectedItemTintColor, bar))")
  }

  /// A view of this class in the bar, if there is one.
  func find(_ name: String, in v: UIView) -> UIView? {
    if String(describing: type(of: v)) == name { return v }
    for s in v.subviews { if let found = find(name, in: s) { return found } }
    return nil
  }

  /// The texts in a view (a More list's rows).
  func texts(in v: UIView) -> [String] {
    if v.isHidden { return [] }
    var out: [String] = []
    if let l = v as? UILabel, let t = l.text, !t.isEmpty { out.append(t) }
    for s in v.subviews { out += texts(in: s) }
    return out
  }

  func after(_ s: Double, _ work: @escaping () -> Void) {
    DispatchQueue.main.asyncAfter(deadline: .now() + s, execute: work)
  }

  override func viewDidAppear(_ animated: Bool) {
    super.viewDidAppear(animated)
    guard !reported else { return }
    reported = true
    after(1) { [self] in
      describe("first")
      switch true {
      case variant.contains("more"):
        selectedIndex = 6
        after(1) { [self] in
          describe("second")
          let more = moreNavigationController
          say("MORE selected \(selectedIndex) shown \(selectedViewController === more) top \(more.topViewController?.title ?? "-") stack \(more.viewControllers.count)")
          selectedViewController = more
          more.popToRootViewController(animated: false)
          after(1) { [self] in
            say("MORELIST title \(more.topViewController?.navigationItem.title ?? "-") rows \(texts(in: more.view).joined(separator: "|"))")
            report("third.bar", tabBar.frame)
          }
        }
      case variant.contains("push"):
        let nav = viewControllers![0] as! UINavigationController
        let pushed = Screen("Pushed", "star")
        pushed.hidesBottomBarWhenPushed = true
        nav.pushViewController(pushed, animated: true)
        after(1) { [self] in
          say("PUSHED bar hidden \(tabBar.isHidden) alpha \(f(tabBar.alpha))")
          report("second.bar", tabBar.frame)
          let v = pushed.view!
          report("second.insets", CGRect(x: v.safeAreaInsets.left, y: v.safeAreaInsets.top, width: v.safeAreaInsets.right, height: v.safeAreaInsets.bottom))
          nav.popViewController(animated: true)
          after(1) { [self] in
            report("third.bar", tabBar.frame)
            say("POPPED bar hidden \(tabBar.isHidden)")
          }
        }
      default:
        // The selection moves: where the lens is, every 50 ms.
        let start = Date()
        selectedIndex = 1
        for i in 0...14 {
          after(Double(i) * 0.05) { [self] in
            if let lens = find("_UILiquidLensView", in: tabBar) {
              let r = lens.convert(lens.bounds, to: tabBar)
              say("LENS \(f(Date().timeIntervalSince(start) * 1000)) \(f(r.minX)) \(f(r.width))")
            }
          }
        }
        after(1) { [self] in
          describe("second")
        }
      }
    }
  }
}

/// UIKit answers accessibility fully only with its accessibility support
/// on (iOS turns it on for VoiceOver or a UI test); on the Simulator, turn
/// it on as Accessibility Inspector does, so the probe sees what VoiceOver
/// would. (Not there under pomme, whose UIKit answers itself.)
func loadAccessibility() {
  guard let root = ProcessInfo.processInfo.environment["IPHONE_SIMULATOR_ROOT"] else { return }
  if let lib = dlopen(root + "/usr/lib/libAccessibility.dylib", RTLD_NOW) {
    for name in ["_AXSApplicationAccessibilitySetEnabled", "_AXSSetAutomationEnabled"] {
      if let sym = dlsym(lib, name) {
        unsafeBitCast(sym, to: (@convention(c) (Int32) -> Void).self)(1)
      }
    }
  }
  for path in ["/System/Library/PrivateFrameworks/AccessibilityUtilities.framework", "/System/Library/AccessibilityBundles/UIKit.axbundle"] {
    _ = Bundle(path: root + path)?.load()
  }
}

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
  var window: UIWindow?

  func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
    loadAccessibility()
    let window = UIWindow(windowScene: scene as! UIWindowScene)
    let tabs = Tabs()
    switch true {
    case variant.contains("more"):
      tabs.viewControllers = [Screen("Home", "house.fill"), Screen("Search", "magnifyingglass"), Screen("Library", "books.vertical"), Screen("Favourites", "star"), Screen("Settings", "gear", badge: "3"), Screen("Downloads", "arrow.down.circle"), Screen("History", "clock")]
    case variant.contains("push"):
      let home = Screen("Home", "house.fill")
      let nav = UINavigationController(rootViewController: home)
      nav.tabBarItem = home.tabBarItem
      tabs.viewControllers = [nav, Screen("Search", "magnifyingglass"), Screen("Settings", "gear", badge: "3")]
    default:
      tabs.viewControllers = [Screen("Home", "house.fill"), Screen("Search", "magnifyingglass"), Screen("Settings", "gear", badge: "3")]
    }
    if variant.contains("appearance") {
      // Colours through the bar's appearance, as React Native's native tabs set them.
      let a = UITabBarAppearance()
      a.configureWithDefaultBackground()
      a.stackedLayoutAppearance.normal.iconColor = .systemRed
      a.stackedLayoutAppearance.normal.titleTextAttributes = [.foregroundColor: UIColor.systemGreen]
      a.stackedLayoutAppearance.selected.iconColor = .systemPurple
      a.stackedLayoutAppearance.selected.titleTextAttributes = [.foregroundColor: UIColor.systemOrange]
      a.stackedLayoutAppearance.normal.badgeBackgroundColor = .systemBlue
      tabs.tabBar.standardAppearance = a
      tabs.tabBar.scrollEdgeAppearance = a
    }
    if variant.contains("dark") {
      window.overrideUserInterfaceStyle = .dark
    }
    window.rootViewController = tabs
    window.makeKeyAndVisible()
    self.window = window
    if variant.contains("landscape") {
      (scene as! UIWindowScene).requestGeometryUpdate(.iOS(interfaceOrientations: .landscapeRight))
    }
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
