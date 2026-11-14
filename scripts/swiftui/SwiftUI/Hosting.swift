// Hosting: SwiftUI views in UIKit. A UIHostingController (a window's root,
// or one an app or library embeds) shows its root view in a hosting view,
// keeps it current as its state changes, and runs its views' tasks and
// appearance actions.

import UIKit

/// A SwiftUI tree shown in a UIKit view.
@MainActor
final class ViewHost {
  /// The view at the root; setting it updates the tree.
  var root: AnyView {
    didSet { setNeedsUpdate() }
  }
  /// The controller that shows it (a navigation stack's controller is its child).
  weak var controller: UIViewController?
  /// The hosting view, once the controller has loaded it.
  weak var view: UIView? {
    didSet { setNeedsUpdate() }
  }
  /// Whether the hosting view's intrinsic content size is the content's size
  /// (`sizingOptions`' `.intrinsicContentSize`).
  var sizesToContent = false {
    didSet { view?.invalidateIntrinsicContentSize() }
  }
  /// The safe areas its content keeps clear of (`safeAreaRegions`).
  var safeAreaRegions: SafeAreaRegions = .all {
    didSet { view?.setNeedsLayout() }
  }

  private var graph: ViewGraph!
  private let renderer = Renderer()
  private var nodes: [Node] = []
  private var updateScheduled = false
  private var running: [ViewPath: Task<Void, Never>] = [:]
  private var appeared: [ViewPath: AppearanceActions] = [:]
  /// A navigation stack at the root: shown by a navigation controller.
  private var navigation: (controller: UINavigationController, content: ContentController)?

  init(root: AnyView) {
    self.root = root
    graph = ViewGraph { [weak self] in self?.setNeedsUpdate() }
  }

  /// Update once the current work is done (state can change many times in a row).
  func setNeedsUpdate() {
    guard !updateScheduled, view != nil else { return }
    updateScheduled = true
    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      updateScheduled = false
      update()
    }
  }

  /// Resolve the tree and show it.
  func update() {
    guard let view else { return }
    // The environment's colour scheme is the appearance's.
    EnvironmentValues.pommeDefaults["colorScheme"] = view.traitCollection.userInterfaceStyle == .dark ? ColorScheme.dark : ColorScheme.light
    let (nodes, _) = graph.update(root)
    self.nodes = nodes
    if nodes.count == 1, let stack = nodes[0] as? NavigationStackNode {
      showNavigationStack(stack, in: view)
    }
    view.setNeedsLayout()
    if sizesToContent {
      view.invalidateIntrinsicContentSize()
    }
    runLifecycle()
    for action in graph.takeAfterUpdate() {
      action()
    }
  }

  /// Lay the tree out in the hosting view.
  func layout() {
    guard let view else { return }
    if navigation == nil {
      renderer.show(nodes, in: view, respectingSafeArea: safeAreaRegions.contains(.container))
    }
  }

  /// The content's size when offered `proposal` (a nil dimension: its ideal size).
  func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    nodes.reduce(CGSize.zero) { size, node in
      let s = node.sizeThatFits(proposal)
      return CGSize(width: max(size.width, s.width), height: max(size.height, s.height))
    }
  }

  private func showNavigationStack(_ stack: NavigationStackNode, in view: UIView) {
    if navigation == nil {
      let content = ContentController()
      let navigationController = UINavigationController(rootViewController: content)
      // SwiftUI's titles are large by default (automatic: the root's large).
      navigationController.navigationBar.prefersLargeTitles = true
      controller?.addChild(navigationController)
      navigationController.view.frame = view.bounds
      navigationController.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
      view.addSubview(navigationController.view)
      navigationController.didMove(toParent: controller)
      navigation = (navigationController, content)
    }
    navigation!.content.show(stack.root, title: stack.title, displayMode: stack.displayMode)
  }

  /// Start the tasks of views that have appeared, cancel those of views that
  /// have gone; the same for appearance actions.
  private func runLifecycle() {
    let tasks = graph.tasks
    for (path, task) in running where tasks[path] == nil {
      task.cancel()
      running[path] = nil
    }
    for (path, work) in tasks where running[path] == nil {
      running[path] = Task(priority: work.priority) { await work.action() }
    }
    let appearances = graph.appearances
    for (path, actions) in appeared where appearances[path] == nil {
      appeared[path] = nil
      actions.disappear?()
    }
    for (path, actions) in appearances where appeared[path] == nil {
      appeared[path] = actions
      actions.appear?()
    }
  }
}

/// A hosting controller's view: the tree is laid out in it.
final class HostingView: UIView {
  let host: ViewHost

