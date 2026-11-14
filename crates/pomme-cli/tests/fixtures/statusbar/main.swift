// The status bar as an app sees and shapes it: a black screen asking for
// light content, then (after 2 s) for no status bar at all; and the battery
// through UIDevice, before and after monitoring it, and as it changes
// (`BATTERY what level state`). tests/status_bar.rs drives it.

import Darwin
import UIKit

func report(_ what: String) {
  let d = UIDevice.current
  fputs("BATTERY \(what) \(d.batteryLevel) \(d.batteryState.rawValue)\n", stderr)
}

final class Controller: UIViewController {
  var hidden = false
  override var preferredStatusBarStyle: UIStatusBarStyle { .lightContent }
  override var prefersStatusBarHidden: Bool { hidden }

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .black
    report("unmonitored")
    UIDevice.current.isBatteryMonitoringEnabled = true
    report("monitored")
    let center = NotificationCenter.default
    center.addObserver(forName: UIDevice.batteryStateDidChangeNotification, object: nil, queue: .main) { _ in report("state") }
    center.addObserver(forName: UIDevice.batteryLevelDidChangeNotification, object: nil, queue: .main) { _ in report("level") }
    DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [self] in
      hidden = true
      setNeedsStatusBarAppearanceUpdate()
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
