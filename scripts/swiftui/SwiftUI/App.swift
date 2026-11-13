// An app's life: `App.main()` starts UIKit with SwiftUI's own app delegate,
// which hands UIKit a scene delegate; that one shows the app's window group
// in a window.

import UIKit

/// The running app, as SwiftUI's delegates find it.
@MainActor
enum AppHost {
  /// The app's scene, made from its `body` when the window scene connects.
  static var makeScene: (() -> any Scene)?
  /// The app's own delegate (`@UIApplicationDelegateAdaptor`), made with the app.
  static var delegate: NSObject?
}

extension App {
  // pomme: replaces extension SwiftUI.App :: public static func main()
  @MainActor @preconcurrency
  public static func main() {
    let app = Self()
    AppHost.makeScene = { app.body }
    _ = UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(AppDelegate.self))
  }
}

/// SwiftUI's app delegate. The app's own (`@UIApplicationDelegateAdaptor`)
/// gets every message it implements; scenes connect with `AppSceneDelegate`.
final class AppDelegate: UIResponder, UIApplicationDelegate {
  private var appDelegate: NSObject? { AppHost.delegate }

  override func responds(to selector: Selector!) -> Bool {
    super.responds(to: selector) || appDelegate?.responds(to: selector) == true
  }

  override func forwardingTarget(for selector: Selector!) -> Any? {
    appDelegate?.responds(to: selector) == true ? appDelegate : nil
  }

  func application(_ application: UIApplication, willFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
    (appDelegate as? UIApplicationDelegate)?.application?(application, willFinishLaunchingWithOptions: launchOptions) ?? true
  }

  func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
    (appDelegate as? UIApplicationDelegate)?.application?(application, didFinishLaunchingWithOptions: launchOptions) ?? true
  }

  func application(_ application: UIApplication, configurationForConnecting session: UISceneSession, options: UIScene.ConnectionOptions) -> UISceneConfiguration {
    let configuration = UISceneConfiguration(name: nil, sessionRole: session.role)
    configuration.delegateClass = AppSceneDelegate.self
    return configuration
  }
}

/// SwiftUI's scene delegate: the app's window group in a window.
final class AppSceneDelegate: UIResponder, UIWindowSceneDelegate {
  var window: UIWindow?

  func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options connectionOptions: UIScene.ConnectionOptions) {
    guard let windowScene = scene as? UIWindowScene, let makeScene = AppHost.makeScene else { return }
    let window = UIWindow(windowScene: windowScene)
    window.rootViewController = HostingController(rootView: rootView(of: makeScene()))
    self.window = window
    window.makeKeyAndVisible()
  }
}

// ---- Scenes ----

/// A scene SwiftUI shows itself (a window group …), rather than through its body.
protocol PrimitiveScene {
  @MainActor func pommeRootView() -> RootView
}

/// The view a scene shows in its window.
@MainActor
func rootView<S: Scene>(of scene: S) -> RootView {
  if let primitive = scene as? any PrimitiveScene {
    return primitive.pommeRootView()
  }
  guard S.Body.self != Never.self else {
    fatalError("SwiftUI in pomme doesn't implement the scene \(S.self) yet")
  }
  return rootView(of: scene.body)
}

extension SceneBuilder {
  // pomme: replaces public struct SceneBuilder :: public static func buildBlock<Content>(_ content: Content) -> Content where Content : SwiftUI.Scene
  public static func buildBlock<Content>(_ content: Content) -> Content where Content: Scene {
    content
  }
}

// pomme: owns public struct WindowGroup<Content> : SwiftUI.Scene where Content : SwiftUICore.View
@available(iOS 14.0, macOS 11.0, tvOS 14.0, watchOS 7.0, *)
public struct WindowGroup<Content>: Scene where Content: View {
  var pommeID: String?
  var pommeTitle: Text?
  var pommeContent: () -> Content
}

extension WindowGroup {
  // pomme: replaces public struct WindowGroup<Content> : SwiftUI.Scene where Content : SwiftUICore.View :: public init(@SwiftUICore.ViewBuilder content: () -> Content)
  public init(@ViewBuilder content: () -> Content) {
    let view = content()
    self.init(id: nil, title: nil, lazyContent: { view })
  }

  // pomme: replaces extension SwiftUI.WindowGroup :: nonisolated internal init(id: Swift.String? = nil, title: SwiftUICore.Text? = nil, @SwiftUICore.ViewBuilder lazyContent: @escaping () -> Content)
  @usableFromInline
  nonisolated internal init(id: String? = nil, title: Text? = nil, @ViewBuilder lazyContent: @escaping () -> Content) {
    pommeID = id
    pommeTitle = title
    pommeContent = lazyContent
  }
}

extension WindowGroup: PrimitiveScene {
  func pommeRootView() -> RootView {
    RootView(pommeContent())
  }
}

// ---- The app's delegate ----

// pomme: owns public struct UIApplicationDelegateAdaptor<DelegateType> : SwiftUICore.DynamicProperty where DelegateType : ObjectiveC.NSObject, DelegateType : UIKit.UIApplicationDelegate
@available(iOS 14.0, tvOS 14.0, *)
@MainActor @preconcurrency @propertyWrapper
public struct UIApplicationDelegateAdaptor<DelegateType>: DynamicProperty where DelegateType: NSObject, DelegateType: UIApplicationDelegate {
  // pomme: replaces public struct UIApplicationDelegateAdaptor<DelegateType> : SwiftUICore.DynamicProperty where DelegateType : ObjectiveC.NSObject, DelegateType : UIKit.UIApplicationDelegate :: public init(_ delegateType: DelegateType.Type = DelegateType.self)
  /// The app's delegate, made the first time (an app has one).
  @MainActor @preconcurrency
  public init(_ delegateType: DelegateType.Type = DelegateType.self) {
    if AppHost.delegate == nil {
      AppHost.delegate = (delegateType as NSObject.Type).init()
    }
  }

  // pomme: replaces public struct UIApplicationDelegateAdaptor<DelegateType> : SwiftUICore.DynamicProperty where DelegateType : ObjectiveC.NSObject, DelegateType : UIKit.UIApplicationDelegate :: public var wrappedValue: DelegateType
  @MainActor @preconcurrency
  public var wrappedValue: DelegateType {
    AppHost.delegate as! DelegateType
  }
}