  init(host: ViewHost) {
    self.host = host
    super.init(frame: .zero)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) isn't supported")
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    host.layout()
  }

  /// Light or dark changed: the tree resolves again (its colour scheme) and is drawn again.
  override func traitCollectionDidChange(_ previous: UITraitCollection?) {
    super.traitCollectionDidChange(previous)
    if previous?.userInterfaceStyle != traitCollection.userInterfaceStyle {
      host.setNeedsUpdate()
    }
  }

  override var intrinsicContentSize: CGSize {
    guard host.sizesToContent else { return super.intrinsicContentSize }
    return host.sizeThatFits(.unspecified)
  }

  override func sizeThatFits(_ size: CGSize) -> CGSize {
    host.sizeThatFits(ProposedViewSize(size))
  }
}

// pomme: in open class UIHostingController<Content> : UIKit.UIViewController where Content : SwiftUICore.View
extension UIHostingController {
  var pommeRootView: Content
  let pommeHost: ViewHost

  // pomme: replaces open class UIHostingController<Content> : UIKit.UIViewController where Content : SwiftUICore.View :: public init(rootView: Content)
  @MainActor @preconcurrency
  public init(rootView: Content) {
    pommeRootView = rootView
    pommeHost = ViewHost(root: AnyView(rootView))
    super.init(nibName: nil, bundle: nil)
    pommeHost.controller = self
  }

  // pomme: replaces open class UIHostingController<Content> : UIKit.UIViewController where Content : SwiftUICore.View :: override dynamic open func loadView()
  @MainActor @preconcurrency @objc
  override dynamic open func loadView() {
    let view = HostingView(host: pommeHost)
    view.backgroundColor = .systemBackground
    self.view = view
    pommeHost.view = view
    pommeHost.update()
  }

  // pomme: replaces open class UIHostingController<Content> : UIKit.UIViewController where Content : SwiftUICore.View :: public var rootView: Content
  @MainActor @preconcurrency
  public var rootView: Content {
    get { pommeRootView }
    set {
      pommeRootView = newValue
      pommeHost.root = AnyView(newValue)
    }
  }

  // pomme: replaces open class UIHostingController<Content> : UIKit.UIViewController where Content : SwiftUICore.View :: public var sizingOptions: SwiftUI.UIHostingControllerSizingOptions
  /// With `.intrinsicContentSize`, the view's intrinsic content size is the content's.
  @available(iOS 16.0, tvOS 16.0, *)
  @MainActor @preconcurrency
  public var sizingOptions: UIHostingControllerSizingOptions {
    get { pommeHost.sizesToContent ? .intrinsicContentSize : [] }
    set { pommeHost.sizesToContent = newValue.contains(.intrinsicContentSize) }
  }

  // pomme: replaces open class UIHostingController<Content> : UIKit.UIViewController where Content : SwiftUICore.View :: public func sizeThatFits(in size: CoreFoundation.CGSize) -> CoreFoundation.CGSize
  @MainActor @preconcurrency
  public func sizeThatFits(in size: CGSize) -> CGSize {
    pommeHost.sizeThatFits(ProposedViewSize(size))
  }
}

extension UIHostingController {
  // pomme: replaces extension SwiftUI.UIHostingController :: public var safeAreaRegions: SwiftUICore.SafeAreaRegions
  /// The safe areas the content keeps clear of; without `.container`, it goes under the bars too.
  @available(iOS 16.4, tvOS 16.4, *)
  @MainActor @preconcurrency
  public var safeAreaRegions: SafeAreaRegions {
    get { pommeHost.safeAreaRegions }
    set { pommeHost.safeAreaRegions = newValue }
  }
}

/// A navigation stack's content (its root view), under the navigation bar.
final class ContentController: UIViewController {
  private let renderer = Renderer()
  private var nodes: [Node] = []

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
  }

  func show(_ nodes: [Node], title: String?, displayMode: NavigationBarItem.TitleDisplayMode) {
    self.nodes = nodes
    if navigationItem.title != title {
      navigationItem.title = title
    }
    let mode = displayMode.pommeLargeTitleDisplayMode
    if navigationItem.largeTitleDisplayMode != mode {
      navigationItem.largeTitleDisplayMode = mode
    }
    view.setNeedsLayout()
  }

  /// The bar grew or shrank (a large title collapsing): lay the content out again.
  override func viewSafeAreaInsetsDidChange() {
    super.viewSafeAreaInsetsDidChange()
    view.setNeedsLayout()
  }

  override func viewDidLayoutSubviews() {
    super.viewDidLayoutSubviews()
    renderer.show(nodes, in: view)
  }
}
