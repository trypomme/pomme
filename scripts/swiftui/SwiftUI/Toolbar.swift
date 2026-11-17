// .toolbar: items a screen puts in its navigation bar, as SwiftUI does on
// iOS: those placed at the leading edge (`.navigation`, `.cancellationAction`,
// `.topBarLeading`) on the left, the others at the trailing edge, `.principal`
// as the title. Each is shown by a bar button item whose custom view hosts
// the item's views (in the environment of the screen). `.toolbar(.hidden,
// for: .navigationBar)` and `.navigationBarHidden(_:)` hide the bar;
// `.navigationSubtitle(_:)` sets the bar's subtitle.

import UIKit

/// An item a toolbar asks for.
struct ToolbarEntry {
  var placement: ToolbarItemPlacement
  var content: AnyView
}

/// Toolbar content pomme knows: its items.
protocol PommeToolbarItems {
  @MainActor func pommeToolbarEntries() -> [ToolbarEntry]
}

/// The items of any toolbar content (what groups them, looked through).
@MainActor func pommeToolbarEntriesIn(_ content: Any) -> [ToolbarEntry] {
  if let c = content as? any PommeToolbarItems { return c.pommeToolbarEntries() }
  // Optional and _ConditionalContent (buildIf, buildEither), and tuples.
  let mirror = Mirror(reflecting: content)
  if mirror.displayStyle == .optional {
    return mirror.children.first.map { pommeToolbarEntriesIn($0.value) } ?? []
  }
  return mirror.children.flatMap { pommeToolbarEntriesIn($0.value) }
}

// ---- Placements ----

// pomme: in public struct ToolbarItemPlacement
extension ToolbarItemPlacement {
  package var pommeName: String

  package init(pommeName: String) {
    _pommeStorage = ()
    self.pommeName = pommeName
  }

  // pomme: replaces public struct ToolbarItemPlacement :: public static let automatic: SwiftUI.ToolbarItemPlacement
  public static let automatic = ToolbarItemPlacement(pommeName: "automatic")
  // pomme: replaces public struct ToolbarItemPlacement :: public static let principal: SwiftUI.ToolbarItemPlacement
  public static let principal = ToolbarItemPlacement(pommeName: "principal")
  // pomme: replaces public struct ToolbarItemPlacement :: public static let navigation: SwiftUI.ToolbarItemPlacement
  public static let navigation = ToolbarItemPlacement(pommeName: "leading")
  // pomme: replaces public struct ToolbarItemPlacement :: public static let primaryAction: SwiftUI.ToolbarItemPlacement
  public static let primaryAction = ToolbarItemPlacement(pommeName: "trailing")
  // pomme: replaces public struct ToolbarItemPlacement :: public static let secondaryAction: SwiftUI.ToolbarItemPlacement
  public static let secondaryAction = ToolbarItemPlacement(pommeName: "trailing")
  // pomme: replaces public struct ToolbarItemPlacement :: public static let status: SwiftUI.ToolbarItemPlacement
  public static let status = ToolbarItemPlacement(pommeName: "bottom")
  // pomme: replaces public struct ToolbarItemPlacement :: public static let confirmationAction: SwiftUI.ToolbarItemPlacement
  public static let confirmationAction = ToolbarItemPlacement(pommeName: "trailing")
  // pomme: replaces public struct ToolbarItemPlacement :: public static let cancellationAction: SwiftUI.ToolbarItemPlacement
  public static let cancellationAction = ToolbarItemPlacement(pommeName: "leading")
  // pomme: replaces public struct ToolbarItemPlacement :: public static let destructiveAction: SwiftUI.ToolbarItemPlacement
  public static let destructiveAction = ToolbarItemPlacement(pommeName: "trailing")
  // pomme: replaces public struct ToolbarItemPlacement :: public static let keyboard: SwiftUI.ToolbarItemPlacement
  public static let keyboard = ToolbarItemPlacement(pommeName: "keyboard")
  // pomme: replaces public struct ToolbarItemPlacement :: public static var topBarLeading: SwiftUI.ToolbarItemPlacement
  public static var topBarLeading: ToolbarItemPlacement { ToolbarItemPlacement(pommeName: "leading") }
  // pomme: replaces public struct ToolbarItemPlacement :: public static var topBarTrailing: SwiftUI.ToolbarItemPlacement
  public static var topBarTrailing: ToolbarItemPlacement { ToolbarItemPlacement(pommeName: "trailing") }
  // pomme: replaces public struct ToolbarItemPlacement :: public static let navigationBarLeading: SwiftUI.ToolbarItemPlacement
  public static let navigationBarLeading = ToolbarItemPlacement(pommeName: "leading")
  // pomme: replaces public struct ToolbarItemPlacement :: public static let navigationBarTrailing: SwiftUI.ToolbarItemPlacement
  public static let navigationBarTrailing = ToolbarItemPlacement(pommeName: "trailing")
  // pomme: replaces public struct ToolbarItemPlacement :: public static let bottomBar: SwiftUI.ToolbarItemPlacement
  public static let bottomBar = ToolbarItemPlacement(pommeName: "bottom")
}

