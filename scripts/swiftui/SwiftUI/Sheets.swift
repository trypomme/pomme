// .sheet and .fullScreenCover: SwiftUI's modal presentations, shown as Apple's
// SwiftUI shows them on iOS, by a hosting controller presented with UIKit (a
// page sheet, or full screen). While `isPresented` is true (or `item` isn't
// nil) the controller is shown with the content, in the environment of the
// view that asked; when it goes (dismissed by the app, or dragged down), the
// binding is reset and `onDismiss` runs. The content's
// `.presentationDetents(_:)` sets the sheet's detents.

import UIKit

/// What a sheet shows; the host presents it (ViewHost.syncSheets).
struct SheetRequest {
  var content: AnyView
  var fullScreen: Bool
  /// Set the binding to say the sheet has gone (false, or nil).
  var reset: @MainActor () -> Void
  var onDismiss: (() -> Void)?
  /// What tells one sheet from the next at the same place (an item's id).
  var identity: AnyHashable
}

/// `.sheet(…)`, `.fullScreenCover(…)` on a view: the view, and the sheet while presented.
struct _PommeSheetModifier: ViewModifier {
  var request: @MainActor () -> SheetRequest?

  func body(content: Content) -> Never { fatalError("pomme resolves sheets itself") }
}

extension _PommeSheetModifier: NodeModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    guard var r = request() else { return }
    // The content's environment: the presenter's, its dismiss action the sheet's.
    var environment = graph.environment
    let reset = r.reset
    environment.pommeByName["dismiss"] = DismissAction(pommeAction: reset)
    environment.pommeByName["isPresented"] = true
    environment.pommeUnderNavigationBar = false
    r.content = AnyView(r.content.modifier(_PommeEnvironmentReplacing(environment: environment)))
    graph.addSheet(r, at: path)
  }
}

/// A sheet's content gets the environment of the view that presents it.
struct _PommeEnvironmentReplacing: ViewModifier {
  var environment: EnvironmentValues
  func body(content: Content) -> Never { fatalError("pomme resolves this itself") }
}

