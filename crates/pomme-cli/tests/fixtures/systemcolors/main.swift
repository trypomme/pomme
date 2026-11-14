// UIKit's system colours as an app reads them: each resolved against light
// and dark traits (`METRIC name.light|dark r g b a`, sRGB components).
// Recorded on the iOS Simulator (scripts/swiftui/metrics-simulator.sh
// systemcolors → ios26.txt); tests/appearance.rs holds pomme to it. Then the
// app's own appearance, as the window's traits report it (`APPEARANCE style`),
// at launch and when it changes.

import Darwin
import UIKit

let colors: [(String, UIColor)] = [
  ("systemBackground", .systemBackground),
  ("secondarySystemBackground", .secondarySystemBackground),
  ("tertiarySystemBackground", .tertiarySystemBackground),
  ("systemGroupedBackground", .systemGroupedBackground),
  ("secondarySystemGroupedBackground", .secondarySystemGroupedBackground),
  ("tertiarySystemGroupedBackground", .tertiarySystemGroupedBackground),
  ("label", .label),
  ("secondaryLabel", .secondaryLabel),
  ("tertiaryLabel", .tertiaryLabel),
  ("quaternaryLabel", .quaternaryLabel),
  ("placeholderText", .placeholderText),
  ("separator", .separator),
  ("opaqueSeparator", .opaqueSeparator),
  ("link", .link),
  ("systemFill", .systemFill),
  ("secondarySystemFill", .secondarySystemFill),
  ("tertiarySystemFill", .tertiarySystemFill),
  ("quaternarySystemFill", .quaternarySystemFill),
  ("systemBlue", .systemBlue),
  ("systemRed", .systemRed),
  ("systemGreen", .systemGreen),
  ("systemYellow", .systemYellow),
  ("systemOrange", .systemOrange),
  ("systemPink", .systemPink),
  ("systemPurple", .systemPurple),
  ("systemIndigo", .systemIndigo),
  ("systemTeal", .systemTeal),
  ("systemCyan", .systemCyan),
  ("systemMint", .systemMint),
  ("systemBrown", .systemBrown),
  ("systemGray", .systemGray),
  ("systemGray2", .systemGray2),
  ("systemGray3", .systemGray3),
  ("systemGray4", .systemGray4),
  ("systemGray5", .systemGray5),
  ("systemGray6", .systemGray6),
]

func report() {
  for (name, color) in colors {
    for (style, s) in [("light", UIUserInterfaceStyle.light), ("dark", .dark)] {
      var (r, g, b, a): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
      color.resolvedColor(with: UITraitCollection(userInterfaceStyle: s)).getRed(&r, green: &g, blue: &b, alpha: &a)
      fputs("METRIC \(name).\(style) \(r) \(g) \(b) \(a)\n", stderr)
    }
  }
}

final class Controller: UIViewController {
  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    report()
  }

  override func viewDidAppear(_ animated: Bool) {
    super.viewDidAppear(animated)
    fputs("APPEARANCE \(traitCollection.userInterfaceStyle.rawValue)\n", stderr)
  }

  override func traitCollectionDidChange(_ previous: UITraitCollection?) {
    super.traitCollectionDidChange(previous)
    if previous?.userInterfaceStyle != traitCollection.userInterfaceStyle {
      fputs("APPEARANCE \(traitCollection.userInterfaceStyle.rawValue)\n", stderr)
    }
  }
}

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
  var window: UIWindow?

  func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
    let window = UIWindow(windowScene: scene as! UIWindowScene)
    window.rootViewController = Controller()
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