// pomme: in public struct ToolbarPlacement
extension ToolbarPlacement {
  package var pommeName: String

  package init(pommeName: String) {
    _pommeStorage = ()
    self.pommeName = pommeName
  }

  // pomme: replaces public struct ToolbarPlacement :: public static var automatic: SwiftUI.ToolbarPlacement
  public static var automatic: ToolbarPlacement { ToolbarPlacement(pommeName: "automatic") }
  // pomme: replaces public struct ToolbarPlacement :: public static var bottomBar: SwiftUI.ToolbarPlacement
  public static var bottomBar: ToolbarPlacement { ToolbarPlacement(pommeName: "bottomBar") }
  // pomme: replaces public struct ToolbarPlacement :: public static var navigationBar: SwiftUI.ToolbarPlacement
  public static var navigationBar: ToolbarPlacement { ToolbarPlacement(pommeName: "navigationBar") }
  // pomme: replaces public struct ToolbarPlacement :: public static var tabBar: SwiftUI.ToolbarPlacement
  public static var tabBar: ToolbarPlacement { ToolbarPlacement(pommeName: "tabBar") }
}

// ---- Items ----

// pomme: in public struct ToolbarItem<ID, Content> : SwiftUI.ToolbarContent where Content : SwiftUICore.View
extension ToolbarItem {
  package var pommePlacement: ToolbarItemPlacement
  package var pommeContent: Content
}

extension ToolbarItem where ID == () {
  // pomme: replaces extension SwiftUI.ToolbarItem where ID == () :: nonisolated public init(placement: SwiftUI.ToolbarItemPlacement = .automatic, @SwiftUICore.ViewBuilder content: () -> Content)
  nonisolated public init(placement: ToolbarItemPlacement = .automatic, @ViewBuilder content: () -> Content) {
    _pommeStorage = ()
    pommePlacement = placement
    pommeContent = content()
  }
}

extension ToolbarItem where ID == String {
  // pomme: replaces extension SwiftUI.ToolbarItem : SwiftUI.CustomizableToolbarContent where ID == Swift.String :: nonisolated public init(id: Swift.String, placement: SwiftUI.ToolbarItemPlacement = .automatic, showsByDefault: Swift.Bool, @SwiftUICore.ViewBuilder content: () -> Content)
  nonisolated public init(id: String, placement: ToolbarItemPlacement = .automatic, showsByDefault: Bool, @ViewBuilder content: () -> Content) {
    _pommeStorage = ()
    pommePlacement = placement
    pommeContent = content()
  }
}

extension ToolbarItem: PommeToolbarItems {
  func pommeToolbarEntries() -> [ToolbarEntry] {
    [ToolbarEntry(placement: pommePlacement, content: AnyView(pommeContent))]
  }
}

// pomme: in public struct ToolbarItemGroup<Content> : SwiftUI.ToolbarContent where Content : SwiftUICore.View
extension ToolbarItemGroup {
  package var pommePlacement: ToolbarItemPlacement
  package var pommeContent: Content

