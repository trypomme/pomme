// pomme's view graph: views resolve, through their bodies, to nodes (what's
// laid out and shown with UIKit). Each view's state lives in the graph at the
// view's place in the tree, so it outlives the view values, which are made
// anew on every update.

import UIKit

/// A view's place in the tree: its identity from one update to the next.
struct ViewPath: Hashable, CustomStringConvertible {
  private(set) var components: [AnyHashable] = []

  func appending(_ component: AnyHashable) -> ViewPath {
    var p = self
    p.components.append(component)
    return p
  }

  var description: String { components.map { "\($0)" }.joined(separator: "/") }
}

/// What views tell the views around them, as they resolve (a navigation title …).
@MainActor
final class Preferences {
  var navigationTitle: String?
}

/// A view SwiftUI shows itself (text, a list …), rather than through its body.
protocol PrimitiveView {
  @MainActor func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node]
}

/// A modifier SwiftUI applies itself to the nodes of the view it modifies.
protocol PrimitiveModifier {
  @MainActor func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences)
}

/// The view a window shows, ready to resolve.
@MainActor
struct RootView {
  let resolve: (ViewGraph, Preferences) -> [Node]

  init<V: View>(_ view: V) {
    resolve = { graph, preferences in graph.resolve(view, at: ViewPath(), preferences) }
  }
}

/// Work a view starts when it appears and that stops when it goes (`.task`).
struct TaskWork {
  let priority: TaskPriority
  let action: @isolated(any) () async -> Void
}

/// What a view does as it appears and disappears (`.onAppear`, `.onDisappear`).
struct AppearanceActions {
  var appear: (() -> Void)?
  var disappear: (() -> Void)?
}

@MainActor
final class ViewGraph {
  private struct StorageKey: Hashable {
    let path: ViewPath
    let type: ObjectIdentifier
  }

  private var storages: [StorageKey: ViewStorage] = [:]
  private var visited: Set<StorageKey> = []
  /// The tasks and appearance actions of the views in the tree, from the last update.
  private(set) var tasks: [ViewPath: TaskWork] = [:]
  private(set) var appearances: [ViewPath: AppearanceActions] = [:]
  /// Called when a view's state changes.
  private let changed: () -> Void

  init(changed: @escaping () -> Void) {
    self.changed = changed
  }

  /// Resolve the tree from its root view, keeping the state of the views still in it.
  func update(_ root: RootView) -> (nodes: [Node], preferences: Preferences) {
    visited = []
    tasks = [:]
    appearances = [:]
    let preferences = Preferences()
    let nodes = root.resolve(self, preferences)
    storages = storages.filter { visited.contains($0.key) }
    return (nodes, preferences)
  }

  func resolve<V: View>(_ view: V, at path: ViewPath, _ preferences: Preferences) -> [Node] {
    if let primitive = view as? any PrimitiveView {
      return primitive.pommeResolve(at: path, in: self, preferences)
    }
    guard V.Body.self != Never.self else {
      fatalError("SwiftUI in pomme doesn't implement the view \(V.self) yet")
    }
    var view = view
    let key = StorageKey(path: path, type: ObjectIdentifier(V.self))
    let storage = storages[key] ?? ViewStorage(changed: changed)
    storages[key] = storage
    visited.insert(key)
    attachDynamicProperties(&view, to: storage)
    return resolve(view.body, at: path.appending("body"), preferences)
  }

  func addTask(_ work: TaskWork, at path: ViewPath) {
    tasks[path] = work
  }

  func addAppearance(_ actions: AppearanceActions, at path: ViewPath) {
    appearances[path] = actions
  }
}

// ---- Primitive views ----

extension Text: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    [TextNode(path: path, string: pommeString)]
  }
}

extension EmptyView: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    []
  }
}

extension Optional: PrimitiveView where Wrapped: View {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    switch self {
    case .some(let view): return graph.resolve(view, at: path, preferences)
    case .none: return []
    }
  }
}

extension TupleView: PrimitiveView {
  /// Each element of the tuple, as its own view.
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    Mirror(reflecting: value).children.enumerated().flatMap { i, child -> [Node] in
      guard let view = child.value as? any View else { return [] }
      return graph.resolve(view, at: path.appending(i), preferences)
    }
  }
}

extension ForEach: PrimitiveView where Content: View {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    pommeElements.flatMap { element in
      graph.resolve(element.view, at: path.appending(AnyHashable(element.id)), preferences)
    }
  }
}

extension ModifiedContent: PrimitiveView where Content: View, Modifier: ViewModifier {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    guard let primitive = modifier as? any PrimitiveModifier else {
      fatalError("SwiftUI in pomme doesn't implement the modifier \(Modifier.self) yet")
    }
    let nodes = graph.resolve(content, at: path.appending("content"), preferences)
    primitive.pommeApply(to: nodes, at: path, in: graph, preferences)
    return nodes
  }
}
