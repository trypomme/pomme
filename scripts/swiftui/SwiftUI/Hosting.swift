// Hosting: a view controller that shows a SwiftUI view tree, updates it when
// its state changes, and runs its views' tasks and appearance actions.

import UIKit

/// The root of a window: resolves the tree, shows it, and keeps it current.
final class HostingController: UIViewController {
  private let rootView: RootView
  private var graph: ViewGraph!
  private let renderer = Renderer()
  private var nodes: [Node] = []
  private var updateScheduled = false
  private var running: [ViewPath: Task<Void, Never>] = [:]
  private var appeared: [ViewPath: AppearanceActions] = [:]
  /// A navigation stack at the root: shown by a navigation controller.
  private var navigation: (controller: UINavigationController, content: ContentController)?

  init(rootView: RootView) {
    self.rootView = rootView
    super.init(nibName: nil, bundle: nil)
    graph = ViewGraph { [weak self] in self?.setNeedsUpdate() }
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) isn't supported")
  }

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
    update()
  }

  override func viewDidLayoutSubviews() {
    super.viewDidLayoutSubviews()
    if navigation == nil {
      renderer.show(nodes, in: view)
    }
  }

  /// Update once the current work is done (state can change many times in a row).
  func setNeedsUpdate() {
    guard !updateScheduled else { return }
    updateScheduled = true
    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      updateScheduled = false
      update()
    }
  }

  private func update() {
    let (nodes, _) = graph.update(rootView)
    self.nodes = nodes
    if nodes.count == 1, let stack = nodes[0] as? NavigationStackNode {
      showNavigationStack(stack)
    } else {
      view.setNeedsLayout()
    }
    runLifecycle()
  }

  private func showNavigationStack(_ stack: NavigationStackNode) {
    if navigation == nil {
      let content = ContentController()
      let controller = UINavigationController(rootViewController: content)
      addChild(controller)
      controller.view.frame = view.bounds
      controller.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
      view.addSubview(controller.view)
      controller.didMove(toParent: self)
      navigation = (controller, content)
    }
    navigation!.content.show(stack.root, title: stack.title)
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

/// A navigation stack's content (its root view), under the navigation bar.
final class ContentController: UIViewController {
  private let renderer = Renderer()
  private var nodes: [Node] = []

  override func viewDidLoad() {
    super.viewDidLoad()
    view.backgroundColor = .systemBackground
  }

  func show(_ nodes: [Node], title: String?) {
    self.nodes = nodes
    if navigationItem.title != title {
      navigationItem.title = title
    }
    view.setNeedsLayout()
  }

  override func viewDidLayoutSubviews() {
    super.viewDidLayoutSubviews()
    renderer.show(nodes, in: view)
  }
}
