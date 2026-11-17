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
  /// The alerts it's showing, by the view that asked (Alerts.swift).
  var shownAlerts: [ViewPath: UIAlertController] = [:]
  /// A pushed screen's toolbar items' views (Toolbar.swift).
  var toolbarHosts: [String: [ToolbarItemHost]] = [:]
  /// The sheets it's showing (Sheets.swift).
  var shownSheets: [ViewPath: SheetHostingController] = [:]
  /// A navigation stack at the root: shown by a navigation controller.
  private var navigation: (controller: UINavigationController, content: ContentController)?
  /// A tab view at the root: shown by a tab bar controller.
  private var tabs: TabsController?

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
    // The screen's scale, and that the scene is active.
    let scale = view.window?.screen.scale ?? UIScreen.main.scale
    EnvironmentValues.pommeDefaults["displayScale"] = scale
    EnvironmentValues.pommeDefaults["pixelLength"] = 1 / scale
    EnvironmentValues.pommeDefaults["scenePhase"] = ScenePhase.active
    // And its layout direction, the app's (its localization's).
    EnvironmentValues.pommeDefaults["layoutDirection"] = UIApplication.shared.userInterfaceLayoutDirection == .rightToLeft ? LayoutDirection.rightToLeft : LayoutDirection.leftToRight
    let (nodes, preferences) = graph.update(root)
    self.nodes = nodes
    // A screen a navigation link pushed: its title and toolbar, from its content.
    if let pushed = controller as? PushedHostingController {
      if pushed.navigationItem.title != preferences.navigationTitle {
        pushed.navigationItem.title = preferences.navigationTitle
      }
      let mode = (preferences.titleDisplayMode ?? .automatic).pommeLargeTitleDisplayMode
      if pushed.navigationItem.largeTitleDisplayMode != mode {
        pushed.navigationItem.largeTitleDisplayMode = mode
      }
      pushed.showToolbar(preferences.toolbar, hosts: &toolbarHosts)
    }
    if nodes.count == 1, let stack = nodes[0] as? NavigationStackNode {
      removeTabs()
      showNavigationStack(stack, in: view)
    } else if nodes.count == 1, let tabView = nodes[0] as? TabViewNode {
      removeNavigationStack()
      showTabs(tabView, in: view)
    } else {
      // (Content that was a navigation stack or tab view, and isn't now.)
      removeNavigationStack()
      removeTabs()
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

  /// Lay the tree out in the hosting view (shown by a navigation or tab bar
  /// controller instead, it has none of its own: what was there goes).
  func layout() {
    guard let view else { return }
    let own = navigation == nil && tabs == nil
    renderer.show(own ? nodes : [], in: view, respectingSafeArea: safeAreaRegions.contains(.container))
  }

  private func removeNavigationStack() {
    guard let (controller, _) = navigation else { return }
    controller.willMove(toParent: nil)
    controller.view.removeFromSuperview()
    controller.removeFromParent()
    navigation = nil
  }

  private func removeTabs() {
    guard let controller = tabs else { return }
    controller.willMove(toParent: nil)
    controller.view.removeFromSuperview()
    controller.removeFromParent()
    tabs = nil
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
    navigation!.content.show(stack: stack)
  }

  private func showTabs(_ node: TabViewNode, in view: UIView) {
    if tabs == nil {
      let controller = TabsController()
      self.controller?.addChild(controller)
      controller.view.frame = view.bounds
      controller.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
      view.addSubview(controller.view)
      controller.didMove(toParent: self.controller)
      tabs = controller
    }
    tabs!.show(node)
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
    applySheetSettings(graph.sheetSettings)
    syncSheets(graph.sheets)
    syncAlerts(graph.alerts)
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
  /// Its toolbar items' views, by placement (Toolbar.swift).
  private var toolbarHosts: [String: [ToolbarItemHost]] = [:]

  /// Show a navigation stack's screen: its views, title, toolbar and bar.
  func show(stack: NavigationStackNode) {
    show(stack.root, title: stack.title, displayMode: stack.displayMode)
    showToolbar(stack.toolbar, hosts: &toolbarHosts)
    if let nav = navigationController, nav.isNavigationBarHidden != stack.barHidden {
      nav.setNavigationBarHidden(stack.barHidden, animated: false)
    }
  }

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

/// A tab view's tabs: a tab bar controller with a page per tab, each its
/// tab's view, or a navigation controller for a tab that's a navigation stack.
final class TabsController: UITabBarController, UITabBarControllerDelegate {
  private var pages: [TabPageController] = []
  private var select: (@MainActor (Int) -> Void)?

  func show(_ node: TabViewNode) {
    delegate = self
    select = node.select
    // A page per tab, kept while the tabs stay as many; each shown (a
    // navigation stack's page becomes its navigation controller), then the
    // controllers the bar's tabs are.
    if pages.count != node.tabs.count {
      pages = node.tabs.map { _ in TabPageController() }
    }
    for (page, tab) in zip(pages, node.tabs) {
      page.show(tab)
    }
    let shown = pages.map(\.shown)
    // With a search tab, the tabs are UIKit's UITabs (the search one its
    // UISearchTab, which iOS 26 sets apart); otherwise the controllers.
    if #available(iOS 18.0, *), let search = node.tabs.firstIndex(where: { $0.tabItem?.search == true }) {
      if (tabs.map { $0.viewController }) != shown || !(tabs[search] is UISearchTab) {
        tabs = shown.enumerated().map { i, controller in
          if i == search { return UISearchTab { _ in controller } }
          return UITab(title: controller.tabBarItem.title ?? "", image: controller.tabBarItem.image, identifier: "pomme.tab.\(i)") { _ in controller }
        }
      }
      for (tab, controller) in zip(tabs, shown) {
        if let title = controller.tabBarItem.title { tab.title = title }
        if let image = controller.tabBarItem.image { tab.image = image }
        tab.badgeValue = controller.tabBarItem.badgeValue
      }
      if let i = node.selected, i < tabs.count, selectedTab !== tabs[i] {
        selectedTab = tabs[i]
      }
      return
    }
    if viewControllers ?? [] != shown {
      setViewControllers(shown, animated: false)
    }
    if let i = node.selected, i != selectedIndex {
      selectedIndex = i
    }
  }

  func tabBarController(_ tabBarController: UITabBarController, didSelect viewController: UIViewController) {
    if let i = viewControllers?.firstIndex(of: viewController) {
      select?(i)
    }
  }

  @available(iOS 18.0, *)
  func tabBarController(_ tabBarController: UITabBarController, didSelectTab selectedTab: UITab, previousTab: UITab?) {
    if let i = tabs.firstIndex(where: { $0 === selectedTab }) {
      select?(i)
    }
  }
}

/// One tab: its view, or its navigation stack under a navigation bar.
final class TabPageController: UIViewController {
  private let renderer = Renderer()
  private var nodes: [Node] = []
  private var navigation: (controller: UINavigationController, content: ContentController)?

  /// What the tab bar controller shows for this tab: this page, or its navigation controller.
  var shown: UIViewController { navigation?.controller ?? self }

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
  }

  func show(_ tab: Node) {
    if let stack = tab as? NavigationStackNode {
      if navigation == nil {
        let content = ContentController()
        let controller = UINavigationController(rootViewController: content)
        controller.navigationBar.prefersLargeTitles = true
        navigation = (controller, content)
      }
      navigation!.content.show(stack: stack)
    } else {
      nodes = [tab]
      if isViewLoaded { view.setNeedsLayout() }
    }
    let item = tab.tabItem
    shown.tabBarItem.title = item?.title
    shown.tabBarItem.image = item?.image
    shown.tabBarItem.badgeValue = item?.badge
  }

  override func viewSafeAreaInsetsDidChange() {
    super.viewSafeAreaInsetsDidChange()
    view.setNeedsLayout()
  }

  override func viewDidLayoutSubviews() {
    super.viewDidLayoutSubviews()
    if navigation == nil {
      renderer.show(nodes, in: view)
    }
  }
}