  // pomme: replaces public struct ToolbarItemGroup<Content> : SwiftUI.ToolbarContent where Content : SwiftUICore.View :: public init(placement: SwiftUI.ToolbarItemPlacement = .automatic, @SwiftUICore.ViewBuilder content: () -> Content)
  public init(placement: ToolbarItemPlacement = .automatic, @ViewBuilder content: () -> Content) {
    _pommeStorage = ()
    pommePlacement = placement
    pommeContent = content()
  }
}

extension ToolbarItemGroup: PommeToolbarItems {
  /// One item for the group (its views side by side, as a bar shows a group).
  func pommeToolbarEntries() -> [ToolbarEntry] {
    [ToolbarEntry(placement: pommePlacement, content: AnyView(HStack { pommeContent }))]
  }
}

// pomme: in internal struct TupleToolbarContent<T> : SwiftUI.ToolbarContent, SwiftUI.CustomizableToolbarContent
extension TupleToolbarContent {
  package var pommeValue: T

  // pomme: replaces internal struct TupleToolbarContent<T> : SwiftUI.ToolbarContent, SwiftUI.CustomizableToolbarContent :: internal init(_ value: T)
  @usableFromInline
  internal init(_ value: T) {
    _pommeStorage = ()
    pommeValue = value
  }
}

extension TupleToolbarContent: PommeToolbarItems {
  func pommeToolbarEntries() -> [ToolbarEntry] {
    Mirror(reflecting: pommeValue).children.flatMap { pommeToolbarEntriesIn($0.value) }
  }
}

// pomme: in internal struct LimitedAvailabilityToolbarContent : SwiftUI.ToolbarContent, SwiftUI.CustomizableToolbarContent
extension LimitedAvailabilityToolbarContent {
  // pomme: replaces internal struct LimitedAvailabilityToolbarContent : SwiftUI.ToolbarContent, SwiftUI.CustomizableToolbarContent :: internal let storage: SwiftUI.LimitedAvailabilityToolbarContentStorageBase
  package var pommeContent: Any

  // pomme: replaces internal struct LimitedAvailabilityToolbarContent : SwiftUI.ToolbarContent, SwiftUI.CustomizableToolbarContent :: internal init(erasing content: some ToolbarContent)
  @usableFromInline
  internal init(erasing content: some ToolbarContent) {
    pommeContent = content
  }
}

extension LimitedAvailabilityToolbarContent: PommeToolbarItems {
  func pommeToolbarEntries() -> [ToolbarEntry] {
    pommeToolbarEntriesIn(pommeContent)
  }
}

/// Toolbar content wrapped by a modifier pomme doesn't show (`sharedBackgroundVisibility`): its items.
struct _PommeToolbarContentWrapper<Content: ToolbarContent>: ToolbarContent, PommeToolbarItems {
  var content: Content
  var body: Never { return fatalError("pomme reads toolbar content itself") }
  func pommeToolbarEntries() -> [ToolbarEntry] { pommeToolbarEntriesIn(content) }
}

extension ToolbarContent {
  // pomme: replaces extension SwiftUI.ToolbarContent :: nonisolated public func sharedBackgroundVisibility(_ visibility: SwiftUICore.Visibility) -> some SwiftUI.ToolbarContent
  /// (The glass behind a bar item: pomme draws none behind custom items.)
  nonisolated public func sharedBackgroundVisibility(_ visibility: Visibility) -> some ToolbarContent {
    MainActor.assumeIsolated { _PommeToolbarContentWrapper(content: self) }
  }
}

// ---- The builder ----

