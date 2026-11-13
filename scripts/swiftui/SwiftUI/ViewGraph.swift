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
  /// How the navigation bar shows the title (`navigationBarTitleDisplayMode`).
  var titleDisplayMode: NavigationBarItem.TitleDisplayMode?
}

/// A view SwiftUI shows itself (text, a list …), rather than through its body.
protocol PrimitiveView {
  @MainActor func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node]
}

/// A modifier SwiftUI applies itself: it resolves the view it modifies
/// (`content`), with the environment it sets, and the nodes it makes of them.
protocol PrimitiveModifier {
  @MainActor func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node]
}

/// A modifier that sets something on its content's nodes (a task, an identifier …).
protocol NodeModifier: PrimitiveModifier {
  @MainActor func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences)
}

extension NodeModifier {
  @MainActor func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let nodes = content()
    pommeApply(to: nodes, at: path, in: graph, preferences)
    return nodes
  }
}

/// A modifier that lays its content out (`fixedSize`, `frame`, `padding` …).
protocol LayoutModifier: PrimitiveModifier {
  @MainActor var pommeLayout: any Layout { get }
}

extension LayoutModifier {
  @MainActor func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    [LayoutNode(path: path, layout: pommeLayout, children: content())]
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
  /// The environment of the view being resolved.
  private(set) var environment = EnvironmentValues()
  /// The contents of the custom modifiers being resolved, innermost last.
  private var contents: [() -> [Node]] = []
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
  func update(_ root: AnyView) -> (nodes: [Node], preferences: Preferences) {
    visited = []
    tasks = [:]
    appearances = [:]
    environment = EnvironmentValues()
    let preferences = Preferences()
    followedNow = []
    let nodes = resolve(root, at: ViewPath(), preferences)
    storages = storages.filter { visited.contains($0.key) }
    followed = followed.filter { followedNow.contains($0.key) }
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
    attachDynamicProperties(&view, to: storage, environment: environment)
    return resolve(view.body, at: path.appending("body"), preferences)
  }

  /// Resolve with a changed environment (for the views `body` resolves).
  func withEnvironment<R>(_ change: (inout EnvironmentValues) -> Void, _ body: () -> R) -> R {
    let saved = environment
    change(&environment)
    defer { environment = saved }
    return body()
  }

  /// A custom modifier: its body, with its content standing for the view it modifies.
  func resolveModifier<M: ViewModifier>(_ modifier: M, content: @escaping () -> [Node], at path: ViewPath, _ preferences: Preferences) -> [Node] {
    var modifier = modifier
    let key = StorageKey(path: path, type: ObjectIdentifier(M.self))
    let storage = storages[key] ?? ViewStorage(changed: changed)
    storages[key] = storage
    visited.insert(key)
    attachDynamicProperties(&modifier, to: storage, environment: environment)
    // The content resolves in the environment around the modifier, as the modifier's own content would.
    let around = environment
    contents.append { [unowned self] in withEnvironment({ $0 = around }) { content() } }
    defer { contents.removeLast() }
    return resolve(modifier.body(content: _ViewModifier_Content<M>()), at: path.appending("body"), preferences)
  }

  /// The view a custom modifier modifies (its `content`), resolved outside that modifier.
  func resolveModifierContent() -> [Node] {
    guard let content = contents.popLast() else { return [] }
    defer { contents.append(content) }
    return content()
  }

  /// The geometry values followed with `onGeometryChange`, by the modifier's place.
  private var geometry: [ViewPath: Any] = [:]

  /// Whether the value followed at `path` differs from the last one (which it becomes).
  func geometryChanged<V: Equatable>(at path: ViewPath, to value: V) -> Bool {
    if let last = geometry[path] as? V, last == value { return false }
    geometry[path] = value
    return true
  }

  /// The values followed with `onChange(of:)`, by the modifier's place.
  private var followed: [ViewPath: Any] = [:]
  private var followedNow: Set<ViewPath> = []
  /// Actions to run once the update is done (`onChange`).
  private(set) var afterUpdate: [() -> Void] = []

