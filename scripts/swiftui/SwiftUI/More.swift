// More of SwiftUI on iOS, shown with UIKit: dismissing, text input settings,
// navigation links and views, links, text editors, confirmation dialogs,
// segmented pickers, tasks that restart when a value changes, and the
// modifiers pomme takes but doesn't show yet (noted where they are).

import Combine
import UIKit

// ---- dismiss, isPresented ----

// pomme: owns public struct DismissAction
@available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
@MainActor @preconcurrency
public struct DismissAction {
  var pommeAction: (@MainActor () -> Void)?
}

extension DismissAction {
  // pomme: replaces public struct DismissAction :: public func callAsFunction()
  /// Dismiss the sheet (or pop the screen) the view is in; elsewhere nothing.
  @MainActor @preconcurrency public func callAsFunction() {
    pommeAction?()
  }
}

extension EnvironmentValues {
  // pomme: replaces extension SwiftUICore.EnvironmentValues :: public var dismiss: SwiftUI.DismissAction
  public var dismiss: DismissAction {
    pommeByName["dismiss"] as? DismissAction ?? MainActor.assumeIsolated { DismissAction(pommeAction: nil) }
  }

  // pomme: replaces extension SwiftUICore.EnvironmentValues :: public var isPresented: Swift.Bool
  public var isPresented: Bool {
    pommeByName["isPresented"] as? Bool ?? false
  }
}

// ---- Text input ----

/// How a text field takes input, from the modifiers around it.
struct TextInputTraits {
  var keyboardType: UIKeyboardType = .default
  var autocapitalization: UITextAutocapitalizationType?
  var autocorrection: UITextAutocorrectionType = .default
  var returnKey: UIReturnKeyType = .default
  var onSubmit: [() -> Void] = []
}

extension EnvironmentValues {
  var pommeTextInput: TextInputTraits {
    get { pommeByName["pommeTextInput"] as? TextInputTraits ?? TextInputTraits() }
    set { pommeByName["pommeTextInput"] = newValue }
  }
}

struct _PommeTextInputModifier: ViewModifier, PrimitiveModifier {
  var change: (inout TextInputTraits) -> Void
  typealias Body = Never

  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    graph.withEnvironment({ change(&$0.pommeTextInput) }, content)
  }
}

// pomme: in public struct SubmitLabel : Swift.Sendable
extension SubmitLabel {
  package var pommeReturnKey: UIReturnKeyType

  package init(pommeReturnKey: UIReturnKeyType) {
    _pommeStorage = ()
    self.pommeReturnKey = pommeReturnKey
  }

  // pomme: replaces public struct SubmitLabel : Swift.Sendable :: public static var done: SwiftUI.SubmitLabel
  public static var done: SubmitLabel { SubmitLabel(pommeReturnKey: .done) }
  // pomme: replaces public struct SubmitLabel : Swift.Sendable :: public static var go: SwiftUI.SubmitLabel
  public static var go: SubmitLabel { SubmitLabel(pommeReturnKey: .go) }
  // pomme: replaces public struct SubmitLabel : Swift.Sendable :: public static var send: SwiftUI.SubmitLabel
  public static var send: SubmitLabel { SubmitLabel(pommeReturnKey: .send) }
  // pomme: replaces public struct SubmitLabel : Swift.Sendable :: public static var join: SwiftUI.SubmitLabel
  public static var join: SubmitLabel { SubmitLabel(pommeReturnKey: .join) }
  // pomme: replaces public struct SubmitLabel : Swift.Sendable :: public static var route: SwiftUI.SubmitLabel
  public static var route: SubmitLabel { SubmitLabel(pommeReturnKey: .route) }
  // pomme: replaces public struct SubmitLabel : Swift.Sendable :: public static var search: SwiftUI.SubmitLabel
  public static var search: SubmitLabel { SubmitLabel(pommeReturnKey: .search) }
  // pomme: replaces public struct SubmitLabel : Swift.Sendable :: public static var `return`: SwiftUI.SubmitLabel
  public static var `return`: SubmitLabel { SubmitLabel(pommeReturnKey: .default) }
  // pomme: replaces public struct SubmitLabel : Swift.Sendable :: public static var next: SwiftUI.SubmitLabel
  public static var next: SubmitLabel { SubmitLabel(pommeReturnKey: .next) }
  // pomme: replaces public struct SubmitLabel : Swift.Sendable :: public static var `continue`: SwiftUI.SubmitLabel
  public static var `continue`: SubmitLabel { SubmitLabel(pommeReturnKey: .continue) }
}

