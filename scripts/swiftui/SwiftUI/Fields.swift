// Text fields: TextField and SecureField, shown with UIKit's text field (its
// keyboard, editing and placeholder): the body font, the title (or prompt) as
// the placeholder, the text following the binding both ways, and the focus a
// FocusState gives it (`.focused`).

import UIKit

// ---- TextField ----

// pomme: owns public struct TextField<Label> : SwiftUICore.View where Label : SwiftUICore.View
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
@MainActor @preconcurrency
public struct TextField<Label>: View where Label: View {
  var pommeTitle: String
  var pommeText: Binding<String>
  var pommePrompt: Text?
  var pommeSelection: Binding<TextSelection?>? = nil
}

extension TextField where Label == Text {
  // pomme: replaces extension SwiftUI.TextField where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, text: SwiftUICore.Binding<Swift.String>)
  nonisolated public init(_ titleKey: LocalizedStringKey, text: Binding<String>) {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeText: text, pommePrompt: nil)
  }

  // pomme: replaces extension SwiftUI.TextField where Label == SwiftUICore.Text :: nonisolated public init<S>(_ title: S, text: SwiftUICore.Binding<Swift.String>) where S : Swift.StringProtocol
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, text: Binding<String>) where S: StringProtocol {
    self.init(pommeTitle: String(title), pommeText: text, pommePrompt: nil)
  }

  // pomme: replaces extension SwiftUI.TextField where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, text: SwiftUICore.Binding<Swift.String>, prompt: SwiftUICore.Text?)
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  nonisolated public init(_ titleKey: LocalizedStringKey, text: Binding<String>, prompt: Text?) {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeText: text, pommePrompt: prompt)
  }

  // pomme: replaces extension SwiftUI.TextField where Label == SwiftUICore.Text :: nonisolated public init<S>(_ title: S, text: SwiftUICore.Binding<Swift.String>, prompt: SwiftUICore.Text?) where S : Swift.StringProtocol
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, text: Binding<String>, prompt: Text?) where S: StringProtocol {
    self.init(pommeTitle: String(title), pommeText: text, pommePrompt: prompt)
  }

  // pomme: replaces extension SwiftUI.TextField where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, text: SwiftUICore.Binding<Swift.String>, prompt: SwiftUICore.Text?, axis: SwiftUICore.Axis)
  @available(iOS 16.0, macOS 13.0, tvOS 16.0, watchOS 9.0, *)
  nonisolated public init(_ titleKey: LocalizedStringKey, text: Binding<String>, prompt: Text?, axis: Axis) {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeText: text, pommePrompt: prompt)
  }

  // pomme: replaces extension SwiftUI.TextField where Label == SwiftUICore.Text :: nonisolated public init<S>(_ title: S, text: SwiftUICore.Binding<Swift.String>, prompt: SwiftUICore.Text?, axis: SwiftUICore.Axis) where S : Swift.StringProtocol
  @available(iOS 16.0, macOS 13.0, tvOS 16.0, watchOS 9.0, *)
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, text: Binding<String>, prompt: Text?, axis: Axis) where S: StringProtocol {
    self.init(pommeTitle: String(title), pommeText: text, pommePrompt: prompt)
  }

  // pomme: replaces extension SwiftUI.TextField where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, text: SwiftUICore.Binding<Swift.String>, axis: SwiftUICore.Axis)
  @available(iOS 16.0, macOS 13.0, tvOS 16.0, watchOS 9.0, *)
  nonisolated public init(_ titleKey: LocalizedStringKey, text: Binding<String>, axis: Axis) {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeText: text, pommePrompt: nil)
  }

  // pomme: replaces extension SwiftUI.TextField where Label == SwiftUICore.Text :: nonisolated public init<S>(_ title: S, text: SwiftUICore.Binding<Swift.String>, axis: SwiftUICore.Axis) where S : Swift.StringProtocol
  @available(iOS 16.0, macOS 13.0, tvOS 16.0, watchOS 9.0, *)
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, text: Binding<String>, axis: Axis) where S: StringProtocol {
    self.init(pommeTitle: String(title), pommeText: text, pommePrompt: nil)
  }

  // pomme: replaces extension SwiftUI.TextField where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, text: SwiftUICore.Binding<Swift.String>, selection: SwiftUICore.Binding<SwiftUI.TextSelection?>, prompt: SwiftUICore.Text? = nil, axis: SwiftUICore.Axis? = nil)
  @available(iOS 18.0, macOS 15.0, visionOS 2.0, *)
  nonisolated public init(_ titleKey: LocalizedStringKey, text: Binding<String>, selection: Binding<TextSelection?>, prompt: Text? = nil, axis: Axis? = nil) {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeText: text, pommePrompt: prompt, pommeSelection: selection)
  }

  // pomme: replaces extension SwiftUI.TextField where Label == SwiftUICore.Text :: nonisolated public init<S>(_ title: S, text: SwiftUICore.Binding<Swift.String>, selection: SwiftUICore.Binding<SwiftUI.TextSelection?>, prompt: SwiftUICore.Text? = nil, axis: SwiftUICore.Axis? = nil) where S : Swift.StringProtocol
  @available(iOS 18.0, macOS 15.0, visionOS 2.0, *)
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, text: Binding<String>, selection: Binding<TextSelection?>, prompt: Text? = nil, axis: Axis? = nil) where S: StringProtocol {
    self.init(pommeTitle: String(title), pommeText: text, pommePrompt: prompt, pommeSelection: selection)
  }
}

extension TextField: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let node = TextFieldNode(path: path, placeholder: pommePrompt?.pommeString ?? pommeTitle, text: pommeText, secure: false)
    node.selection = pommeSelection
    return [node]
  }
}

// ---- SecureField ----

