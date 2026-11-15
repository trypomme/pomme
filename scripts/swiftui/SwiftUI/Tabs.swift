// Tabs: TabView, shown as iOS shows it, by a UITabBarController (pomme's
// UIKit draws the bar in either design: crates/pomme-hle/src/tabbar.rs).
// Each of a TabView's views is a tab: its tab item (`.tabItem { Label(…) }`)
// says its title and image, its tag (`.tag(…)`) is the selection's value
// for it. iOS 18's `Tab`s say the same themselves: a Tab is its content,
// tagged with its value and carrying its title and image (its
// `_identifiedView`), so both kinds of TabView resolve the same way. A tab
// can hold a NavigationStack, with its own navigation bar.

import UIKit

// ---- A tab's title and image ----

/// What a tab bar shows for a tab: its title and image.
struct TabItem {
  var title: String?
  var image: UIImage?
  /// Its badge (`.badge`): a count or a word.
  var badge: String?
  /// A search tab (`Tab(role: .search)`): UIKit's own, set apart under Liquid Glass.
  var search = false

  /// From a tab item's label as resolved: its first text and first image.
  init(from nodes: [Node]) {
    func visit(_ node: Node) {
      switch node {
      case let text as TextNode:
        if title == nil { title = text.string }
      case let image as ImageNode:
        if self.image == nil { self.image = image.image }
      case let label as LabelNode:
        if let icon = label.icon { visit(icon) }
        if let title = label.title { visit(title) }
      case let wrapper as WrapperNode:
        visit(wrapper.child)
      case let layout as LayoutNode:
        layout.children.forEach(visit)
      default:
        break
      }
    }
    nodes.forEach(visit)
  }

  init(title: String?, image: UIImage?, badge: String? = nil, search: Bool = false) {
    self.title = title
    self.image = image
    self.badge = badge
    self.search = search
  }
}

/// `.tabItem { … }`, and a `Tab`'s title, image and value: on its content's views.
struct TabItemModifier: ViewModifier, NodeModifier {
  typealias Body = Never
  /// A label view to resolve, or the title and image themselves.
  var label: AnyView?
  var item: TabItem?
  /// A Tab's value (its tag); `.tabItem` leaves the view's own tag.
  var tag: AnyHashable?

  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    var item = item ?? TabItem(title: nil, image: nil)
    if let label {
      let resolved = TabItem(from: graph.resolve(label, at: path.appending("tabItem"), Preferences()))
      item.title = item.title ?? resolved.title
      item.image = item.image ?? resolved.image
    }
    for node in nodes {
      // (A badge given before the tab item stays.)
      var merged = item
      merged.badge = item.badge ?? node.tabItem?.badge
      node.tabItem = merged
      if let tag { node.tag = tag }
    }
  }
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func tabItem<V>(@SwiftUICore.ViewBuilder _ label: () -> V) -> some SwiftUICore.View where V : SwiftUICore.View
  nonisolated public func tabItem<V>(@ViewBuilder _ label: () -> V) -> some View where V: View {
    modifier(TabItemModifier(label: AnyView(label())))
  }
}

