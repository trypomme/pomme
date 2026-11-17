// .alert: SwiftUI's alerts, shown with UIKit's UIAlertController, as Apple's
// SwiftUI shows them on iOS. While `isPresented` is true, the hosting
// controller presents one: the title, the message (its text), and an action
// for each Button in `actions` (its title and role; "OK" when there are
// none). Choosing one runs its action and sets `isPresented` to false.

import UIKit

/// What an alert shows; the host presents it (ViewHost.syncAlerts).
struct AlertRequest {
  var title: String
  var message: String?
  var buttons: [AlertButton]
  var isPresented: Binding<Bool>
}

struct AlertButton {
  var title: String
  var role: ButtonRole?
  var action: @MainActor () -> Void
}

/// Views that are an alert's actions: buttons, and what groups them.
protocol AlertActions {
  @MainActor func pommeAlertButtons() -> [AlertButton]
}

/// The text a view shows, when it's text (an alert's title or message).
@MainActor func pommeText(of view: Any) -> String? {
  if let t = view as? Text { return t.pommeString }
  for child in Mirror(reflecting: view).children {
    if let s = pommeText(of: child.value) { return s }
  }
  return nil
}

@MainActor func pommeAlertButtonsIn(_ view: Any) -> [AlertButton] {
  if let a = view as? any AlertActions { return a.pommeAlertButtons() }
  return []
}

extension Button: AlertActions {
  func pommeAlertButtons() -> [AlertButton] {
    [AlertButton(title: pommeText(of: pommeLabel) ?? "OK", role: pommeRole, action: pommeAction)]
  }
}

extension TupleView: AlertActions {
  func pommeAlertButtons() -> [AlertButton] {
    Mirror(reflecting: value).children.flatMap { pommeAlertButtonsIn($0.value) }
  }
}

extension Optional: AlertActions where Wrapped: View {
  func pommeAlertButtons() -> [AlertButton] {
    if case .some(let v) = self { return pommeAlertButtonsIn(v) }
    return []
  }
}

extension _ConditionalContent: AlertActions where TrueContent: View, FalseContent: View {
  func pommeAlertButtons() -> [AlertButton] {
    switch storage {
    case .trueContent(let v): return pommeAlertButtonsIn(v)
    case .falseContent(let v): return pommeAlertButtonsIn(v)
    }
  }
}

extension Group: AlertActions where Content: View {
  func pommeAlertButtons() -> [AlertButton] {
    Mirror(reflecting: self).children.flatMap { pommeAlertButtonsIn($0.value) }
  }
}

/// `.alert(…)` on a view: the view, and the alert while it's presented.
struct _PommeAlertModifier: ViewModifier {
  var title: String
  var isPresented: Binding<Bool>
  var actions: Any
  var message: Any?

  func body(content: Content) -> Never { fatalError("pomme resolves alerts itself") }
}

extension _PommeAlertModifier: NodeModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    guard isPresented.wrappedValue else { return }
    var buttons = pommeAlertButtonsIn(actions)
    if buttons.isEmpty {
      buttons = [AlertButton(title: "OK", role: .cancel, action: {})]
    }
    graph.addAlert(AlertRequest(title: title, message: message.flatMap { pommeText(of: $0) }, buttons: buttons, isPresented: isPresented), at: path)
  }
}