// pomme: in public struct TextInputAutocapitalization : Swift.Sendable
extension TextInputAutocapitalization {
  package var pommeType: UITextAutocapitalizationType

  package init(pommeType: UITextAutocapitalizationType) {
    _pommeStorage = ()
    self.pommeType = pommeType
  }

  // pomme: replaces public struct TextInputAutocapitalization : Swift.Sendable :: public static var never: SwiftUI.TextInputAutocapitalization
  public static var never: TextInputAutocapitalization { TextInputAutocapitalization(pommeType: .none) }
  // pomme: replaces public struct TextInputAutocapitalization : Swift.Sendable :: public static var words: SwiftUI.TextInputAutocapitalization
  public static var words: TextInputAutocapitalization { TextInputAutocapitalization(pommeType: .words) }
  // pomme: replaces public struct TextInputAutocapitalization : Swift.Sendable :: public static var sentences: SwiftUI.TextInputAutocapitalization
  public static var sentences: TextInputAutocapitalization { TextInputAutocapitalization(pommeType: .sentences) }
  // pomme: replaces public struct TextInputAutocapitalization : Swift.Sendable :: public static var characters: SwiftUI.TextInputAutocapitalization
  public static var characters: TextInputAutocapitalization { TextInputAutocapitalization(pommeType: .allCharacters) }
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func keyboardType(_ type: UIKit.UIKeyboardType) -> some SwiftUICore.View
  nonisolated public func keyboardType(_ type: UIKeyboardType) -> some View {
    modifier(_PommeTextInputModifier { $0.keyboardType = type })
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func textInputAutocapitalization(_ autocapitalization: SwiftUI.TextInputAutocapitalization?) -> some SwiftUICore.View
  nonisolated public func textInputAutocapitalization(_ autocapitalization: TextInputAutocapitalization?) -> some View {
    modifier(_PommeTextInputModifier { $0.autocapitalization = autocapitalization?.pommeType })
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func autocorrectionDisabled(_ disable: Swift.Bool = true) -> some SwiftUICore.View
  nonisolated public func autocorrectionDisabled(_ disable: Bool = true) -> some View {
    modifier(_PommeTextInputModifier { $0.autocorrection = disable ? .no : .default })
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func disableAutocorrection(_ disable: Swift.Bool?) -> some SwiftUICore.View
  nonisolated public func disableAutocorrection(_ disable: Bool?) -> some View {
    modifier(_PommeTextInputModifier { $0.autocorrection = disable == true ? .no : disable == false ? .yes : .default })
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func submitLabel(_ submitLabel: SwiftUI.SubmitLabel) -> some SwiftUICore.View
  nonisolated public func submitLabel(_ submitLabel: SubmitLabel) -> some View {
    modifier(_PommeTextInputModifier { $0.returnKey = submitLabel.pommeReturnKey })
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func onSubmit(of triggers: SwiftUI.SubmitTriggers = .text, _ action: @escaping () -> Swift.Void) -> some SwiftUICore.View
  /// (Text fields submit; search fields aren't shown yet.)
  nonisolated public func onSubmit(of triggers: SubmitTriggers = .text, _ action: @escaping () -> Void) -> some View {
    let text = triggers.contains(.text)
    return modifier(_PommeTextInputModifier { if text { $0.onSubmit.append(action) } })
  }
}

// ---- Modifiers taken, not shown yet ----

/// A modifier pomme takes and doesn't show yet: its content as it is.
struct _PommeUnshownModifier: ViewModifier, NodeModifier {
  typealias Body = Never
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {}
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func refreshable(@_inheritActorContext action: @escaping @Sendable () async -> Swift.Void) -> some SwiftUICore.View
  /// (Pull to refresh isn't shown yet: pomme's UIKit has no refresh control.)
  nonisolated public func refreshable(@_inheritActorContext action: @escaping @Sendable () async -> Void) -> some View {
    modifier(_PommeUnshownModifier())
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func searchable(text: SwiftUICore.Binding<Swift.String>, placement: SwiftUI.SearchFieldPlacement = .automatic, prompt: SwiftUICore.Text? = nil) -> some SwiftUICore.View
  /// (The search field isn't shown yet: pomme's UIKit has no search controller.)
  nonisolated public func searchable(text: Binding<String>, placement: SearchFieldPlacement = .automatic, prompt: Text? = nil) -> some View {
    modifier(_PommeUnshownModifier())
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func textSelection<S>(_ selectability: S) -> some SwiftUICore.View where S : SwiftUICore.TextSelectability
  /// (Selecting text in a Text isn't offered yet.)
  nonisolated public func textSelection<S>(_ selectability: S) -> some View where S: TextSelectability {
    modifier(_PommeUnshownModifier())
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func navigationViewStyle<S>(_ style: S) -> some SwiftUICore.View where S : SwiftUI.NavigationViewStyle
  /// (On iPhone a navigation view is a stack whatever its style.)
  nonisolated public func navigationViewStyle<S>(_ style: S) -> some View where S: NavigationViewStyle {
    modifier(_PommeUnshownModifier())
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func listStyle<S>(_ style: S) -> some SwiftUICore.View where S : SwiftUI.ListStyle
  /// (pomme's lists are inset grouped, iOS's default, whatever the style.)
  nonisolated public func listStyle<S>(_ style: S) -> some View where S: ListStyle {
    modifier(_PommeUnshownModifier())
  }
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func scrollContentBackground(_ visibility: SwiftUICore.Visibility) -> some SwiftUICore.View
  /// Whether a list shows its gray background.
  nonisolated public func scrollContentBackground(_ visibility: Visibility) -> some View {
    environment(\.pommeScrollContentBackgroundHidden, visibility == .hidden)
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func pickerStyle<S>(_ style: S) -> some SwiftUICore.View where S : SwiftUI.PickerStyle
  nonisolated public func pickerStyle<S>(_ style: S) -> some View where S: PickerStyle {
    environment(\.pommePickerSegmented, style is SegmentedPickerStyle)
  }
}

extension EnvironmentValues {
  var pommeScrollContentBackgroundHidden: Bool {
    get { pommeByName["pommeScrollContentBackgroundHidden"] as? Bool ?? false }
    set { pommeByName["pommeScrollContentBackgroundHidden"] = newValue }
  }

  var pommePickerSegmented: Bool {
    get { pommeByName["pommePickerSegmented"] as? Bool ?? false }
    set { pommeByName["pommePickerSegmented"] = newValue }
  }
}

// ---- Styles ----

// pomme: in public struct InsetGroupedListStyle : SwiftUI.ListStyle
extension InsetGroupedListStyle {
  // pomme: replaces public struct InsetGroupedListStyle : SwiftUI.ListStyle :: public init()
  public init() { _pommeStorage = () }
}

extension ListStyle where Self == InsetGroupedListStyle {
  // pomme: replaces extension SwiftUI.ListStyle where Self == SwiftUI.InsetGroupedListStyle :: public static var insetGrouped: SwiftUI.InsetGroupedListStyle
  public static var insetGrouped: InsetGroupedListStyle { InsetGroupedListStyle() }
}

// pomme: in public struct SegmentedPickerStyle : SwiftUI.PickerStyle
extension SegmentedPickerStyle {
  // pomme: replaces public struct SegmentedPickerStyle : SwiftUI.PickerStyle :: public init()
  public init() { _pommeStorage = () }
}

extension PickerStyle where Self == SegmentedPickerStyle {
  // pomme: replaces extension SwiftUI.PickerStyle where Self == SwiftUI.SegmentedPickerStyle :: public static var segmented: SwiftUI.SegmentedPickerStyle
  public static var segmented: SegmentedPickerStyle { SegmentedPickerStyle() }
}

// pomme: in public struct StackNavigationViewStyle : SwiftUI.NavigationViewStyle
extension StackNavigationViewStyle {
  // pomme: replaces public struct StackNavigationViewStyle : SwiftUI.NavigationViewStyle :: public init()
  public init() { _pommeStorage = () }
}

extension NavigationViewStyle where Self == StackNavigationViewStyle {
  // pomme: replaces extension SwiftUI.NavigationViewStyle where Self == SwiftUI.StackNavigationViewStyle :: public static var stack: SwiftUI.StackNavigationViewStyle
  public static var stack: StackNavigationViewStyle { StackNavigationViewStyle() }
}

// pomme: in public struct BorderedButtonStyle : SwiftUI.PrimitiveButtonStyle
extension BorderedButtonStyle {
  // pomme: replaces public struct BorderedButtonStyle : SwiftUI.PrimitiveButtonStyle :: public init()
  public init() { _pommeStorage = () }
}

extension PrimitiveButtonStyle where Self == BorderedButtonStyle {
  // pomme: replaces extension SwiftUI.PrimitiveButtonStyle where Self == SwiftUI.BorderedButtonStyle :: public static var bordered: SwiftUI.BorderedButtonStyle
  public static var bordered: BorderedButtonStyle { BorderedButtonStyle() }
}

// pomme: in public struct BorderedProminentButtonStyle : SwiftUI.PrimitiveButtonStyle
extension BorderedProminentButtonStyle {
  // pomme: replaces public struct BorderedProminentButtonStyle : SwiftUI.PrimitiveButtonStyle :: public init()
  public init() { _pommeStorage = () }
}

extension PrimitiveButtonStyle where Self == BorderedProminentButtonStyle {
  // pomme: replaces extension SwiftUI.PrimitiveButtonStyle where Self == SwiftUI.BorderedProminentButtonStyle :: public static var borderedProminent: SwiftUI.BorderedProminentButtonStyle
  public static var borderedProminent: BorderedProminentButtonStyle { BorderedProminentButtonStyle() }
}

// ---- NavigationView ----

// pomme: in public struct NavigationView<Content> : SwiftUICore.View where Content : SwiftUICore.View
extension NavigationView {
  package var pommeContent: Content

  // pomme: replaces public struct NavigationView<Content> : SwiftUICore.View where Content : SwiftUICore.View :: public init(@SwiftUICore.ViewBuilder content: () -> Content)
  public init(@ViewBuilder content: () -> Content) {
    _pommeStorage = ()
    pommeContent = content()
  }
}

extension NavigationView: PrimitiveView {
  /// On iPhone, a navigation stack.
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let content = pommeContent
    return graph.resolve(NavigationStack { content }, at: path.appending("stack"), preferences)
  }
}

// ---- NavigationLink ----

// pomme: in public struct NavigationLink<Label, Destination> : SwiftUICore.View where Label : SwiftUICore.View, Destination : SwiftUICore.View
extension NavigationLink {
  package var pommeDestination: Destination
  package var pommeLabel: Label


  // pomme: replaces public struct NavigationLink<Label, Destination> : SwiftUICore.View where Label : SwiftUICore.View, Destination : SwiftUICore.View :: public var body: some SwiftUICore.View
  public var body: Never { fatalError("pomme resolves a NavigationLink itself") }
  // pomme: replaces public struct NavigationLink<Label, Destination> : SwiftUICore.View where Label : SwiftUICore.View, Destination : SwiftUICore.View :: public typealias Body = @_opaqueReturnTypeOf("$s7SwiftUI14NavigationLinkV4bodyQrvp", 0) __<Label, Destination>
  public typealias Body = Never
}

extension NavigationLink {
  // pomme: replaces extension SwiftUI.NavigationLink :: public init(destination: Destination, @SwiftUICore.ViewBuilder label: () -> Label)
  public init(destination: Destination, @ViewBuilder label: () -> Label) {
    _pommeStorage = ()
    pommeDestination = destination
    pommeLabel = label()
  }
}

extension NavigationLink where Label == Text {
  // pomme: replaces extension SwiftUI.NavigationLink where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, destination: Destination)
  nonisolated public init(_ titleKey: LocalizedStringKey, destination: Destination) {
    _pommeStorage = ()
    pommeDestination = destination
    pommeLabel = Text(verbatim: titleKey.pommeLocalized())
  }
}

extension NavigationLink: PrimitiveView {
  /// Its label as a button that pushes the destination on the navigation
  /// stack it's in (its view in the environment of the link).
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let label = pommeLabel
    let nodes = graph.resolve(Button(action: {}) { label }, at: path.appending("button"), preferences)
    let destination = AnyView(pommeDestination)
    let environment = graph.environment
    (nodes.first as? ButtonNode)?.viewAction = { view in
      pommePush(destination, environment: environment, from: view)
    }
    return nodes
  }
}

/// Push `content` on the navigation controller of the screen `view` is in.
@MainActor func pommePush(_ content: AnyView, environment: EnvironmentValues, from view: UIView) {
  var responder: UIResponder? = view
  while let r = responder {
    if let controller = r as? UIViewController, let navigation = controller.navigationController {
      let screen = PushedHostingController(rootView: AnyView(EmptyView()))
      var env = environment
      env.pommeByName["dismiss"] = DismissAction { [weak screen] in
        screen?.navigationController?.popViewController(animated: true)
      }
      env.pommeUnderNavigationBar = true
      screen.rootView = AnyView(content.modifier(_PommeEnvironmentReplacing(environment: env)))
      navigation.pushViewController(screen, animated: true)
      return
    }
    responder = r.next
  }
}

/// A screen a navigation link pushed: its title and toolbar from its content.
final class PushedHostingController: UIHostingController<AnyView> {}

// ---- Link ----

// pomme: in public struct Link<Label> : SwiftUICore.View where Label : SwiftUICore.View
extension Link {
  package var pommeDestination: URL
  package var pommeLabel: Label

  // pomme: replaces public struct Link<Label> : SwiftUICore.View where Label : SwiftUICore.View :: public init(destination: Foundation.URL, @SwiftUICore.ViewBuilder label: () -> Label)
  public init(destination: URL, @ViewBuilder label: () -> Label) {
    _pommeStorage = ()
    pommeDestination = destination
    pommeLabel = label()
  }

  // pomme: replaces public struct Link<Label> : SwiftUICore.View where Label : SwiftUICore.View :: public var body: some SwiftUICore.View
  public var body: Never { fatalError("pomme resolves a Link itself") }
  // pomme: replaces public struct Link<Label> : SwiftUICore.View where Label : SwiftUICore.View :: public typealias Body = @_opaqueReturnTypeOf("$s7SwiftUI4LinkV4bodyQrvp", 0) __<Label>
  public typealias Body = Never
}

extension Link where Label == Text {
  // pomme: replaces extension SwiftUI.Link where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, destination: Foundation.URL)
  nonisolated public init(_ titleKey: LocalizedStringKey, destination: URL) {
    _pommeStorage = ()
    pommeDestination = destination
    pommeLabel = Text(verbatim: titleKey.pommeLocalized())
  }
}

extension Link: PrimitiveView {
  /// A button that opens its URL (as the environment's openURL does: the app that handles it).
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let url = pommeDestination
    let label = pommeLabel
    return graph.resolve(Button(action: { UIApplication.shared.open(url) }) { label }, at: path.appending("button"), preferences)
  }
}

// ---- task(id:) ----

// pomme: owns public struct _TaskValueModifier2<ID> : SwiftUICore.ViewModifier where ID : Swift.Equatable
@available(iOS 26.4, macOS 26.4, tvOS 26.4, watchOS 26.4, visionOS 26.4, *)
public struct _TaskValueModifier2<ID>: ViewModifier where ID: Equatable {
  var pommeID: ID
  var pommeWork: TaskWork
}

extension _TaskValueModifier2 {
  // pomme: replaces public struct _TaskValueModifier2<ID> : SwiftUICore.ViewModifier where ID : Swift.Equatable :: internal init(id: ID, name: Swift.String, executorPreference taskExecutor: (any _Concurrency.TaskExecutor)?, priority: _Concurrency.TaskPriority, action: sending @escaping @isolated(any) () async -> Swift.Void)
  @usableFromInline
  nonisolated internal init(id: ID, name: String, executorPreference taskExecutor: (any TaskExecutor)?, priority: TaskPriority, action: sending @escaping @isolated(any) () async -> Void) {
    pommeID = id
    pommeWork = TaskWork(priority: priority, action: action)
  }
}

extension _TaskValueModifier2: NodeModifier {
  /// A task for the value: a new value cancels it and starts another.
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    graph.addTask(pommeWork, at: path.appending("id:\(String(describing: pommeID))"))
  }
}

extension _TaskValueModifier: NodeModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    graph.addTask(TaskWork(priority: priority, action: action), at: path.appending("id:\(String(describing: value))"))
  }
}

// ---- onChange(of:initial:), LazyVStack (declared in SwiftUICore) ----

extension PommeOnChangeModifier: NodeModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    let (old, first) = graph.followedValue(at: path, as: V.self)
    let new = value
    _ = graph.valueChanged(at: path, to: new)
    if first {
      if initial { graph.runAfterUpdate { action(new, new) } }
    } else if let old, old != new {
      graph.runAfterUpdate { action(old, new) }
    }
  }
}

extension LazyVStack: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    graph.resolve(pommeStack, at: path, preferences)
  }
}