/// `.badge(…)`: a tab's badge (nil or 0: none).
struct BadgeModifier: ViewModifier, NodeModifier {
  typealias Body = Never
  let badge: String?

  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    for node in nodes {
      var item = node.tabItem ?? TabItem(title: nil, image: nil)
      item.badge = badge
      node.tabItem = item
    }
  }

  /// A count's badge: none for 0, as iOS shows it.
  static func count(_ n: Int) -> String? { n == 0 ? nil : String(n) }
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func badge(_ count: Swift.Int) -> some SwiftUICore.View
  nonisolated public func badge(_ count: Int) -> some View {
    modifier(BadgeModifier(badge: BadgeModifier.count(count)))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func badge(_ label: SwiftUICore.Text?) -> some SwiftUICore.View
  nonisolated public func badge(_ label: Text?) -> some View {
    modifier(BadgeModifier(badge: label?.pommeString))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func badge(_ key: SwiftUICore.LocalizedStringKey?) -> some SwiftUICore.View
  nonisolated public func badge(_ key: LocalizedStringKey?) -> some View {
    modifier(BadgeModifier(badge: key?.pommeLocalized()))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func badge(_ resource: Foundation.LocalizedStringResource?) -> some SwiftUICore.View
  nonisolated public func badge(_ resource: LocalizedStringResource?) -> some View {
    modifier(BadgeModifier(badge: resource.map { String(localized: $0) }))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func badge<S>(_ label: S?) -> some SwiftUICore.View where S : Swift.StringProtocol
  nonisolated public func badge<S>(_ label: S?) -> some View where S: StringProtocol {
    modifier(BadgeModifier(badge: label.map { String($0) }))
  }
}

// ---- TabView ----

// pomme: owns public struct TabView<SelectionValue, Content> : SwiftUICore.View where SelectionValue : Swift.Hashable, Content : SwiftUICore.View
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 7.0, *)
@MainActor @preconcurrency
public struct TabView<SelectionValue, Content>: View where SelectionValue: Hashable, Content: View {
  var pommeSelection: Binding<SelectionValue>?
  var pommeContent: Content
}

extension TabView {
  // pomme: replaces public struct TabView<SelectionValue, Content> : SwiftUICore.View where SelectionValue : Swift.Hashable, Content : SwiftUICore.View :: public init(selection: SwiftUICore.Binding<SelectionValue>?, @SwiftUICore.ViewBuilder content: () -> Content)
  public init(selection: Binding<SelectionValue>?, @ViewBuilder content: () -> Content) {
    self.init(pommeSelection: selection, pommeContent: content())
  }

  // pomme: replaces public struct TabView<SelectionValue, Content> : SwiftUICore.View where SelectionValue : Swift.Hashable, Content : SwiftUICore.View :: public init<C>(selection: SwiftUICore.Binding<SelectionValue>, @SwiftUI.TabContentBuilder<SelectionValue> content: () -> C) where Content == SwiftUI.TabContentBuilder<SelectionValue>.Content<C>, C : SwiftUI.TabContent
  @available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
  public init<C>(selection: Binding<SelectionValue>, @TabContentBuilder<SelectionValue> content: () -> C) where Content == TabContentBuilder<SelectionValue>.Content<C>, C: TabContent {
    self.init(pommeSelection: selection, pommeContent: TabContentBuilder<SelectionValue>.Content(pommeContent: content()))
  }
}

extension TabView where SelectionValue == Int {
  // pomme: replaces extension SwiftUI.TabView where SelectionValue == Swift.Int :: nonisolated public init(@SwiftUICore.ViewBuilder content: () -> Content)
  nonisolated public init(@ViewBuilder content: () -> Content) {
    self.init(pommeSelection: nil, pommeContent: content())
  }
}

extension TabView {
  // pomme: replaces extension SwiftUI.TabView :: public init<C>(@SwiftUI.TabContentBuilder<Swift.Never> content: () -> C) where SelectionValue == Swift.Never, Content == SwiftUI.TabContentBuilder<Swift.Never>.Content<C>, C : SwiftUI.TabContent
  @available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
  public init<C>(@TabContentBuilder<Never> content: () -> C) where SelectionValue == Never, Content == TabContentBuilder<Never>.Content<C>, C: TabContent {
    self.init(pommeSelection: nil, pommeContent: TabContentBuilder<Never>.Content(pommeContent: content()))
  }
}

extension TabView: PrimitiveView {
  /// Each of its views a tab; the selection's tab selected (without a
  /// selection, the tab bar keeps its own).
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    switch graph.environment.pommeTabViewStyle {
    case nil, is DefaultTabViewStyle, is TabBarOnlyTabViewStyle, is SidebarAdaptableTabViewStyle: break
    case let style?:
      fatalError("SwiftUI in pomme doesn't implement the tab view style \(type(of: style)) yet")
    }
    let tabs = graph.resolve(pommeContent, at: path.appending("tabs"), Preferences())
    let binding = pommeSelection
    let selected = binding.flatMap { pommeUnwrapped($0.wrappedValue) }
    return [TabViewNode(path: path, tabs: tabs, selected: selected.flatMap { s in tabs.firstIndex { $0.tag == s } }) { index in
      // A tab chosen in the bar: the selection becomes its tag.
      guard let binding, let tag = tabs[index].tag, let value = tag.base as? SelectionValue else { return }
      binding.wrappedValue = value
    }]
  }
}

/// A tab view: its tabs (each a node with its tab item and tag), which is
/// selected, and what choosing one does. Shown by a tab bar controller (see ViewHost).
final class TabViewNode: Node {
  let tabs: [Node]
  let selected: Int?
  let select: @MainActor (Int) -> Void

  init(path: ViewPath, tabs: [Node], selected: Int?, select: @escaping @MainActor (Int) -> Void) {
    self.tabs = tabs
    self.selected = selected
    self.select = select
    super.init(path: path)
  }
}

// ---- iOS 18's Tab ----

// pomme: owns public struct Tab<Value, Content, Label>
@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
public struct Tab<Value, Content, Label> {
  var pommeTitle: String?
  var pommeImage: UIImage?
  /// Its value (none for a tab that isn't selected by value).
  var pommeValue: Value?
  var pommeContent: Content
  var pommeLabel: Label?
  /// Its role (search: UIKit's search tab, which iOS 26 sets apart).
  var pommeRole: TabRole? = nil
}

// pomme: owns public struct TabRole : Swift.Hashable, Swift.Sendable
@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
public struct TabRole: Hashable, Sendable {
  let pommeName: String

  // pomme: replaces public struct TabRole : Swift.Hashable, Swift.Sendable :: public static var search: SwiftUI.TabRole
  public static var search: TabRole { TabRole(pommeName: "search") }

  // pomme: replaces public struct TabRole : Swift.Hashable, Swift.Sendable :: public static func == (a: SwiftUI.TabRole, b: SwiftUI.TabRole) -> Swift.Bool
  public static func == (a: TabRole, b: TabRole) -> Bool { a.pommeName == b.pommeName }

  // pomme: replaces public struct TabRole : Swift.Hashable, Swift.Sendable :: public func hash(into hasher: inout Swift.Hasher)
  public func hash(into hasher: inout Hasher) { hasher.combine(pommeName) }

  // pomme: replaces public struct TabRole : Swift.Hashable, Swift.Sendable :: public var hashValue: Swift.Int
  public var hashValue: Int { pommeName.hashValue }
}

@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension Tab {
  /// A named image (an asset's).
  static func pommeImage(named name: String) -> UIImage? { UIImage(named: name) }
  static func pommeImage(systemName name: String) -> UIImage? { UIImage(systemName: name) }
}

@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension Tab where Value: Hashable, Content: View, Label: View {
  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<S>(_ title: S, image: Swift.String, value: Value, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel, S : Swift.StringProtocol
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, image: String, value: Value, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel, S: StringProtocol {
    self.init(pommeTitle: String(title), pommeImage: Self.pommeImage(named: image), pommeValue: value, pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<S, T>(_ title: S, image: Swift.String, value: T, @SwiftUICore.ViewBuilder content: () -> Content) where Value == T?, Label == SwiftUI.DefaultTabLabel, S : Swift.StringProtocol, T : Swift.Hashable
  @_disfavoredOverload
  nonisolated public init<S, T>(_ title: S, image: String, value: T, @ViewBuilder content: () -> Content) where Value == T?, Label == DefaultTabLabel, S: StringProtocol, T: Hashable {
    self.init(pommeTitle: String(title), pommeImage: Self.pommeImage(named: image), pommeValue: .some(value), pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, image: Swift.String, value: Value, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  nonisolated public init(_ titleKey: LocalizedStringKey, image: String, value: Value, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeImage: Self.pommeImage(named: image), pommeValue: value, pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<T>(_ titleKey: SwiftUICore.LocalizedStringKey, image: Swift.String, value: T, @SwiftUICore.ViewBuilder content: () -> Content) where Value == T?, Label == SwiftUI.DefaultTabLabel, T : Swift.Hashable
  nonisolated public init<T>(_ titleKey: LocalizedStringKey, image: String, value: T, @ViewBuilder content: () -> Content) where Value == T?, Label == DefaultTabLabel, T: Hashable {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeImage: Self.pommeImage(named: image), pommeValue: .some(value), pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<S>(_ title: S, systemImage: Swift.String, value: Value, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel, S : Swift.StringProtocol
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, systemImage: String, value: Value, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel, S: StringProtocol {
    self.init(pommeTitle: String(title), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: value, pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<S, T>(_ title: S, systemImage: Swift.String, value: T, @SwiftUICore.ViewBuilder content: () -> Content) where Value == T?, Label == SwiftUI.DefaultTabLabel, S : Swift.StringProtocol, T : Swift.Hashable
  @_disfavoredOverload
  nonisolated public init<S, T>(_ title: S, systemImage: String, value: T, @ViewBuilder content: () -> Content) where Value == T?, Label == DefaultTabLabel, S: StringProtocol, T: Hashable {
    self.init(pommeTitle: String(title), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: .some(value), pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, systemImage: Swift.String, value: Value, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  nonisolated public init(_ titleKey: LocalizedStringKey, systemImage: String, value: Value, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: value, pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<T>(_ titleKey: SwiftUICore.LocalizedStringKey, systemImage: Swift.String, value: T, @SwiftUICore.ViewBuilder content: () -> Content) where Value == T?, Label == SwiftUI.DefaultTabLabel, T : Swift.Hashable
  nonisolated public init<T>(_ titleKey: LocalizedStringKey, systemImage: String, value: T, @ViewBuilder content: () -> Content) where Value == T?, Label == DefaultTabLabel, T: Hashable {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: .some(value), pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init(value: Value, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUICore.EmptyView
  nonisolated public init(value: Value, @ViewBuilder content: () -> Content) where Label == EmptyView {
    self.init(pommeTitle: nil, pommeImage: nil, pommeValue: value, pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<V>(value: V, @SwiftUICore.ViewBuilder content: () -> Content) where Value == V?, Label == SwiftUICore.EmptyView, V : Swift.Hashable
  nonisolated public init<V>(value: V, @ViewBuilder content: () -> Content) where Value == V?, Label == EmptyView, V: Hashable {
    self.init(pommeTitle: nil, pommeImage: nil, pommeValue: .some(value), pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init(value: Value, @SwiftUICore.ViewBuilder content: () -> Content, @SwiftUICore.ViewBuilder label: () -> Label)
  nonisolated public init(value: Value, @ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
    self.init(pommeTitle: nil, pommeImage: nil, pommeValue: value, pommeContent: content(), pommeLabel: label())
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<V>(value: V, @SwiftUICore.ViewBuilder content: () -> Content, @SwiftUICore.ViewBuilder label: () -> Label) where Value == V?, V : Swift.Hashable
  nonisolated public init<V>(value: V, @ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) where Value == V?, V: Hashable {
    self.init(pommeTitle: nil, pommeImage: nil, pommeValue: .some(value), pommeContent: content(), pommeLabel: label())
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: public var _identifiedView: some SwiftUICore.View
  /// Its content, tagged with its value, carrying its title and image.
  @MainActor @preconcurrency public var _identifiedView: some View {
    return pommeContent.modifier(TabItemModifier(
      label: pommeLabel.map { AnyView($0) },
      item: TabItem(title: pommeTitle, image: pommeImage, search: pommeRole == .search),
      tag: pommeValue.flatMap { pommeUnwrapped($0) }))
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: public var body: SwiftUI.Tab<Value, Content, Label>
  @MainActor @preconcurrency public var body: Tab<Value, Content, Label> { return self }
}

@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension Tab where Value == Never, Content: View, Label: View {
  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init<S>(_ title: S, image: Swift.String, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel, S : Swift.StringProtocol
  @_disfavoredOverload
  public init<S>(_ title: S, image: String, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel, S: StringProtocol {
    self.init(pommeTitle: String(title), pommeImage: Self.pommeImage(named: image), pommeValue: nil, pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init(_ titleKey: SwiftUICore.LocalizedStringKey, image: Swift.String, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  public init(_ titleKey: LocalizedStringKey, image: String, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeImage: Self.pommeImage(named: image), pommeValue: nil, pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init<S>(_ title: S, systemImage: Swift.String, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel, S : Swift.StringProtocol
  @_disfavoredOverload
  public init<S>(_ title: S, systemImage: String, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel, S: StringProtocol {
    self.init(pommeTitle: String(title), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: nil, pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init(_ titleKey: SwiftUICore.LocalizedStringKey, systemImage: Swift.String, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  public init(_ titleKey: LocalizedStringKey, systemImage: String, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: nil, pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init(@SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUICore.EmptyView
  public init(@ViewBuilder content: () -> Content) where Label == EmptyView {
    self.init(pommeTitle: nil, pommeImage: nil, pommeValue: nil, pommeContent: content(), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init(@SwiftUICore.ViewBuilder content: () -> Content, @SwiftUICore.ViewBuilder label: () -> Label)
  public init(@ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
    self.init(pommeTitle: nil, pommeImage: nil, pommeValue: nil, pommeContent: content(), pommeLabel: label())
  }
}

// (Generated from the SDK's interface: each initializer with a role or a
// LocalizedStringResource title, into the same parts as the rest.)
@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension Tab where Value: Hashable, Content: View, Label: View {
  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<S>(_ title: S, image: Swift.String, value: Value, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel, S : Swift.StringProtocol
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, image: String, value: Value, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel, S : StringProtocol {
    self.init(pommeTitle: String(title), pommeImage: Self.pommeImage(named: image), pommeValue: value, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<S, T>(_ title: S, image: Swift.String, value: T, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Value == T?, Label == SwiftUI.DefaultTabLabel, S : Swift.StringProtocol, T : Swift.Hashable
  @_disfavoredOverload
  nonisolated public init<S, T>(_ title: S, image: String, value: T, role: TabRole?, @ViewBuilder content: () -> Content) where Value == T?, Label == DefaultTabLabel, S : StringProtocol, T : Hashable {
    self.init(pommeTitle: String(title), pommeImage: Self.pommeImage(named: image), pommeValue: .some(value), pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init(_ titleResource: Foundation.LocalizedStringResource, image: Swift.String, value: Value, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  @_disfavoredOverload
  nonisolated public init(_ titleResource: LocalizedStringResource, image: String, value: Value, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: String(localized: titleResource), pommeImage: Self.pommeImage(named: image), pommeValue: value, pommeContent: content(), pommeLabel: nil, pommeRole: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, image: Swift.String, value: Value, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  nonisolated public init(_ titleKey: LocalizedStringKey, image: String, value: Value, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeImage: Self.pommeImage(named: image), pommeValue: value, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init(_ titleResource: Foundation.LocalizedStringResource, image: Swift.String, value: Value, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  @_disfavoredOverload
  nonisolated public init(_ titleResource: LocalizedStringResource, image: String, value: Value, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: String(localized: titleResource), pommeImage: Self.pommeImage(named: image), pommeValue: value, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<T>(_ titleResource: Foundation.LocalizedStringResource, image: Swift.String, value: T, @SwiftUICore.ViewBuilder content: () -> Content) where Value == T?, Label == SwiftUI.DefaultTabLabel, T : Swift.Hashable
  @_disfavoredOverload
  nonisolated public init<T>(_ titleResource: LocalizedStringResource, image: String, value: T, @ViewBuilder content: () -> Content) where Value == T?, Label == DefaultTabLabel, T : Hashable {
    self.init(pommeTitle: String(localized: titleResource), pommeImage: Self.pommeImage(named: image), pommeValue: .some(value), pommeContent: content(), pommeLabel: nil, pommeRole: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<T>(_ titleKey: SwiftUICore.LocalizedStringKey, image: Swift.String, value: T, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Value == T?, Label == SwiftUI.DefaultTabLabel, T : Swift.Hashable
  nonisolated public init<T>(_ titleKey: LocalizedStringKey, image: String, value: T, role: TabRole?, @ViewBuilder content: () -> Content) where Value == T?, Label == DefaultTabLabel, T : Hashable {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeImage: Self.pommeImage(named: image), pommeValue: .some(value), pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<T>(_ titleResource: Foundation.LocalizedStringResource, image: Swift.String, value: T, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Value == T?, Label == SwiftUI.DefaultTabLabel, T : Swift.Hashable
  @_disfavoredOverload
  nonisolated public init<T>(_ titleResource: LocalizedStringResource, image: String, value: T, role: TabRole?, @ViewBuilder content: () -> Content) where Value == T?, Label == DefaultTabLabel, T : Hashable {
    self.init(pommeTitle: String(localized: titleResource), pommeImage: Self.pommeImage(named: image), pommeValue: .some(value), pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<S>(_ title: S, systemImage: Swift.String, value: Value, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel, S : Swift.StringProtocol
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, systemImage: String, value: Value, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel, S : StringProtocol {
    self.init(pommeTitle: String(title), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: value, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<S, T>(_ title: S, systemImage: Swift.String, value: T, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Value == T?, Label == SwiftUI.DefaultTabLabel, S : Swift.StringProtocol, T : Swift.Hashable
  @_disfavoredOverload
  nonisolated public init<S, T>(_ title: S, systemImage: String, value: T, role: TabRole?, @ViewBuilder content: () -> Content) where Value == T?, Label == DefaultTabLabel, S : StringProtocol, T : Hashable {
    self.init(pommeTitle: String(title), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: .some(value), pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init(_ titleResource: Foundation.LocalizedStringResource, systemImage: Swift.String, value: Value, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  @_disfavoredOverload
  nonisolated public init(_ titleResource: LocalizedStringResource, systemImage: String, value: Value, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: String(localized: titleResource), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: value, pommeContent: content(), pommeLabel: nil, pommeRole: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, systemImage: Swift.String, value: Value, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  nonisolated public init(_ titleKey: LocalizedStringKey, systemImage: String, value: Value, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: value, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init(_ titleResource: Foundation.LocalizedStringResource, systemImage: Swift.String, value: Value, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  @_disfavoredOverload
  nonisolated public init(_ titleResource: LocalizedStringResource, systemImage: String, value: Value, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: String(localized: titleResource), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: value, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<T>(_ titleResource: Foundation.LocalizedStringResource, systemImage: Swift.String, value: T, @SwiftUICore.ViewBuilder content: () -> Content) where Value == T?, Label == SwiftUI.DefaultTabLabel, T : Swift.Hashable
  @_disfavoredOverload
  nonisolated public init<T>(_ titleResource: LocalizedStringResource, systemImage: String, value: T, @ViewBuilder content: () -> Content) where Value == T?, Label == DefaultTabLabel, T : Hashable {
    self.init(pommeTitle: String(localized: titleResource), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: .some(value), pommeContent: content(), pommeLabel: nil, pommeRole: nil)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<T>(_ titleKey: SwiftUICore.LocalizedStringKey, systemImage: Swift.String, value: T, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Value == T?, Label == SwiftUI.DefaultTabLabel, T : Swift.Hashable
  nonisolated public init<T>(_ titleKey: LocalizedStringKey, systemImage: String, value: T, role: TabRole?, @ViewBuilder content: () -> Content) where Value == T?, Label == DefaultTabLabel, T : Hashable {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: .some(value), pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<T>(_ titleResource: Foundation.LocalizedStringResource, systemImage: Swift.String, value: T, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Value == T?, Label == SwiftUI.DefaultTabLabel, T : Swift.Hashable
  @_disfavoredOverload
  nonisolated public init<T>(_ titleResource: LocalizedStringResource, systemImage: String, value: T, role: TabRole?, @ViewBuilder content: () -> Content) where Value == T?, Label == DefaultTabLabel, T : Hashable {
    self.init(pommeTitle: String(localized: titleResource), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: .some(value), pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init(value: Value, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  nonisolated public init(value: Value, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: nil, pommeImage: nil, pommeValue: value, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<V>(value: V, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Value == V?, Label == SwiftUI.DefaultTabLabel, V : Swift.Hashable
  nonisolated public init<V>(value: V, role: TabRole?, @ViewBuilder content: () -> Content) where Value == V?, Label == DefaultTabLabel, V : Hashable {
    self.init(pommeTitle: nil, pommeImage: nil, pommeValue: .some(value), pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init(value: Value, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content, @SwiftUICore.ViewBuilder label: () -> Label)
  nonisolated public init(value: Value, role: TabRole?, @ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
    self.init(pommeTitle: nil, pommeImage: nil, pommeValue: value, pommeContent: content(), pommeLabel: label(), pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab : SwiftUI.TabContent where Value : Swift.Hashable, Content : SwiftUICore.View, Label : SwiftUICore.View :: nonisolated public init<V>(value: V, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content, @SwiftUICore.ViewBuilder label: () -> Label) where Value == V?, V : Swift.Hashable
  nonisolated public init<V>(value: V, role: TabRole?, @ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) where Value == V?, V : Hashable {
    self.init(pommeTitle: nil, pommeImage: nil, pommeValue: .some(value), pommeContent: content(), pommeLabel: label(), pommeRole: role)
  }
}

@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension Tab where Value == Never, Content: View, Label: View {
  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init<S>(_ title: S, image: Swift.String, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel, S : Swift.StringProtocol
  @_disfavoredOverload
  public init<S>(_ title: S, image: String, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel, S : StringProtocol {
    self.init(pommeTitle: String(title), pommeImage: Self.pommeImage(named: image), pommeValue: nil, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init(_ titleResource: Foundation.LocalizedStringResource, image: Swift.String, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  @_disfavoredOverload
  public init(_ titleResource: LocalizedStringResource, image: String, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: String(localized: titleResource), pommeImage: Self.pommeImage(named: image), pommeValue: nil, pommeContent: content(), pommeLabel: nil, pommeRole: nil)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init(_ titleKey: SwiftUICore.LocalizedStringKey, image: Swift.String, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  public init(_ titleKey: LocalizedStringKey, image: String, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeImage: Self.pommeImage(named: image), pommeValue: nil, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init(_ titleResource: Foundation.LocalizedStringResource, image: Swift.String, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  @_disfavoredOverload
  public init(_ titleResource: LocalizedStringResource, image: String, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: String(localized: titleResource), pommeImage: Self.pommeImage(named: image), pommeValue: nil, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init<S>(_ title: S, systemImage: Swift.String, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel, S : Swift.StringProtocol
  @_disfavoredOverload
  public init<S>(_ title: S, systemImage: String, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel, S : StringProtocol {
    self.init(pommeTitle: String(title), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: nil, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init(_ titleResource: Foundation.LocalizedStringResource, systemImage: Swift.String, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  @_disfavoredOverload
  public init(_ titleResource: LocalizedStringResource, systemImage: String, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: String(localized: titleResource), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: nil, pommeContent: content(), pommeLabel: nil, pommeRole: nil)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init(_ titleKey: SwiftUICore.LocalizedStringKey, systemImage: Swift.String, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  public init(_ titleKey: LocalizedStringKey, systemImage: String, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: nil, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init(_ titleResource: Foundation.LocalizedStringResource, systemImage: Swift.String, role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  @_disfavoredOverload
  public init(_ titleResource: LocalizedStringResource, systemImage: String, role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: String(localized: titleResource), pommeImage: Self.pommeImage(systemName: systemImage), pommeValue: nil, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init(role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content) where Label == SwiftUI.DefaultTabLabel
  public init(role: TabRole?, @ViewBuilder content: () -> Content) where Label == DefaultTabLabel {
    self.init(pommeTitle: nil, pommeImage: nil, pommeValue: nil, pommeContent: content(), pommeLabel: nil, pommeRole: role)
  }

  // pomme: replaces extension SwiftUI.Tab where Value == Swift.Never, Content : SwiftUICore.View, Label : SwiftUICore.View :: public init(role: SwiftUI.TabRole?, @SwiftUICore.ViewBuilder content: () -> Content, @SwiftUICore.ViewBuilder label: () -> Label)
  public init(role: TabRole?, @ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
    self.init(pommeTitle: nil, pommeImage: nil, pommeValue: nil, pommeContent: content(), pommeLabel: label(), pommeRole: role)
  }
}

// ---- Badges on tabs ----

/// A tab with a badge (`Tab(…).badge(3)`).
@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
struct PommeBadgedTab<C: TabContent>: TabContent {
  typealias TabValue = C.TabValue
  let content: C
  let badge: String?

  var _identifiedView: some View {
    return content._identifiedView.modifier(BadgeModifier(badge: badge))
  }

  var body: PommeBadgedTab<C> {
    return self
  }
}

@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension TabContent {
  // pomme: replaces extension SwiftUI.TabContent :: nonisolated public func badge(_ count: Swift.Int) -> some SwiftUI.TabContent<Self.TabValue>
  nonisolated public func badge(_ count: Int) -> some TabContent<TabValue> {
    return PommeBadgedTab(content: self, badge: BadgeModifier.count(count))
  }

  // pomme: replaces extension SwiftUI.TabContent :: nonisolated public func badge(_ label: SwiftUICore.Text?) -> some SwiftUI.TabContent<Self.TabValue>
  nonisolated public func badge(_ label: Text?) -> some TabContent<TabValue> {
    return PommeBadgedTab(content: self, badge: label?.pommeString)
  }

  // pomme: replaces extension SwiftUI.TabContent :: nonisolated public func badge(_ key: SwiftUICore.LocalizedStringKey) -> some SwiftUI.TabContent<Self.TabValue>
  nonisolated public func badge(_ key: LocalizedStringKey) -> some TabContent<TabValue> {
    return PommeBadgedTab(content: self, badge: key.pommeLocalized())
  }

  // pomme: replaces extension SwiftUI.TabContent :: nonisolated public func badge(_ resource: Foundation.LocalizedStringResource) -> some SwiftUI.TabContent<Self.TabValue>
  nonisolated public func badge(_ resource: LocalizedStringResource) -> some TabContent<TabValue> {
    return PommeBadgedTab(content: self, badge: String(localized: resource))
  }

  // pomme: replaces extension SwiftUI.TabContent :: nonisolated public func badge<S>(_ label: S) -> some SwiftUI.TabContent<Self.TabValue> where S : Swift.StringProtocol
  nonisolated public func badge<S>(_ label: S) -> some TabContent<TabValue> where S: StringProtocol {
    return PommeBadgedTab(content: self, badge: String(label))
  }
}

// ---- ForEach and Group of tabs ----

// pomme: in extension SwiftUICore.ForEach : SwiftUI.TabContent where Content : SwiftUI.TabContent
extension ForEach where Content: TabContent {
  // pomme: replaces extension SwiftUICore.ForEach : SwiftUI.TabContent where Content : SwiftUI.TabContent :: nonisolated public init<V>(_ data: Data, id: Swift.KeyPath<Data.Element, ID>, @SwiftUI.TabContentBuilder<V> content: @escaping (Data.Element) -> Content) where V == Content.TabValue
  nonisolated public init<V>(_ data: Data, id: KeyPath<Data.Element, ID>, @TabContentBuilder<V> content: @escaping (Data.Element) -> Content) where V == Content.TabValue {
    self.init(pommeData: data, id: { $0[keyPath: id] }, content: content)
  }

  // pomme: replaces extension SwiftUICore.ForEach : SwiftUI.TabContent where Content : SwiftUI.TabContent :: internal init<V>(_ data: Data, id: Swift.KeyPath<Data.Element, ID>, @SwiftUI.TabContentBuilder<V?> liftedContent content: @escaping (Data.Element) -> Content) where V : Swift.Hashable, Content.TabValue == V?
  @usableFromInline
  internal init<V>(_ data: Data, id: KeyPath<Data.Element, ID>, @TabContentBuilder<V?> liftedContent content: @escaping (Data.Element) -> Content) where V: Hashable, Content.TabValue == V? {
    self.init(pommeData: data, id: { $0[keyPath: id] }, content: content)
  }

  // pomme: replaces extension SwiftUICore.ForEach : SwiftUI.TabContent where Content : SwiftUI.TabContent :: nonisolated public init<V>(_ data: Data, id: Swift.KeyPath<Data.Element, ID>, @SwiftUI.TabContentBuilder<V?> content: @escaping (Data.Element) -> Content) where V : Swift.Hashable, Content.TabValue == V?
  @_disfavoredOverload
  nonisolated public init<V>(_ data: Data, id: KeyPath<Data.Element, ID>, @TabContentBuilder<V?> content: @escaping (Data.Element) -> Content) where V: Hashable, Content.TabValue == V? {
    self.init(pommeData: data, id: { $0[keyPath: id] }, content: content)
  }

  // pomme: replaces extension SwiftUICore.ForEach : SwiftUI.TabContent where Content : SwiftUI.TabContent :: public var _identifiedView: SwiftUICore.ForEach<Data, ID, Content._IdentifiedView>
  /// Each element's tab, as a view.
  @MainActor @preconcurrency public var _identifiedView: ForEach<Data, ID, Content._IdentifiedView> {
    let content = self.content
    return ForEach<Data, ID, Content._IdentifiedView>(pommeData: data, id: pommeID) { content($0)._identifiedView }
  }

  // pomme: replaces extension SwiftUICore.ForEach : SwiftUI.TabContent where Content : SwiftUI.TabContent :: public var body: SwiftUICore.ForEach<Data, ID, Content>
  @MainActor @preconcurrency public var body: ForEach<Data, ID, Content> {
    return self
  }
}

// pomme: in extension SwiftUICore.ForEach where ID == Data.Element.ID, Content : SwiftUI.TabContent, Data.Element : Swift.Identifiable
extension ForEach where ID == Data.Element.ID, Content: TabContent, Data.Element: Identifiable {
  // pomme: replaces extension SwiftUICore.ForEach where ID == Data.Element.ID, Content : SwiftUI.TabContent, Data.Element : Swift.Identifiable :: public init<V>(_ data: Data, @SwiftUI.TabContentBuilder<V> content: @escaping (Data.Element) -> Content) where V == Content.TabValue
  public init<V>(_ data: Data, @TabContentBuilder<V> content: @escaping (Data.Element) -> Content) where V == Content.TabValue {
    self.init(pommeData: data, id: { $0.id }, content: content)
  }

  // pomme: replaces extension SwiftUICore.ForEach where ID == Data.Element.ID, Content : SwiftUI.TabContent, Data.Element : Swift.Identifiable :: internal init<V>(_ data: Data, @SwiftUI.TabContentBuilder<V?> liftedContent content: @escaping (Data.Element) -> Content) where V : Swift.Hashable, Content.TabValue == V?
  @usableFromInline
  internal init<V>(_ data: Data, @TabContentBuilder<V?> liftedContent content: @escaping (Data.Element) -> Content) where V: Hashable, Content.TabValue == V? {
    self.init(pommeData: data, id: { $0.id }, content: content)
  }

  // pomme: replaces extension SwiftUICore.ForEach where ID == Data.Element.ID, Content : SwiftUI.TabContent, Data.Element : Swift.Identifiable :: public init<V>(_ data: Data, @SwiftUI.TabContentBuilder<V?> content: @escaping (Data.Element) -> Content) where V : Swift.Hashable, Content.TabValue == V?
  @_disfavoredOverload
  public init<V>(_ data: Data, @TabContentBuilder<V?> content: @escaping (Data.Element) -> Content) where V: Hashable, Content.TabValue == V? {
    self.init(pommeData: data, id: { $0.id }, content: content)
  }
}

// pomme: in extension SwiftUICore.ForEach where Data == Swift.Range<Swift.Int>, ID == Swift.Int, Content : SwiftUI.TabContent
extension ForEach where Data == Range<Int>, ID == Int, Content: TabContent {
  // pomme: replaces extension SwiftUICore.ForEach where Data == Swift.Range<Swift.Int>, ID == Swift.Int, Content : SwiftUI.TabContent :: public init<V>(_ data: Swift.Range<Swift.Int>, @SwiftUI.TabContentBuilder<V> content: @escaping (Swift.Int) -> Content) where V == Content.TabValue
  public init<V>(_ data: Range<Int>, @TabContentBuilder<V> content: @escaping (Int) -> Content) where V == Content.TabValue {
    self.init(pommeData: data, id: { $0 }, content: content)
  }

  // pomme: replaces extension SwiftUICore.ForEach where Data == Swift.Range<Swift.Int>, ID == Swift.Int, Content : SwiftUI.TabContent :: internal init<V>(_ data: Swift.Range<Swift.Int>, @SwiftUI.TabContentBuilder<V?> liftedContent content: @escaping (Swift.Int) -> Content) where V : Swift.Hashable, Content.TabValue == V?
  @usableFromInline
  internal init<V>(_ data: Range<Int>, @TabContentBuilder<V?> liftedContent content: @escaping (Int) -> Content) where V: Hashable, Content.TabValue == V? {
    self.init(pommeData: data, id: { $0 }, content: content)
  }

  // pomme: replaces extension SwiftUICore.ForEach where Data == Swift.Range<Swift.Int>, ID == Swift.Int, Content : SwiftUI.TabContent :: public init<V>(_ data: Swift.Range<Swift.Int>, @SwiftUI.TabContentBuilder<V?> content: @escaping (Swift.Int) -> Content) where V : Swift.Hashable, Content.TabValue == V?
  @_disfavoredOverload
  public init<V>(_ data: Range<Int>, @TabContentBuilder<V?> content: @escaping (Int) -> Content) where V: Hashable, Content.TabValue == V? {
    self.init(pommeData: data, id: { $0 }, content: content)
  }
}

// pomme: in extension SwiftUICore.Group : SwiftUI.TabContent where Content : SwiftUI.TabContent
extension Group where Content: TabContent {
  // pomme: replaces extension SwiftUICore.Group : SwiftUI.TabContent where Content : SwiftUI.TabContent :: public init<V>(@SwiftUI.TabContentBuilder<V> content: () -> Content) where V == Content.TabValue
  public init<V>(@TabContentBuilder<V> content: () -> Content) where V == Content.TabValue {
    self.init(_content: content())
  }

  // pomme: replaces extension SwiftUICore.Group : SwiftUI.TabContent where Content : SwiftUI.TabContent :: public init<V>(@SwiftUI.TabContentBuilder<V?> content: () -> Content) where V : Swift.Hashable, Content.TabValue == V?
  @_disfavoredOverload
  public init<V>(@TabContentBuilder<V?> content: () -> Content) where V: Hashable, Content.TabValue == V? {
    self.init(_content: content())
  }

  // pomme: replaces extension SwiftUICore.Group : SwiftUI.TabContent where Content : SwiftUI.TabContent :: public var _identifiedView: Content._IdentifiedView
  /// Its tabs, where it's placed.
  @MainActor @preconcurrency public var _identifiedView: Content._IdentifiedView {
    return content._identifiedView
  }

  // pomme: replaces extension SwiftUICore.Group : SwiftUI.TabContent where Content : SwiftUI.TabContent :: public var body: SwiftUICore.Group<Content>
  @MainActor @preconcurrency public var body: Group<Content> {
    return self
  }
}

// ---- Styles ----

extension EnvironmentValues {
  /// The tab views' style within (`tabViewStyle`); automatic until set.
  var pommeTabViewStyle: Any? {
    get { pommeByName["pommeTabViewStyle"] }
    set { pommeByName["pommeTabViewStyle"] = newValue }
  }
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func tabViewStyle<S>(_ style: S) -> some SwiftUICore.View where S : SwiftUI.TabViewStyle
  nonisolated public func tabViewStyle<S>(_ style: S) -> some View where S: TabViewStyle {
    environment(\.pommeTabViewStyle, style)
  }
}

// On an iPhone these all show a tab bar.

extension TabViewStyle where Self == DefaultTabViewStyle {
  // pomme: replaces extension SwiftUI.TabViewStyle where Self == SwiftUI.DefaultTabViewStyle :: public static var automatic: SwiftUI.DefaultTabViewStyle
  public static var automatic: DefaultTabViewStyle { DefaultTabViewStyle() }
}

// pomme: in public struct DefaultTabViewStyle : SwiftUI.TabViewStyle
extension DefaultTabViewStyle {
  // pomme: replaces public struct DefaultTabViewStyle : SwiftUI.TabViewStyle :: public init()
  public init() { _pommeStorage = () }
}

@available(iOS 18.0, macOS 15.0, tvOS 18.0, visionOS 2.0, *)
extension TabViewStyle where Self == TabBarOnlyTabViewStyle {
  // pomme: replaces extension SwiftUI.TabViewStyle where Self == SwiftUI.TabBarOnlyTabViewStyle :: internal static var _tabBarOnlyABI: SwiftUI.TabBarOnlyTabViewStyle
  // (`.tabBarOnly` itself is emitted into the app; this is its symbol here.)
  @usableFromInline
  @MainActor @preconcurrency internal static var _tabBarOnlyABI: TabBarOnlyTabViewStyle {
    @_silgen_name("$s7SwiftUI12TabViewStylePA2A0c7BarOnlycdE0VRszrlE03tabfG0AEvgZ")
    get { TabBarOnlyTabViewStyle() }
  }
}

// pomme: in public struct TabBarOnlyTabViewStyle : SwiftUI.TabViewStyle
extension TabBarOnlyTabViewStyle {
  // pomme: replaces public struct TabBarOnlyTabViewStyle : SwiftUI.TabViewStyle :: public init()
  public init() { _pommeStorage = () }
}

@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension TabViewStyle where Self == SidebarAdaptableTabViewStyle {
  // pomme: replaces extension SwiftUI.TabViewStyle where Self == SwiftUI.SidebarAdaptableTabViewStyle :: public static var sidebarAdaptable: SwiftUI.SidebarAdaptableTabViewStyle
  public static var sidebarAdaptable: SidebarAdaptableTabViewStyle { SidebarAdaptableTabViewStyle() }
}

// pomme: in public struct SidebarAdaptableTabViewStyle : SwiftUI.TabViewStyle
extension SidebarAdaptableTabViewStyle {
  // pomme: replaces public struct SidebarAdaptableTabViewStyle : SwiftUI.TabViewStyle :: public init()
  public init() { _pommeStorage = () }
}

// ---- Tab content: a TabView's tabs, built ----
// (Getters say `return`: TabContent's and View's requirements carry result
// builders, which would otherwise apply to them.)

@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension TabContentBuilder {
  // pomme: replaces public struct TabContentBuilder<TabValue> where TabValue : Swift.Hashable :: public struct Content<C> : SwiftUICore.View where C : SwiftUI.TabContent
  /// A TabView's tabs, as a view: their identified views.
  public struct Content<C>: View where C: TabContent {
    var pommeContent: C

    @MainActor @preconcurrency public var body: some View {
      return pommeContent._identifiedView
    }
  }
}

// pomme: owns internal struct _TupleTabContent<T, U> where T : Swift.Hashable
@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
@usableFromInline
@frozen @MainActor @preconcurrency internal struct _TupleTabContent<T, U> where T: Hashable {
  // pomme: replaces internal struct _TupleTabContent<T, U> where T : Swift.Hashable :: internal let _identifiedView: SwiftUICore.TupleView<U>
  @usableFromInline
  @MainActor @preconcurrency internal let _identifiedView: TupleView<U>
}

// pomme: owns public struct _TabContentBodyAdaptor<C> : SwiftUICore.View where C : SwiftUI.TabContent
/// A tab content type of the app's own, as a view: its body's identified view.
@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
public struct _TabContentBodyAdaptor<C>: View where C: TabContent {
  var pommeContent: C

  // pomme: replaces public struct _TabContentBodyAdaptor<C> : SwiftUICore.View where C : SwiftUI.TabContent :: public var body: C.Body._IdentifiedView
  @MainActor @preconcurrency public var body: C.Body._IdentifiedView {
    return pommeContent.body._identifiedView
  }
}

@available(iOS 18.0, macOS 15.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension TabContent where Self._IdentifiedView == _TabContentBodyAdaptor<Self> {
  // pomme: replaces extension SwiftUI.TabContent where Self._IdentifiedView == SwiftUI._TabContentBodyAdaptor<Self> :: public var _identifiedView: SwiftUI._TabContentBodyAdaptor<Self>
  @MainActor @preconcurrency public var _identifiedView: _TabContentBodyAdaptor<Self> {
    _TabContentBodyAdaptor(pommeContent: self)
  }
}

// pomme: in extension SwiftUICore._ConditionalContent : SwiftUI.TabContent where TrueContent : SwiftUI.TabContent, FalseContent : SwiftUI.TabContent, TrueContent.TabValue == FalseContent.TabValue
extension _ConditionalContent where TrueContent: TabContent, FalseContent: TabContent, TrueContent.TabValue == FalseContent.TabValue {
  // pomme: replaces extension SwiftUICore._ConditionalContent : SwiftUI.TabContent where TrueContent : SwiftUI.TabContent, FalseContent : SwiftUI.TabContent, TrueContent.TabValue == FalseContent.TabValue :: internal init(storage: SwiftUICore._ConditionalContent<TrueContent, FalseContent>.Storage)
  @usableFromInline
  internal init(storage: Storage) {
    self.init(pommeStorage: storage)
  }

  // pomme: replaces extension SwiftUICore._ConditionalContent : SwiftUI.TabContent where TrueContent : SwiftUI.TabContent, FalseContent : SwiftUI.TabContent, TrueContent.TabValue == FalseContent.TabValue :: public var _identifiedView: SwiftUICore._ConditionalContent<TrueContent._IdentifiedView, FalseContent._IdentifiedView>
  /// The branch taken, as a view.
  @MainActor @preconcurrency public var _identifiedView: _ConditionalContent<TrueContent._IdentifiedView, FalseContent._IdentifiedView> {
    switch storage {
    case .trueContent(let t): return .init(pommeStorage: .trueContent(t._identifiedView))
    case .falseContent(let f): return .init(pommeStorage: .falseContent(f._identifiedView))
    }
  }

  // pomme: replaces extension SwiftUICore._ConditionalContent : SwiftUI.TabContent where TrueContent : SwiftUI.TabContent, FalseContent : SwiftUI.TabContent, TrueContent.TabValue == FalseContent.TabValue :: public var body: SwiftUICore._ConditionalContent<TrueContent, FalseContent>
  @MainActor @preconcurrency public var body: _ConditionalContent<TrueContent, FalseContent> { return self }
}