@available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func alert<A>(_ titleKey: SwiftUICore.LocalizedStringKey, isPresented: SwiftUICore.Binding<Swift.Bool>, @SwiftUICore.ViewBuilder actions: () -> A) -> some SwiftUICore.View where A : SwiftUICore.View
  nonisolated public func alert<A>(_ titleKey: LocalizedStringKey, isPresented: Binding<Bool>, @ViewBuilder actions: () -> A) -> some View where A: View {
    modifier(_PommeAlertModifier(title: titleKey.pommeLocalized(), isPresented: isPresented, actions: actions(), message: nil))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func alert<S, A>(_ title: S, isPresented: SwiftUICore.Binding<Swift.Bool>, @SwiftUICore.ViewBuilder actions: () -> A) -> some SwiftUICore.View where S : Swift.StringProtocol, A : SwiftUICore.View
  @_disfavoredOverload
  nonisolated public func alert<S, A>(_ title: S, isPresented: Binding<Bool>, @ViewBuilder actions: () -> A) -> some View where S: StringProtocol, A: View {
    modifier(_PommeAlertModifier(title: String(title), isPresented: isPresented, actions: actions(), message: nil))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func alert<A>(_ title: SwiftUICore.Text, isPresented: SwiftUICore.Binding<Swift.Bool>, @SwiftUICore.ViewBuilder actions: () -> A) -> some SwiftUICore.View where A : SwiftUICore.View
  nonisolated public func alert<A>(_ title: Text, isPresented: Binding<Bool>, @ViewBuilder actions: () -> A) -> some View where A: View {
    modifier(_PommeAlertModifier(title: MainActor.assumeIsolated { title.pommeString }, isPresented: isPresented, actions: actions(), message: nil))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func alert<A, M>(_ titleKey: SwiftUICore.LocalizedStringKey, isPresented: SwiftUICore.Binding<Swift.Bool>, @SwiftUICore.ViewBuilder actions: () -> A, @SwiftUICore.ViewBuilder message: () -> M) -> some SwiftUICore.View where A : SwiftUICore.View, M : SwiftUICore.View
  nonisolated public func alert<A, M>(_ titleKey: LocalizedStringKey, isPresented: Binding<Bool>, @ViewBuilder actions: () -> A, @ViewBuilder message: () -> M) -> some View where A: View, M: View {
    modifier(_PommeAlertModifier(title: titleKey.pommeLocalized(), isPresented: isPresented, actions: actions(), message: message()))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func alert<S, A, M>(_ title: S, isPresented: SwiftUICore.Binding<Swift.Bool>, @SwiftUICore.ViewBuilder actions: () -> A, @SwiftUICore.ViewBuilder message: () -> M) -> some SwiftUICore.View where S : Swift.StringProtocol, A : SwiftUICore.View, M : SwiftUICore.View
  @_disfavoredOverload
  nonisolated public func alert<S, A, M>(_ title: S, isPresented: Binding<Bool>, @ViewBuilder actions: () -> A, @ViewBuilder message: () -> M) -> some View where S: StringProtocol, A: View, M: View {
    modifier(_PommeAlertModifier(title: String(title), isPresented: isPresented, actions: actions(), message: message()))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func alert<A, M>(_ title: SwiftUICore.Text, isPresented: SwiftUICore.Binding<Swift.Bool>, @SwiftUICore.ViewBuilder actions: () -> A, @SwiftUICore.ViewBuilder message: () -> M) -> some SwiftUICore.View where A : SwiftUICore.View, M : SwiftUICore.View
  nonisolated public func alert<A, M>(_ title: Text, isPresented: Binding<Bool>, @ViewBuilder actions: () -> A, @ViewBuilder message: () -> M) -> some View where A: View, M: View {
    modifier(_PommeAlertModifier(title: MainActor.assumeIsolated { title.pommeString }, isPresented: isPresented, actions: actions(), message: message()))
  }
}

extension ViewHost {
  /// Present the alerts the tree asks for that aren't shown, and take away
  /// those it no longer does.
  func syncAlerts(_ requests: [ViewPath: AlertRequest]) {
    for (path, shown) in shownAlerts where requests[path] == nil {
      shownAlerts[path] = nil
      if shown.presentingViewController != nil {
        shown.dismiss(animated: true)
      }
    }
    guard var presenter = controller else { return }
    while let p = presenter.presentedViewController { presenter = p }
    for (path, r) in requests where shownAlerts[path] == nil {
      let alert = UIAlertController(title: r.title, message: r.message, preferredStyle: .alert)
      for b in r.buttons {
        let style: UIAlertAction.Style = b.role == .destructive ? .destructive : b.role == .cancel ? .cancel : .default
        let binding = r.isPresented
        alert.addAction(UIAlertAction(title: b.title, style: style) { [weak self] _ in
          MainActor.assumeIsolated {
            self?.shownAlerts[path] = nil
            binding.wrappedValue = false
            b.action()
          }
        })
      }
      shownAlerts[path] = alert
      presenter.present(alert, animated: true)
      presenter = alert
    }
  }
}