  /// Whether the value followed at `path` changed since the last update (not
  /// the first time it's seen); it's remembered either way.
  func valueChanged<V: Equatable>(at path: ViewPath, to value: V) -> Bool {
    followedNow.insert(path)
    defer { followed[path] = value }
    guard let last = followed[path] as? V else { return false }
    return last != value
  }

  func runAfterUpdate(_ action: @escaping () -> Void) {
    afterUpdate.append(action)
  }

  /// The actions queued by the update, which they leave the graph without.
  func takeAfterUpdate() -> [() -> Void] {
    defer { afterUpdate = [] }
    return afterUpdate
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
    let content = { graph.resolve(self.content, at: path.appending("content"), preferences) }
    if let primitive = modifier as? any PrimitiveModifier {
      return primitive.pommeResolve(content: content, at: path, in: graph, preferences)
    }
    guard Modifier.Body.self != Never.self else {
      fatalError("SwiftUI in pomme doesn't implement the modifier \(Modifier.self) yet")
    }
    return graph.resolveModifier(modifier, content: content, at: path, preferences)
  }
}

extension _ViewModifier_Content: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    graph.resolveModifierContent()
  }
}

// ---- Modifiers of the environment and layout ----

extension _EnvironmentKeyWritingModifier: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    graph.withEnvironment({ pommeWrite(&$0) }, content)
  }
}

extension _FixedSizeLayout: LayoutModifier {
  var pommeLayout: any Layout { pommeFixedSize }
}

extension _CoordinateSpaceModifier: NodeModifier {
  /// It names a coordinate space for the geometry its content reads; it changes no layout.
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {}
}

extension AnyView: PrimitiveView {
  /// The view it erases, at a place named for its type (another type is another view).
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let view = pommeView
    return graph.resolve(view, at: path.appending(ObjectIdentifier(type(of: view))), preferences)
  }
}

// ---- Layouts ----

extension ViewGraph {
  /// A layout tree: its content's views, laid out by its root's layout.
  func resolveTree<Root, Content: View>(_ tree: _VariadicView.Tree<Root, Content>, at path: ViewPath, _ preferences: Preferences) -> [Node] {
    let children = resolve(tree.content, at: path.appending("content"), preferences)
    let layout: any Layout
    if let l = tree.root as? any Layout {
      layout = l
    } else if let root = tree.root as? any PommeLayoutRoot {
      layout = root.pommeLayout
    } else {
      fatalError("SwiftUI in pomme doesn't implement the container \(Root.self) yet")
    }
    return [LayoutNode(path: path, layout: layout, children: children)]
  }
}

extension _VariadicView.Tree: PrimitiveView where Content: View {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    graph.resolveTree(self, at: path, preferences)
  }
}

extension VStack: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    graph.resolveTree(pommeTree, at: path, preferences)
  }
}

extension HStack: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    graph.resolveTree(pommeTree, at: path, preferences)
  }
}

extension ZStack: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    graph.resolveTree(pommeTree, at: path, preferences)
  }
}

extension _ConditionalContent: PrimitiveView where TrueContent: View, FalseContent: View {
  /// The branch taken: each has its own place (state doesn't carry from one to the other).
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    switch storage {
    case .trueContent(let view): return graph.resolve(view, at: path.appending("true"), preferences)
    case .falseContent(let view): return graph.resolve(view, at: path.appending("false"), preferences)
    }
  }
}

extension _FrameLayout: LayoutModifier {
  var pommeLayout: any Layout { pommeFrame }
}

extension _FlexFrameLayout: LayoutModifier {
  var pommeLayout: any Layout { pommeFrame }
}

extension _TraitWritingModifier: NodeModifier {
  /// The trait on its content's views (a layout value is also found by its own key).
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    for node in nodes {
      node.layoutValues[ObjectIdentifier(Trait.self)] = value
      if let layoutTrait = Trait.self as? any PommeLayoutTrait.Type {
        node.layoutValues[layoutTrait.pommeLayoutKey] = value
      }
    }
  }
}

extension _ValueActionModifier: NodeModifier {
  /// `onChange(of:)`: the action runs after an update in which the value changed.
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    guard graph.valueChanged(at: path, to: value) else { return }
    let action = action
    let value = value
    graph.runAfterUpdate { action(value) }
  }
}
