// Picker: in a form, iOS's menu picker: its label, and at the trailing edge
// the selected option's label in the secondary color with the up-down
// chevron (iOS 26, measured on the simulator). Options are the content's
// views, known by their tags. (Tapping to choose comes with menus.)

import UIKit

// pomme: owns public struct Picker<Label, SelectionValue, Content> : SwiftUICore.View where Label : SwiftUICore.View, SelectionValue : Swift.Hashable, Content : SwiftUICore.View
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
@MainActor @preconcurrency
public struct Picker<Label, SelectionValue, Content>: View where Label: View, SelectionValue: Hashable, Content: View {
  var pommeLabel: Label
  var pommeSelection: Binding<SelectionValue>
  var pommeContent: Content
}

extension Picker {
  // pomme: replaces extension SwiftUI.Picker :: nonisolated public init(selection: SwiftUICore.Binding<SelectionValue>, @SwiftUICore.ViewBuilder content: () -> Content, @SwiftUICore.ViewBuilder label: () -> Label)
  nonisolated public init(selection: Binding<SelectionValue>, @ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
    self.init(pommeLabel: label(), pommeSelection: selection, pommeContent: content())
  }
}

extension Picker where Label == Text {
  // pomme: replaces extension SwiftUI.Picker where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, selection: SwiftUICore.Binding<SelectionValue>, @SwiftUICore.ViewBuilder content: () -> Content)
  nonisolated public init(_ titleKey: LocalizedStringKey, selection: Binding<SelectionValue>, @ViewBuilder content: () -> Content) {
    self.init(pommeLabel: Text(titleKey), pommeSelection: selection, pommeContent: content())
  }

  // pomme: replaces extension SwiftUI.Picker where Label == SwiftUICore.Text :: nonisolated public init<S>(_ title: S, selection: SwiftUICore.Binding<SelectionValue>, @SwiftUICore.ViewBuilder content: () -> Content) where S : Swift.StringProtocol
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, selection: Binding<SelectionValue>, @ViewBuilder content: () -> Content) where S: StringProtocol {
    self.init(pommeLabel: Text(title), pommeSelection: selection, pommeContent: content())
  }
}

extension Picker: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let label = graph.resolve(pommeLabel, at: path.appending("label"), preferences)
    let options = graph.resolve(pommeContent, at: path.appending("options"), preferences)
    // The option whose tag is the selection (an optional selection's value counts).
    let selected = pommeUnwrapped(pommeSelection.wrappedValue)
    let current = options.first { $0.tag != nil && $0.tag == selected }
    for case let text as TextNode in [current].compactMap({ $0 }) {
      text.style.color = .secondaryLabel
    }
    return [PickerNode(path: path, label: label.first, value: current)]
  }
}

/// A value as a tag compares: an optional's wrapped value (nil: none).
func pommeUnwrapped(_ value: some Hashable) -> AnyHashable? {
  let mirror = Mirror(reflecting: value)
  guard mirror.displayStyle == .optional else { return AnyHashable(value) }
  guard let wrapped = mirror.children.first?.value else { return nil }
  if let inner = wrapped as? any Hashable {
    // (An AnyHashable inside: its own value.)
    if let any = inner as? AnyHashable { return any }
    return AnyHashable(inner)
  }
  return nil
}

/// The picker's row: its label at the leading edge; the selected option and
/// the chevron at the trailing edge.
final class PickerNode: Node {
  let label: Node?
  let value: Node?
  /// The up-down chevron (9 × 14 pt), 6.67 pt after the value (measured).
  static let chevron = CGSize(width: 9, height: 14)
  static let chevronGap: CGFloat = 6.67

  init(path: ViewPath, label: Node?, value: Node?) {
    self.label = label
    self.value = value
    super.init(path: path)
  }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    let l = label?.sizeThatFits(.unspecified) ?? .zero
    let v = value?.sizeThatFits(.unspecified) ?? .zero
    let width = proposal.width ?? l.width + 8 + v.width + Self.chevronGap + Self.chevron.width
    return CGSize(width: width, height: max(l.height, v.height, Self.chevron.height))
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    if let label {
      let s = label.sizeThatFits(.unspecified)
      label.place(in: CGRect(x: frame.minX, y: frame.midY - s.height / 2, width: s.width, height: s.height), of: parent, renderer)
    }
    let chevronX = frame.maxX - Self.chevron.width - 1.33
    let chevron: UIImageView = renderer.view(for: path.appending("chevron")) { UIImageView(image: Self.chevronImage()) }
    chevron.frame = CGRect(x: chevronX, y: frame.midY - Self.chevron.height / 2, width: Self.chevron.width, height: Self.chevron.height)
    parent.pommeAdopt(chevron)
    if let value {
      let s = value.sizeThatFits(.unspecified)
      value.place(in: CGRect(x: chevronX - Self.chevronGap - s.width, y: frame.midY - s.height / 2, width: s.width, height: s.height), of: parent, renderer)
    }
  }

  /// Two chevrons, up and down, in the secondary color.
  static func chevronImage() -> UIImage {
    UIGraphicsImageRenderer(size: chevron).image { _ in
      let path = UIBezierPath()
      let (w, h) = (chevron.width, chevron.height)
      path.move(to: CGPoint(x: 1, y: h * 0.36))
      path.addLine(to: CGPoint(x: w / 2, y: 1.2))
      path.addLine(to: CGPoint(x: w - 1, y: h * 0.36))
      path.move(to: CGPoint(x: 1, y: h * 0.64))
      path.addLine(to: CGPoint(x: w / 2, y: h - 1.2))
      path.addLine(to: CGPoint(x: w - 1, y: h * 0.64))
      path.lineWidth = 1.6
      path.lineCapStyle = .round
      path.lineJoinStyle = .round
      UIColor.secondaryLabel.setStroke()
      path.stroke()
    }
  }
}

extension _TagTraitWritingModifier: NodeModifier {
  /// The views it modifies are known by its tag.
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    for node in nodes {
      node.tag = AnyHashable(tag)
    }
  }
}
