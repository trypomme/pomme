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

  init(title: String?, image: UIImage?) {
    self.title = title
    self.image = image
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
      node.tabItem = item
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
      item: TabItem(title: pommeTitle, image: pommeImage),
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