extension ToolbarContentBuilder {
  // pomme: replaces public struct ToolbarContentBuilder :: public static func buildExpression<Content>(_ content: Content) -> Content where Content : SwiftUI.ToolbarContent
  public static func buildExpression<Content>(_ content: Content) -> Content where Content: ToolbarContent { content }
  // pomme: replaces public struct ToolbarContentBuilder :: public static func buildBlock<Content>(_ content: Content) -> some SwiftUI.ToolbarContent where Content : SwiftUI.ToolbarContent
  public static func buildBlock<Content>(_ content: Content) -> some ToolbarContent where Content: ToolbarContent { content }
  // pomme: replaces extension SwiftUI.ToolbarContentBuilder :: public static func buildIf<Content>(_ content: Content?) -> Content? where Content : SwiftUI.ToolbarContent
  public static func buildIf<Content>(_ content: Content?) -> Content? where Content: ToolbarContent { content }
  // pomme: replaces extension SwiftUI.ToolbarContentBuilder :: public static func buildEither<TrueContent, FalseContent>(first: TrueContent) -> SwiftUICore._ConditionalContent<TrueContent, FalseContent> where TrueContent : SwiftUI.ToolbarContent, FalseContent : SwiftUI.ToolbarContent
  public static func buildEither<TrueContent, FalseContent>(first: TrueContent) -> _ConditionalContent<TrueContent, FalseContent> where TrueContent: ToolbarContent, FalseContent: ToolbarContent {
    _ConditionalContent(pommeStorage: .trueContent(first))
  }
  // pomme: replaces extension SwiftUI.ToolbarContentBuilder :: public static func buildEither<TrueContent, FalseContent>(second: FalseContent) -> SwiftUICore._ConditionalContent<TrueContent, FalseContent> where TrueContent : SwiftUI.ToolbarContent, FalseContent : SwiftUI.ToolbarContent
  public static func buildEither<TrueContent, FalseContent>(second: FalseContent) -> _ConditionalContent<TrueContent, FalseContent> where TrueContent: ToolbarContent, FalseContent: ToolbarContent {
    _ConditionalContent(pommeStorage: .falseContent(second))
  }
  // pomme: replaces extension SwiftUI.ToolbarContentBuilder :: public static func buildLimitedAvailability<Content>(_ content: Content) -> some SwiftUI.ToolbarContent where Content : SwiftUI.ToolbarContent
  public static func buildLimitedAvailability<Content>(_ content: Content) -> some ToolbarContent where Content: ToolbarContent {
    MainActor.assumeIsolated { _PommeToolbarContentWrapper(content: content) }
  }
  // pomme: replaces extension SwiftUI.ToolbarContentBuilder :: public static func buildLimitedAvailability(_ content: any SwiftUI.ToolbarContent) -> some SwiftUI.ToolbarContent
  public static func buildLimitedAvailability(_ content: any ToolbarContent) -> some ToolbarContent {
    LimitedAvailabilityToolbarContent(erasing: _PommeAnyToolbarContent(content: content))
  }
}

struct _PommeAnyToolbarContent: ToolbarContent, PommeToolbarItems {
  var content: Any
  var body: Never { return fatalError("pomme reads toolbar content itself") }
  func pommeToolbarEntries() -> [ToolbarEntry] { pommeToolbarEntriesIn(content) }
}

// ---- On a view ----

/// `.toolbar { … }`: its items, in the environment of the view they're on,
/// for the navigation stack to show.
struct _PommeToolbarModifier: ViewModifier, NodeModifier {
  var content: Any
  typealias Body = Never

  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    let environment = graph.environment
    for var entry in pommeToolbarEntriesIn(content) {
      entry.content = AnyView(entry.content.modifier(_PommeEnvironmentReplacing(environment: environment)))
      preferences.toolbar.append(entry)
    }
  }
}