// ---- onReceive ----

/// The latest action for a subscription (a view's action is made anew each update).
final class ActionBox {
  var action: (Any) -> Void
  init(_ action: @escaping (Any) -> Void) { self.action = action }
}

extension SubscriptionView: PrimitiveView {
  /// Its content; it subscribes to the publisher the first time it's in the
  /// tree (and stays subscribed while it is), running the latest action with
  /// each value, on the main thread.
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let action = self.action
    let run: (Any) -> Void = { if let v = $0 as? PublisherType.Output { action(v) } }
    graph.subscribedNow.insert(path)
    if let existing = graph.subscriptions[path] {
      existing.action.action = run
    } else {
      let box = ActionBox(run)
      let cancellable = publisher.sink { value in
        if Thread.isMainThread {
          box.action(value)
        } else {
          DispatchQueue.main.async { box.action(value) }
        }
      }
      graph.subscriptions[path] = (AnyCancellable(cancellable), box)
    }
    return graph.resolve(content, at: path.appending("content"), preferences)
  }
}

// ---- .id(_:) ----

extension IDView: PrimitiveView {
  /// Its content, its place in the tree named by the id: another id is
  /// another view (its state starts afresh), as in SwiftUI.
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    graph.resolve(pommeContent, at: path.appending(AnyHashable(pommeID)), preferences)
  }
}