extension _PommeEnvironmentReplacing: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    graph.withEnvironment({ $0 = environment }, content)
  }
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func sheet<Content>(isPresented: SwiftUICore.Binding<Swift.Bool>, onDismiss: (() -> Swift.Void)? = nil, @SwiftUICore.ViewBuilder content: @escaping () -> Content) -> some SwiftUICore.View where Content : SwiftUICore.View
  nonisolated public func sheet<Content>(isPresented: Binding<Bool>, onDismiss: (() -> Void)? = nil, @ViewBuilder content: @escaping () -> Content) -> some View where Content: View {
    modifier(_PommeSheetModifier(request: pommeSheet(isPresented, onDismiss, fullScreen: false, content)))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func sheet<Item, Content>(item: SwiftUICore.Binding<Item?>, onDismiss: (() -> Swift.Void)? = nil, @SwiftUICore.ViewBuilder content: @escaping (Item) -> Content) -> some SwiftUICore.View where Item : Swift.Identifiable, Content : SwiftUICore.View
  nonisolated public func sheet<Item, Content>(item: Binding<Item?>, onDismiss: (() -> Void)? = nil, @ViewBuilder content: @escaping (Item) -> Content) -> some View where Item: Identifiable, Content: View {
    modifier(_PommeSheetModifier(request: pommeSheet(item, onDismiss, fullScreen: false, content)))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func fullScreenCover<Content>(isPresented: SwiftUICore.Binding<Swift.Bool>, onDismiss: (() -> Swift.Void)? = nil, @SwiftUICore.ViewBuilder content: @escaping () -> Content) -> some SwiftUICore.View where Content : SwiftUICore.View
  nonisolated public func fullScreenCover<Content>(isPresented: Binding<Bool>, onDismiss: (() -> Void)? = nil, @ViewBuilder content: @escaping () -> Content) -> some View where Content: View {
    modifier(_PommeSheetModifier(request: pommeSheet(isPresented, onDismiss, fullScreen: true, content)))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func fullScreenCover<Item, Content>(item: SwiftUICore.Binding<Item?>, onDismiss: (() -> Swift.Void)? = nil, @SwiftUICore.ViewBuilder content: @escaping (Item) -> Content) -> some SwiftUICore.View where Item : Swift.Identifiable, Content : SwiftUICore.View
  nonisolated public func fullScreenCover<Item, Content>(item: Binding<Item?>, onDismiss: (() -> Void)? = nil, @ViewBuilder content: @escaping (Item) -> Content) -> some View where Item: Identifiable, Content: View {
    modifier(_PommeSheetModifier(request: pommeSheet(item, onDismiss, fullScreen: true, content)))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func presentationDetents(_ detents: Swift.Set<SwiftUI.PresentationDetent>) -> some SwiftUICore.View
  nonisolated public func presentationDetents(_ detents: Set<PresentationDetent>) -> some View {
    modifier(_PommeSheetSettings(detents: detents))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func presentationDragIndicator(_ visibility: SwiftUICore.Visibility) -> some SwiftUICore.View
  nonisolated public func presentationDragIndicator(_ visibility: Visibility) -> some View {
    modifier(_PommeSheetSettings(dragIndicator: visibility))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func interactiveDismissDisabled(_ isDisabled: Swift.Bool = true) -> some SwiftUICore.View
  nonisolated public func interactiveDismissDisabled(_ isDisabled: Bool = true) -> some View {
    modifier(_PommeSheetSettings(dismissDisabled: isDisabled))
  }
}

private func pommeSheet<Content: View>(_ isPresented: Binding<Bool>, _ onDismiss: (() -> Void)?, fullScreen: Bool, _ content: @escaping () -> Content) -> @MainActor () -> SheetRequest? {
  {
    guard isPresented.wrappedValue else { return nil }
    return SheetRequest(content: AnyView(content()), fullScreen: fullScreen, reset: { isPresented.wrappedValue = false }, onDismiss: onDismiss, identity: AnyHashable(true))
  }
}

private func pommeSheet<Item: Identifiable, Content: View>(_ item: Binding<Item?>, _ onDismiss: (() -> Void)?, fullScreen: Bool, _ content: @escaping (Item) -> Content) -> @MainActor () -> SheetRequest? {
  {
    guard let value = item.wrappedValue else { return nil }
    return SheetRequest(content: AnyView(content(value)), fullScreen: fullScreen, reset: { item.wrappedValue = nil }, onDismiss: onDismiss, identity: AnyHashable(value.id))
  }
}

// ---- What the content says about its sheet ----

/// The sheet settings a presented tree asks for (the outermost wins, as in SwiftUI).
struct SheetSettings {
  var detents: Set<PresentationDetent>?
  var dragIndicator: Visibility?
  var dismissDisabled: Bool?
}

struct _PommeSheetSettings: ViewModifier {
  var settings = SheetSettings()
  init(detents: Set<PresentationDetent>) { settings.detents = detents }
  init(dragIndicator: Visibility) { settings.dragIndicator = dragIndicator }
  init(dismissDisabled: Bool) { settings.dismissDisabled = dismissDisabled }

  func body(content: Content) -> Never { fatalError("pomme resolves this itself") }
}

extension _PommeSheetSettings: NodeModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    // Applied innermost first: an outer one overrides.
    if let d = settings.detents { graph.sheetSettings.detents = d }
    if let v = settings.dragIndicator { graph.sheetSettings.dragIndicator = v }
    if let b = settings.dismissDisabled { graph.sheetSettings.dismissDisabled = b }
  }
}

// pomme: in public struct PresentationDetent : Swift.Hashable, Swift.Sendable
extension PresentationDetent {
  package var pommeKind: PommeDetentKind

  // pomme: replaces public struct PresentationDetent : Swift.Hashable, Swift.Sendable :: public static let medium: SwiftUI.PresentationDetent
  public static let medium = PresentationDetent(pommeKind: .medium)
  // pomme: replaces public struct PresentationDetent : Swift.Hashable, Swift.Sendable :: public static let large: SwiftUI.PresentationDetent
  public static let large = PresentationDetent(pommeKind: .large)

  // pomme: replaces public struct PresentationDetent : Swift.Hashable, Swift.Sendable :: public static func fraction(_ fraction: CoreFoundation.CGFloat) -> SwiftUI.PresentationDetent
  public static func fraction(_ fraction: CGFloat) -> PresentationDetent { PresentationDetent(pommeKind: .fraction(fraction)) }
  // pomme: replaces public struct PresentationDetent : Swift.Hashable, Swift.Sendable :: public static func height(_ height: CoreFoundation.CGFloat) -> SwiftUI.PresentationDetent
  public static func height(_ height: CGFloat) -> PresentationDetent { PresentationDetent(pommeKind: .height(height)) }

  package init(pommeKind: PommeDetentKind) {
    _pommeStorage = ()
    self.pommeKind = pommeKind
  }

  // pomme: replaces public struct PresentationDetent : Swift.Hashable, Swift.Sendable :: public static func == (a: SwiftUI.PresentationDetent, b: SwiftUI.PresentationDetent) -> Swift.Bool
  public static func == (a: PresentationDetent, b: PresentationDetent) -> Bool { a.pommeKind == b.pommeKind }
  // pomme: replaces public struct PresentationDetent : Swift.Hashable, Swift.Sendable :: public func hash(into hasher: inout Swift.Hasher)
  public func hash(into hasher: inout Hasher) { hasher.combine(pommeKind) }
  // pomme: replaces public struct PresentationDetent : Swift.Hashable, Swift.Sendable :: public var hashValue: Swift.Int
  public var hashValue: Int { pommeKind.hashValue }
}

package enum PommeDetentKind: Hashable, Sendable {
  case medium, large
  case fraction(CGFloat)
  case height(CGFloat)
}

extension PresentationDetent {
  /// UIKit's detent for it.
  @MainActor var pommeUIKit: UISheetPresentationController.Detent {
    switch pommeKind {
    case .medium: return .medium()
    case .large: return .large()
    case .fraction(let f): return .custom { $0.maximumDetentValue * f }
    case .height(let h): return .custom { _ in h }
    }
  }

  /// Smallest first, as UIKit wants them (fractions and heights by their size on a 700 pt sheet).
  var pommeOrder: CGFloat {
    switch pommeKind {
    case .medium: return 0.553 * 700
    case .large: return 700
    case .fraction(let f): return f * 700
    case .height(let h): return h
    }
  }
}

// ---- Presenting ----

/// A sheet's controller: it says when it's gone.
final class SheetHostingController: UIHostingController<AnyView> {
  var identity: AnyHashable = AnyHashable(true)
  var gone: (() -> Void)?

  override func viewDidDisappear(_ animated: Bool) {
    super.viewDidDisappear(animated)
    if isBeingDismissed || presentingViewController == nil {
      let g = gone
      gone = nil
      g?()
    }
  }

  /// Apply what the content asked of its sheet.
  func apply(_ settings: SheetSettings) {
    guard let sheet = sheetPresentationController else { return }
    if let detents = settings.detents, !detents.isEmpty {
      sheet.detents = detents.sorted { $0.pommeOrder < $1.pommeOrder }.map(\.pommeUIKit)
    }
    if let v = settings.dragIndicator {
      sheet.prefersGrabberVisible = v == .visible
    }
    isModalInPresentation = settings.dismissDisabled ?? false
  }
}

extension ViewHost {
  /// Present the sheets the tree asks for that aren't shown, update those
  /// that are, and dismiss those it no longer asks for.
  func syncSheets(_ requests: [ViewPath: SheetRequest]) {
    for (path, shown) in shownSheets where requests[path]?.identity != shown.identity {
      shownSheets[path] = nil
      shown.gone = nil
      if shown.presentingViewController != nil {
        shown.dismiss(animated: true)
      }
    }
    for (path, r) in requests {
      if let shown = shownSheets[path] {
        shown.rootView = r.content
        continue
      }
      guard var presenter = controller else { return }
      while let p = presenter.presentedViewController { presenter = p }
      let sheet = SheetHostingController(rootView: r.content)
      sheet.identity = r.identity
      sheet.modalPresentationStyle = r.fullScreen ? .fullScreen : .pageSheet
      sheet.gone = { [weak self] in
        MainActor.assumeIsolated {
          self?.shownSheets[path] = nil
          r.reset()
          r.onDismiss?()
        }
      }
      shownSheets[path] = sheet
      presenter.present(sheet, animated: true)
    }
  }

  /// A sheet's settings, from its content.
  func applySheetSettings(_ settings: SheetSettings) {
    (controller as? SheetHostingController)?.apply(settings)
  }
}