// pomme: owns public struct SecureField<Label> : SwiftUICore.View where Label : SwiftUICore.View
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
@MainActor @preconcurrency
public struct SecureField<Label>: View where Label: View {
  var pommeTitle: String
  var pommeText: Binding<String>
  var pommePrompt: Text?
}

extension SecureField where Label == Text {
  // pomme: replaces extension SwiftUI.SecureField where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, text: SwiftUICore.Binding<Swift.String>)
  nonisolated public init(_ titleKey: LocalizedStringKey, text: Binding<String>) {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeText: text, pommePrompt: nil)
  }

  // pomme: replaces extension SwiftUI.SecureField where Label == SwiftUICore.Text :: nonisolated public init<S>(_ title: S, text: SwiftUICore.Binding<Swift.String>) where S : Swift.StringProtocol
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, text: Binding<String>) where S: StringProtocol {
    self.init(pommeTitle: String(title), pommeText: text, pommePrompt: nil)
  }

  // pomme: replaces extension SwiftUI.SecureField where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, text: SwiftUICore.Binding<Swift.String>, prompt: SwiftUICore.Text?)
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  nonisolated public init(_ titleKey: LocalizedStringKey, text: Binding<String>, prompt: Text?) {
    self.init(pommeTitle: titleKey.pommeLocalized(), pommeText: text, pommePrompt: prompt)
  }

  // pomme: replaces extension SwiftUI.SecureField where Label == SwiftUICore.Text :: nonisolated public init<S>(_ title: S, text: SwiftUICore.Binding<Swift.String>, prompt: SwiftUICore.Text?) where S : Swift.StringProtocol
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, text: Binding<String>, prompt: Text?) where S: StringProtocol {
    self.init(pommeTitle: String(title), pommeText: text, pommePrompt: prompt)
  }
}

extension SecureField: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    [TextFieldNode(path: path, placeholder: pommePrompt?.pommeString ?? pommeTitle, text: pommeText, secure: true)]
  }
}

// ---- the field ----

/// A text field that reports its text and focus.
final class PommeTextField: UITextField {
  var changed: ((String) -> Void)?
  var focusChanged: ((Bool) -> Void)?

  override init(frame: CGRect) {
    super.init(frame: frame)
    addTarget(self, action: #selector(editingChanged), for: .editingChanged)
    addTarget(self, action: #selector(began), for: .editingDidBegin)
    addTarget(self, action: #selector(ended), for: .editingDidEnd)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) isn't supported")
  }

  var selectionChanged: ((Range<Int>) -> Void)?

  /// The selection, in UTF-16 offsets of the text.
  var pommeSelection: Range<Int>? {
    get {
      guard let r = selectedTextRange else { return nil }
      let start = offset(from: beginningOfDocument, to: r.start)
      return start..<(start + offset(from: r.start, to: r.end))
    }
    set {
      guard let newValue,
            let start = position(from: beginningOfDocument, offset: newValue.lowerBound),
            let end = position(from: beginningOfDocument, offset: newValue.upperBound) else { return }
      selectedTextRange = textRange(from: start, to: end)
    }
  }

  @objc private func editingChanged() {
    changed?(text ?? "")
    if let s = pommeSelection { selectionChanged?(s) }
  }
  @objc private func began() { focusChanged?(true) }
  @objc private func ended() { focusChanged?(false) }
}

/// A one-line field: as wide as offered, as tall as a line of body text.
final class TextFieldNode: Node {
  let placeholder: String
  let text: Binding<String>
  let secure: Bool
  /// The selection it follows, both ways (`TextField(_:text:selection:)`).
  var selection: Binding<TextSelection?>?
  static var font: UIFont { .preferredFont(forTextStyle: .body) }

  init(path: ViewPath, placeholder: String, text: Binding<String>, secure: Bool) {
    self.placeholder = placeholder
    self.text = text
    self.secure = secure
    super.init(path: path)
  }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    CGSize(width: proposal.width ?? 10, height: TextNode.lineHeight(Self.font))
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let field: PommeTextField = renderer.view(for: path) { PommeTextField(frame: .zero) }
    field.frame = frame
    field.font = Self.font
    field.isSecureTextEntry = secure
    if field.placeholder != placeholder {
      field.placeholder = placeholder
    }
    if field.text != text.wrappedValue {
      field.text = text.wrappedValue
    }
    let binding = text
    field.changed = { binding.wrappedValue = $0 }
    if let selection {
      // The binding's selection, where it's of this text; and the field's back.
      let string = text.wrappedValue
      if case .selection(let r)? = selection.wrappedValue?.indices,
         r.upperBound <= string.endIndex,
         case let want = r.lowerBound.utf16Offset(in: string)..<r.upperBound.utf16Offset(in: string),
         field.pommeSelection != want {
        field.pommeSelection = want
      }
      field.selectionChanged = { offsets in
        let s = binding.wrappedValue
        let utf16 = s.utf16
        guard offsets.upperBound <= utf16.count else { return }
        let range = String.Index(utf16Offset: offsets.lowerBound, in: s)..<String.Index(utf16Offset: offsets.upperBound, in: s)
        let new = TextSelection(range: range)
        if selection.wrappedValue != new { selection.wrappedValue = new }
      }
    } else {
      field.selectionChanged = nil
    }
    field.accessibilityIdentifier = accessibilityIdentifier
    parent.pommeAdopt(field)
    // The focus, as a FocusState has it, and back.
    if let focus {
      field.focusChanged = { has in
        if focus.get() != has { focus.set(has) }
      }
      let wants = focus.get()
      if wants && !field.isFirstResponder {
        field.becomeFirstResponder()
      } else if !wants && field.isFirstResponder {
        field.resignFirstResponder()
      }
    } else {
      field.focusChanged = nil
    }
  }
}
