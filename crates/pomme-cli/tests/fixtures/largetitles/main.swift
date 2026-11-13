// A large title collapsing as its screen scrolls: a navigation controller
// that prefers large titles, its root's view a scroll view. The probe moves
// the content step by step and reports, after each, the bar's frame, the
// screen's safe-area top, the scroll view's adjusted top inset and its offset
// (`METRIC step.name x y width height`, in points). Recorded on the iOS
// Simulator (scripts/swiftui/metrics-simulator.sh largetitles → ios26.txt);
// tests/large_titles.rs holds pomme to it.

import Darwin
import UIKit

func report(_ name: String, _ r: CGRect) {
  // (stderr: unbuffered, so each line is out at once, terminal or not.)
  fputs("METRIC \(name) \(r.minX) \(r.minY) \(r.width) \(r.height)\n", stderr)
}

final class ScrollingController: UIViewController, UIScrollViewDelegate {
  let scroll = UIScrollView()

  override func loadView() {
    scroll.backgroundColor = .systemBackground
    scroll.contentSize = CGSize(width: 390, height: 2000)
    scroll.delegate = self
    view = scroll
  }

  override func viewDidLoad() {
    super.viewDidLoad()
    title = "Title"
  }

  /// Where things are with the content `by` points past its rest.
  func step(_ i: Int, by: CGFloat, then rest: [CGFloat]) {
    let top = scroll.adjustedContentInset.top
    scroll.contentOffset = CGPoint(x: 0, y: by - top)
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [self] in
      let bar = navigationController!.navigationBar
      report("s\(i).bar", bar.frame)
      report("s\(i).insets", CGRect(x: 0, y: view.safeAreaInsets.top, width: 0, height: scroll.adjustedContentInset.top))
      report("s\(i).offset", CGRect(x: 0, y: scroll.contentOffset.y, width: 0, height: by))
      if let next = rest.first {
        step(i + 1, by: next, then: Array(rest.dropFirst()))
      } else {
        inline()
      }
    }
  }

  /// The title not large: the bar's own height. Then large again (still
  /// collapsed, the content at the top), for dragging.
  func inline() {
    navigationItem.largeTitleDisplayMode = .never
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [self] in
      report("inline.bar", navigationController!.navigationBar.frame)
      navigationItem.largeTitleDisplayMode = .automatic
    }
  }

  /// Each change as it happens (not recorded: for watching a drag).
  func scrollViewDidScroll(_ scrollView: UIScrollView) {
    guard let bar = navigationController?.navigationBar else { return }
    fputs("SCROLL past \(scroll.contentOffset.y + scroll.adjustedContentInset.top) bar \(bar.frame.height) inset \(scroll.adjustedContentInset.top) dragging \(scroll.isDragging)\n", stderr)
  }

  override func viewDidAppear(_ animated: Bool) {
    super.viewDidAppear(animated)
    // At rest; partly, just and fully collapsed (UIKit collapses the title
    // only once the content's top passes the collapsed bar when moved, not
    // dragged); scrolled on; back to the top (still collapsed).
    step(0, by: 0, then: [10, 31, 62, 100, 300, 0])
  }
}

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
  var window: UIWindow?

  func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
    let window = UIWindow(windowScene: scene as! UIWindowScene)
    let nav = UINavigationController(rootViewController: ScrollingController())
    nav.navigationBar.prefersLargeTitles = true
    window.rootViewController = nav
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
