// NavigationStack and navigation titles.

import UIKit

// pomme: owns public struct NavigationStack<Data, Root> : SwiftUICore.View where Root : SwiftUICore.View
@available(iOS 16.0, macOS 13.0, tvOS 16.0, watchOS 9.0, *)
@MainActor @preconcurrency
public struct NavigationStack<Data, Root>: View where Root: View {
  var pommeRoot: Root
}

extension NavigationStack {
  // pomme: replaces public struct NavigationStack<Data, Root> : SwiftUICore.View where Root : SwiftUICore.View :: public init(@SwiftUICore.ViewBuilder root: () -> Root) where Data == SwiftUI.NavigationPath
  @MainActor @preconcurrency
  public init(@ViewBuilder root: () -> Root) where Data == NavigationPath {
    pommeRoot = root()
  }
}

extension NavigationStack: PrimitiveView {
  /// The root view and the title it sets, which the navigation bar shows.
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let inner = Preferences()
    let root = graph.resolve(pommeRoot, at: path.appending("root"), inner)
    return [NavigationStackNode(path: path, root: root, title: inner.navigationTitle)]
  }
}

/// A navigation stack: shown by a navigation controller (see HostingController).
final class NavigationStackNode: Node {
  let root: [Node]
  let title: String?

  init(path: ViewPath, root: [Node], title: String?) {
    self.root = root
    self.title = title
    super.init(path: path)
  }
}

/// `.navigationTitle(…)`: the title of the navigation stack's screen.
struct NavigationTitleModifier: ViewModifier, NodeModifier {
  typealias Body = Never
  let title: () -> String

  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    preferences.navigationTitle = title()
  }
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func navigationTitle(_ titleKey: SwiftUICore.LocalizedStringKey) -> some SwiftUICore.View
  nonisolated public func navigationTitle(_ titleKey: LocalizedStringKey) -> some View {
    modifier(NavigationTitleModifier { titleKey.pommeLocalized() })
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func navigationTitle(_ title: SwiftUICore.Text) -> some SwiftUICore.View
  nonisolated public func navigationTitle(_ title: Text) -> some View {
    modifier(NavigationTitleModifier { title.pommeString })
  }
}