/// `.toolbar(.hidden, for: .navigationBar)`, `.navigationBarHidden(_:)`, `.navigationSubtitle(_:)`.
struct _PommeNavigationBarModifier: ViewModifier, NodeModifier {
  var hidden: Bool?
  var subtitle: (() -> String)?
  typealias Body = Never

  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    if let hidden { preferences.navigationBarHidden = hidden }
    if let subtitle { preferences.navigationSubtitle = subtitle() }
  }
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func toolbar<Content>(@SwiftUI.ToolbarContentBuilder content: () -> Content) -> some SwiftUICore.View where Content : SwiftUI.ToolbarContent
  nonisolated public func toolbar<Content>(@ToolbarContentBuilder content: () -> Content) -> some View where Content: ToolbarContent {
    modifier(_PommeToolbarModifier(content: content()))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func toolbar<Content>(@SwiftUICore.ViewBuilder content: () -> Content) -> some SwiftUICore.View where Content : SwiftUICore.View
  /// Views as a toolbar's content: one trailing item.
  nonisolated public func toolbar<Content>(@ViewBuilder content: () -> Content) -> some View where Content: View {
    let c = content()
    return modifier(_PommeToolbarModifier(content: _PommeAnyToolbarContent(content: MainActor.assumeIsolated { ToolbarItem(placement: .automatic) { c } })))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func toolbar(_ visibility: SwiftUICore.Visibility, for bars: SwiftUI.ToolbarPlacement...) -> some SwiftUICore.View
  nonisolated public func toolbar(_ visibility: Visibility, for bars: ToolbarPlacement...) -> some View {
    let navigationBar = bars.isEmpty || bars.contains { $0.pommeName == "navigationBar" || $0.pommeName == "automatic" }
    let hidden: Bool? = navigationBar ? (visibility == .hidden ? true : visibility == .visible ? false : nil) : nil
    return modifier(_PommeNavigationBarModifier(hidden: hidden))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func navigationBarHidden(_ hidden: Swift.Bool) -> some SwiftUICore.View
  nonisolated public func navigationBarHidden(_ hidden: Bool) -> some View {
    modifier(_PommeNavigationBarModifier(hidden: hidden))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func navigationSubtitle(_ subtitleKey: SwiftUICore.LocalizedStringKey) -> some SwiftUICore.View
  nonisolated public func navigationSubtitle(_ subtitleKey: LocalizedStringKey) -> some View {
    modifier(_PommeNavigationBarModifier(subtitle: { subtitleKey.pommeLocalized() }))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func navigationSubtitle(_ subtitle: SwiftUICore.Text) -> some SwiftUICore.View
  nonisolated public func navigationSubtitle(_ subtitle: Text) -> some View {
    modifier(_PommeNavigationBarModifier(subtitle: { subtitle.pommeString }))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func navigationSubtitle<S>(_ subtitle: S) -> some SwiftUICore.View where S : Swift.StringProtocol
  @_disfavoredOverload
  nonisolated public func navigationSubtitle<S>(_ subtitle: S) -> some View where S: StringProtocol {
    let s = String(subtitle)
    return modifier(_PommeNavigationBarModifier(subtitle: { s }))
  }
}

// ---- In the bar ----

/// A bar item's view: its SwiftUI content, sized to fit.
final class ToolbarItemHost {
  let controller: UIHostingController<AnyView>
  let item: UIBarButtonItem

  init(content: AnyView) {
    controller = UIHostingController(rootView: content)
    controller.sizingOptions = .intrinsicContentSize
    controller.view.backgroundColor = .clear
    controller.view.frame.size = controller.sizeThatFits(in: CGSize(width: 1000, height: 44))
    item = UIBarButtonItem(customView: controller.view)
  }

  func update(_ content: AnyView) {
    controller.rootView = content
    controller.view.frame.size = controller.sizeThatFits(in: CGSize(width: 1000, height: 44))
  }
}

extension UIViewController {
  /// Show a screen's toolbar items in its navigation bar.
  func showToolbar(_ entries: [ToolbarEntry], hosts: inout [String: [ToolbarItemHost]]) {
    var leading: [UIBarButtonItem] = []
    var trailing: [UIBarButtonItem] = []
    var used: [String: Int] = [:]
    var principal: UIView?
    for entry in entries {
      let side = entry.placement.pommeName
      guard side != "keyboard", side != "bottom" else { continue }
      let index = used[side, default: 0]
      used[side] = index + 1
      var list = hosts[side, default: []]
      let host: ToolbarItemHost
      if index < list.count {
        host = list[index]
        host.update(entry.content)
      } else {
        host = ToolbarItemHost(content: entry.content)
        list.append(host)
        hosts[side] = list
      }
      switch side {
      case "leading": leading.append(host.item)
      case "principal": principal = host.controller.view
      default: trailing.append(host.item)
      }
    }
    if (navigationItem.leftBarButtonItems ?? []) != leading {
      navigationItem.leftBarButtonItems = leading
    }
    // (The first trailing item is the rightmost: SwiftUI lists them leading to trailing.)
    let right = Array(trailing.reversed())
    if (navigationItem.rightBarButtonItems ?? []) != right {
      navigationItem.rightBarButtonItems = right
    }
    if navigationItem.titleView !== principal {
      navigationItem.titleView = principal
    }
  }
}
