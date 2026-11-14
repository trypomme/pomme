// A tab bar controller with three tabs (SF Symbols, a badge on the third),
// as most apps have one. The probe reports the tab bar, a tab's safe area,
// each tab button's accessibility (label, value, traits, frame) and the
// labels and images drawn in the bar (`METRIC name x y width height`, in
// points; `AX`, `TEXT` and `IMAGE` lines for what's in it), first with the
// first tab selected, then after the second is selected. Recorded on the iOS
// Simulator in both designs (scripts/swiftui/metrics-simulator.sh tabbar →
// ios26.txt; COMPAT=1 → ios26-compat.txt, UIDesignRequiresCompatibility);
// tests/tab_bar.rs holds pomme to them.

import Darwin
import UIKit

func say(_ s: String) {
  // (stderr: unbuffered, so each line is out at once, terminal or not.)
  fputs(s + "\n", stderr)
}

func report(_ name: String, _ r: CGRect) {
  say("METRIC \(name) \(r.minX) \(r.minY) \(r.width) \(r.height)")
}

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

  override func viewDidAppear(_ animated: Bool) {
    super.viewDidAppear(animated)
    guard !reported else { return }
    reported = true
    DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [self] in
      describe("first")
      selectedIndex = 1
      DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [self] in
        describe("second")
      }
    }
  }
}

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
  var window: UIWindow?

  func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
    let window = UIWindow(windowScene: scene as! UIWindowScene)
    let tabs = Tabs()
    tabs.viewControllers = [Screen("Home", "house.fill"), Screen("Search", "magnifyingglass"), Screen("Settings", "gear", badge: "3")]
    window.rootViewController = tabs
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
