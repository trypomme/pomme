// Controls: toggles, sliders and buttons, shown with UIKit's switch, slider
// and button, and wired to their bindings and actions.

import UIKit

// ---- Toggle ----

// pomme: owns public struct Toggle<Label> : SwiftUICore.View where Label : SwiftUICore.View
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
@MainActor @preconcurrency
public struct Toggle<Label>: View where Label: View {
  var pommeIsOn: Binding<Bool>
  var pommeLabel: Label
}

extension Toggle {
  // pomme: replaces public struct Toggle<Label> : SwiftUICore.View where Label : SwiftUICore.View :: public init(isOn: SwiftUICore.Binding<Swift.Bool>, @SwiftUICore.ViewBuilder label: () -> Label)
  @MainActor @preconcurrency
  public init(isOn: Binding<Bool>, @ViewBuilder label: () -> Label) {
    pommeIsOn = isOn
    pommeLabel = label()
  }
}

extension Toggle where Label == Text {
  // pomme: replaces extension SwiftUI.Toggle where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, isOn: SwiftUICore.Binding<Swift.Bool>)
  nonisolated public init(_ titleKey: LocalizedStringKey, isOn: Binding<Bool>) {
    pommeIsOn = isOn
    pommeLabel = Text(verbatim: titleKey.pommeLocalized())
  }

  // pomme: replaces extension SwiftUI.Toggle where Label == SwiftUICore.Text :: nonisolated public init<S>(_ title: S, isOn: SwiftUICore.Binding<Swift.Bool>) where S : Swift.StringProtocol
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, isOn: Binding<Bool>) where S: StringProtocol {
    pommeIsOn = isOn
    pommeLabel = Text(title)
  }
}

extension Toggle: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    [ToggleNode(path: path, label: graph.resolve(pommeLabel, at: path.appending("label"), preferences), isOn: pommeIsOn)]
  }
}

/// A switch that reports its changes.
final class PommeSwitch: UISwitch {
  var changed: ((Bool) -> Void)?

  override init(frame: CGRect) {
    super.init(frame: frame)
    addTarget(self, action: #selector(valueChanged), for: .valueChanged)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) isn't supported")
  }

  @objc private func valueChanged() {
    changed?(isOn)
  }
}

/// A toggle: its label, and a switch at the trailing edge.
final class ToggleNode: Node {
  let label: [Node]
  let isOn: Binding<Bool>
  /// The switch's size, as UIKit has it (it depends on the design).
  static var switchSize: CGSize { measuringSwitch.intrinsicContentSize }
  private static let measuringSwitch = UISwitch()
  /// iOS 26's switch reaches 2 pt past the row's content (measured).
  static let switchOverhang: CGFloat = 2
  static let spacing: CGFloat = 8

  init(path: ViewPath, label: [Node], isOn: Binding<Bool>) {
    self.label = label
    self.isOn = isOn
    super.init(path: path)
  }

  private func labelProposal(_ proposal: ProposedViewSize) -> ProposedViewSize {
    ProposedViewSize(width: proposal.width.map { max($0 - Self.switchSize.width - Self.spacing, 0) }, height: proposal.height)
  }

  /// As wide as offered (the switch goes to the trailing edge), and as tall
  /// as its label: the switch reaches past it (iOS 26, measured).
  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    let labelSize = label.first?.sizeThatFits(labelProposal(proposal)) ?? .zero
    let width = proposal.width ?? labelSize.width + Self.spacing + Self.switchSize.width
    return CGSize(width: width, height: labelSize.height)
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    if let label = label.first {
      let size = label.sizeThatFits(labelProposal(ProposedViewSize(frame.size)))
      label.place(in: CGRect(x: frame.minX, y: frame.midY - size.height / 2, width: size.width, height: size.height), of: parent, renderer)
    }
    let control: PommeSwitch = renderer.view(for: path.appending("switch")) { PommeSwitch(frame: .zero) }
    control.frame = CGRect(x: frame.maxX + Self.switchOverhang - Self.switchSize.width, y: frame.midY - Self.switchSize.height / 2,
                           width: Self.switchSize.width, height: Self.switchSize.height)
    if control.isOn != isOn.wrappedValue {
      control.isOn = isOn.wrappedValue
    }
    let binding = isOn
    control.changed = { binding.wrappedValue = $0 }
    applyAccessibility(to: control)
    parent.pommeAdopt(control)
  }
}

// ---- Slider ----

// pomme: owns public struct Slider<Label, ValueLabel> : SwiftUICore.View where Label : SwiftUICore.View, ValueLabel : SwiftUICore.View
@available(iOS 13.0, macOS 10.15, watchOS 6.0, *)
@MainActor @preconcurrency
public struct Slider<Label, ValueLabel>: View where Label: View, ValueLabel: View {
  /// The value, as a fraction of the range, and how to set it (stepped).
  var pommeFraction: () -> Double
  var pommeSetFraction: (Double) -> Void
  var pommeEditingChanged: (Bool) -> Void
}

extension Slider where Label == EmptyView, ValueLabel == EmptyView {
  // pomme: replaces extension SwiftUI.Slider where Label == SwiftUICore.EmptyView, ValueLabel == SwiftUICore.EmptyView :: nonisolated public init<V>(value: SwiftUICore.Binding<V>, in bounds: Swift.ClosedRange<V> = 0...1, onEditingChanged: @escaping (Swift.Bool) -> Swift.Void = { _ in }) where V : Swift.BinaryFloatingPoint, V.Stride : Swift.BinaryFloatingPoint
  nonisolated public init<V>(value: Binding<V>, in bounds: ClosedRange<V> = 0...1, onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V: BinaryFloatingPoint, V.Stride: BinaryFloatingPoint {
    self.init(pommeValue: value, in: bounds, step: nil, onEditingChanged: onEditingChanged)
  }

  // pomme: replaces extension SwiftUI.Slider where Label == SwiftUICore.EmptyView, ValueLabel == SwiftUICore.EmptyView :: nonisolated public init<V>(value: SwiftUICore.Binding<V>, in bounds: Swift.ClosedRange<V>, step: V.Stride = 1, onEditingChanged: @escaping (Swift.Bool) -> Swift.Void = { _ in }) where V : Swift.BinaryFloatingPoint, V.Stride : Swift.BinaryFloatingPoint
  nonisolated public init<V>(value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride = 1, onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V: BinaryFloatingPoint, V.Stride: BinaryFloatingPoint {
    self.init(pommeValue: value, in: bounds, step: step, onEditingChanged: onEditingChanged)
  }
}

extension Slider {
  nonisolated init<V>(pommeValue value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride?, onEditingChanged: @escaping (Bool) -> Void) where V: BinaryFloatingPoint, V.Stride: BinaryFloatingPoint {
    let span = Double(bounds.upperBound - bounds.lowerBound)
    pommeFraction = { span == 0 ? 0 : Double(value.wrappedValue - bounds.lowerBound) / span }
    pommeSetFraction = { fraction in
      var v = bounds.lowerBound + V(fraction * span)
      if let step, step > 0 {
        v = bounds.lowerBound + V((Double(v - bounds.lowerBound) / Double(step)).rounded() * Double(step))
      }
      value.wrappedValue = min(max(v, bounds.lowerBound), bounds.upperBound)
    }
    pommeEditingChanged = onEditingChanged
  }
}

extension Slider: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    [SliderNode(path: path, fraction: pommeFraction(), setFraction: pommeSetFraction, editingChanged: pommeEditingChanged)]
  }
}

/// A slider that reports its value and when a drag starts and ends.
final class PommeSlider: UISlider {
  var changed: ((Float) -> Void)?
  var editingChanged: ((Bool) -> Void)?

  override init(frame: CGRect) {
    super.init(frame: frame)
    addTarget(self, action: #selector(valueChanged), for: .valueChanged)
    addTarget(self, action: #selector(began), for: .touchDown)
    addTarget(self, action: #selector(ended), for: [.touchUpInside, .touchUpOutside, .touchCancel])
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) isn't supported")
  }

  @objc private func valueChanged() { changed?(value) }
  @objc private func began() { editingChanged?(true) }
  @objc private func ended() { editingChanged?(false) }
}

/// A slider: as wide as offered, 22 tall (iOS 26, measured); UIKit's slider
/// in it reaches past that.
final class SliderNode: Node {
  let fraction: Double
  let setFraction: (Double) -> Void
  let editingChanged: (Bool) -> Void
  static let height: CGFloat = 22
  private static var controlHeight: CGFloat { measuringSlider.intrinsicContentSize.height }
  private static let measuringSlider = UISlider()

  init(path: ViewPath, fraction: Double, setFraction: @escaping (Double) -> Void, editingChanged: @escaping (Bool) -> Void) {
    self.fraction = fraction
    self.setFraction = setFraction
    self.editingChanged = editingChanged
    super.init(path: path)
  }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    CGSize(width: proposal.width ?? 120, height: Self.height)
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let control: PommeSlider = renderer.view(for: path) { PommeSlider(frame: .zero) }
    control.frame = CGRect(x: frame.minX, y: frame.midY - Self.controlHeight / 2, width: frame.width, height: Self.controlHeight)
    if Double(control.value) != fraction {
      control.value = Float(fraction)
    }
    let set = setFraction
    control.changed = { set(Double($0)) }
    control.editingChanged = editingChanged
    applyAccessibility(to: control)
    parent.pommeAdopt(control)
  }
}

// ---- Button ----

// pomme: owns public struct Button<Label> : SwiftUICore.View where Label : SwiftUICore.View
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
@MainActor @preconcurrency
public struct Button<Label>: View where Label: View {
  var pommeAction: @MainActor () -> Void
  var pommeLabel: Label
  var pommeRole: ButtonRole? = nil
}

extension Button {
  // pomme: replaces public struct Button<Label> : SwiftUICore.View where Label : SwiftUICore.View :: public init(action: @escaping @_Concurrency.MainActor () -> Swift.Void, @SwiftUICore.ViewBuilder label: () -> Label)
  @MainActor @preconcurrency
  public init(action: @escaping @MainActor () -> Void, @ViewBuilder label: () -> Label) {
    pommeAction = action
    pommeLabel = label()
  }
}

extension Button where Label == Text {
  // pomme: replaces extension SwiftUI.Button where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, action: @escaping @_Concurrency.MainActor () -> Swift.Void)
  @preconcurrency
  nonisolated public init(_ titleKey: LocalizedStringKey, action: @escaping @MainActor () -> Void) {
    pommeAction = action
    pommeLabel = Text(verbatim: titleKey.pommeLocalized())
  }

  // pomme: replaces extension SwiftUI.Button where Label == SwiftUICore.Text :: nonisolated public init<S>(_ title: S, action: @escaping @_Concurrency.MainActor () -> Swift.Void) where S : Swift.StringProtocol
  @preconcurrency @_disfavoredOverload
  nonisolated public init<S>(_ title: S, action: @escaping @MainActor () -> Void) where S: StringProtocol {
    pommeAction = action
    pommeLabel = Text(title)
  }
}

extension Button {
  // pomme: replaces extension SwiftUI.Button :: nonisolated public init(role: SwiftUI.ButtonRole?, action: @escaping @_Concurrency.MainActor () -> Swift.Void, @SwiftUICore.ViewBuilder label: () -> Label)
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  @preconcurrency
  nonisolated public init(role: ButtonRole?, action: @escaping @MainActor () -> Void, @ViewBuilder label: () -> Label) {
    pommeAction = action
    pommeLabel = label()
    pommeRole = role
  }
}

extension Button where Label == Text {
  // pomme: replaces extension SwiftUI.Button where Label == SwiftUICore.Text :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, role: SwiftUI.ButtonRole?, action: @escaping @_Concurrency.MainActor () -> Swift.Void)
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  @preconcurrency
  nonisolated public init(_ titleKey: LocalizedStringKey, role: ButtonRole?, action: @escaping @MainActor () -> Void) {
    self.init(role: role, action: action) { Text(verbatim: titleKey.pommeLocalized()) }
  }

  // pomme: replaces extension SwiftUI.Button where Label == SwiftUICore.Text :: nonisolated public init<S>(_ title: S, role: SwiftUI.ButtonRole?, action: @escaping @_Concurrency.MainActor () -> Swift.Void) where S : Swift.StringProtocol
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  @preconcurrency @_disfavoredOverload
  nonisolated public init<S>(_ title: S, role: ButtonRole?, action: @escaping @MainActor () -> Void) where S: StringProtocol {
    self.init(role: role, action: action) { Text(title) }
  }
}

// pomme: in public struct ButtonRole : Swift.Equatable, Swift.Sendable
extension ButtonRole {
  /// Which role it is.
  let pommeName: String

  init(pommeName: String) {
    _pommeStorage = ()
    self.pommeName = pommeName
  }

  // pomme: replaces public struct ButtonRole : Swift.Equatable, Swift.Sendable :: public static let destructive: SwiftUI.ButtonRole
  public static let destructive = ButtonRole(pommeName: "destructive")
  // pomme: replaces public struct ButtonRole : Swift.Equatable, Swift.Sendable :: public static let cancel: SwiftUI.ButtonRole
  public static let cancel = ButtonRole(pommeName: "cancel")
  // pomme: replaces public struct ButtonRole : Swift.Equatable, Swift.Sendable :: public static let confirm: SwiftUI.ButtonRole
  @available(iOS 26.0, macOS 26.0, tvOS 26.0, watchOS 26.0, visionOS 26.0, *)
  public static let confirm = ButtonRole(pommeName: "confirm")
  // pomme: replaces public struct ButtonRole : Swift.Equatable, Swift.Sendable :: public static let close: SwiftUI.ButtonRole
  @available(iOS 26.0, macOS 26.0, tvOS 26.0, watchOS 26.0, visionOS 26.0, *)
  public static let close = ButtonRole(pommeName: "close")
  // pomme: replaces public struct ButtonRole : Swift.Equatable, Swift.Sendable :: public static func == (a: SwiftUI.ButtonRole, b: SwiftUI.ButtonRole) -> Swift.Bool
  public static func == (a: ButtonRole, b: ButtonRole) -> Bool { a.pommeName == b.pommeName }
}

extension Button: PrimitiveView {
  /// Its label, that runs the action when tapped: in its style's foreground
  /// (red for a destructive one).
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let destructive = pommeRole?.pommeName == "destructive"
    let foreground: UIColor?
    switch graph.environment.pommeButtonStyle {
    case nil, is DefaultButtonStyle, is BorderlessButtonStyle:
      foreground = destructive ? .systemRed : .tintColor
    case is PlainButtonStyle:
      foreground = destructive ? .systemRed : nil
    case let style as any PrimitiveButtonStyle where style is BorderedButtonStyle || style is BorderedProminentButtonStyle:
      return pommeResolveBordered(prominent: style is BorderedProminentButtonStyle, at: path, in: graph, preferences)
    case let style?:
      fatalError("SwiftUI in pomme doesn't implement the button style \(type(of: style)) yet")
    }
    // (An explicit `.foregroundStyle` around the button wins over its tint.)
    let label = graph.withEnvironment({ env in if env.pommeForeground == nil { foreground.map { env.pommeForeground = $0 } } }) {
      graph.resolve(pommeLabel, at: path.appending("label"), preferences)
    }
    let node = ButtonNode(path: path, label: label, action: pommeAction)
    node.enabled = graph.environment.isEnabled
    return [node]
  }

  /// `.bordered` and `.borderedProminent` (iOS 26, tests/fixtures/borderedbuttons):
  /// the label in a capsule, 12 pt in from its sides and 7 from its top and
  /// bottom (small: 10 and 5, in the subheadline font). Bordered: the
  /// secondary fill, the label in the tint; prominent: the tint, the label
  /// white; disabled, either: the secondary fill, the label the label gray at
  /// 22.5%.
  private func pommeResolveBordered(prominent: Bool, at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let env = graph.environment
    let destructive = pommeRole?.pommeName == "destructive"
    let enabled = env.isEnabled
    let small = env.controlSize == .small || env.controlSize == .mini
    let accent: UIColor = destructive ? .systemRed : .tintColor
    let disabledText = UIColor { $0.userInterfaceStyle == .dark
      ? UIColor(red: 235 / 255, green: 235 / 255, blue: 245 / 255, alpha: 0.225)
      : UIColor(red: 60 / 255, green: 60 / 255, blue: 67 / 255, alpha: 0.225) }
    let text: UIColor = !enabled ? disabledText : prominent ? .white : accent
    let fill: UIColor = !enabled ? .secondarySystemFill : prominent ? accent : .secondarySystemFill
    let label = graph.withEnvironment({ e in
      if e.pommeForeground == nil || !enabled { e.pommeForeground = text }
      if small && e.font == nil { e.font = .subheadline }
    }) {
      graph.resolve(pommeLabel, at: path.appending("label"), preferences)
    }
    let inset = small ? CGSize(width: 10, height: 5) : CGSize(width: 12, height: 7)
    let shape = label.first.map { [BorderedNode(path: path.appending("capsule"), child: $0, inset: inset, fill: fill)] } ?? []
    let node = ButtonNode(path: path, label: shape, action: pommeAction)
    node.enabled = enabled
    return [node]
  }
}

/// A control that runs a button's action when tapped: the size of its label,
/// or of its list row.
final class PommeButtonControl: UIControl {
  var action: (() -> Void)?

  override init(frame: CGRect) {
    super.init(frame: frame)
    addTarget(self, action: #selector(tapped), for: .touchUpInside)
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) isn't supported")
  }

  @objc private func tapped() { action?() }

  // A button is one accessibility element, as on iOS: a button, named by
  // the text in its label.
  override var isAccessibilityElement: Bool {
    get { true }
    set {}
  }

  override var accessibilityTraits: UIAccessibilityTraits {
    get { .button }
    set {}
  }

  /// Its label set with `.accessibilityLabel(_:)`, which names it instead of its text.
  var pommeLabel: String?

  override var accessibilityLabel: String? {
    get {
      if let pommeLabel { return pommeLabel }
      var texts: [String] = []
      func walk(_ v: UIView) {
        if let l = v as? UILabel, let t = l.text, !t.isEmpty, !l.isHidden { texts.append(t) }
        else if v !== self, let a = v.accessibilityLabel, !a.isEmpty, v.isAccessibilityElement { texts.append(a) }
        for s in v.subviews { walk(s) }
      }
      walk(self)
      return texts.isEmpty ? nil : texts.joined(separator: ", ")
    }
    set {}
  }
}

final class ButtonNode: Node {
  let label: [Node]
  let action: @MainActor () -> Void
  /// An action that needs the control tapped (a navigation link's push).
  var viewAction: (@MainActor (UIView) -> Void)?
  /// Whether it takes taps (`.disabled`).
  var enabled = true

  init(path: ViewPath, label: [Node], action: @escaping @MainActor () -> Void) {
    self.label = label
    self.action = action
    super.init(path: path)
  }

  override var rowAction: (@MainActor () -> Void)? { action }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    label.first?.sizeThatFits(proposal) ?? .zero
  }

  override func explicitGuide(_ key: AlignmentKey, size: CGSize) -> CGFloat? {
    label.first?.explicitGuide(key, size: size)
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let control: PommeButtonControl = renderer.view(for: path) { PommeButtonControl(frame: .zero) }
    control.frame = frame
    let action = action
    if let viewAction {
      control.action = { [weak control] in MainActor.assumeIsolated { if let control { viewAction(control) } } }
    } else {
      control.action = { MainActor.assumeIsolated { action() } }
    }
    control.accessibilityIdentifier = accessibilityIdentifier
    control.pommeLabel = accessibilityLabel
    control.isEnabled = enabled
    control.accessibilityElementsHidden = accessibilityHidden
    control.accessibilityTraits = .button
    parent.pommeAdopt(control)
    for node in label {
      node.place(in: CGRect(origin: .zero, size: frame.size), of: control, renderer)
    }
  }
}

// ---- Button styles ----

// The built-in styles pomme draws itself: automatic (and borderless, which it
// is on iOS outside other containers), its label in the tint color; plain,
// its label as it is.

// pomme: in public struct PlainButtonStyle : SwiftUI.PrimitiveButtonStyle
extension PlainButtonStyle {
  // pomme: replaces public struct PlainButtonStyle : SwiftUI.PrimitiveButtonStyle :: public init()
  public init() { _pommeStorage = () }
}

// pomme: in public struct BorderlessButtonStyle : SwiftUI.PrimitiveButtonStyle
extension BorderlessButtonStyle {
  // pomme: replaces public struct BorderlessButtonStyle : SwiftUI.PrimitiveButtonStyle :: public init()
  public init() { _pommeStorage = () }
}

// pomme: in public struct DefaultButtonStyle : SwiftUI.PrimitiveButtonStyle
extension DefaultButtonStyle {
  // pomme: replaces public struct DefaultButtonStyle : SwiftUI.PrimitiveButtonStyle :: public init()
  public init() { _pommeStorage = () }
}

extension PrimitiveButtonStyle where Self == PlainButtonStyle {
  // pomme: replaces extension SwiftUI.PrimitiveButtonStyle where Self == SwiftUI.PlainButtonStyle :: public static var plain: SwiftUI.PlainButtonStyle
  public static var plain: PlainButtonStyle { PlainButtonStyle() }
}

extension PrimitiveButtonStyle where Self == BorderlessButtonStyle {
  // pomme: replaces extension SwiftUI.PrimitiveButtonStyle where Self == SwiftUI.BorderlessButtonStyle :: public static var borderless: SwiftUI.BorderlessButtonStyle
  public static var borderless: BorderlessButtonStyle { BorderlessButtonStyle() }
}

extension PrimitiveButtonStyle where Self == DefaultButtonStyle {
  // pomme: replaces extension SwiftUI.PrimitiveButtonStyle where Self == SwiftUI.DefaultButtonStyle :: public static var automatic: SwiftUI.DefaultButtonStyle
  public static var automatic: DefaultButtonStyle { DefaultButtonStyle() }
}

extension EnvironmentValues {
  /// The style of the buttons within (`buttonStyle`): a PrimitiveButtonStyle
  /// or a ButtonStyle; automatic until set.
  var pommeButtonStyle: Any? {
    get { pommeByName["pommeButtonStyle"] }
    set { pommeByName["pommeButtonStyle"] = newValue }
  }
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func buttonStyle<S>(_ style: S) -> some SwiftUICore.View where S : SwiftUI.PrimitiveButtonStyle
  nonisolated public func buttonStyle<S>(_ style: S) -> some View where S: PrimitiveButtonStyle {
    environment(\.pommeButtonStyle, style)
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func buttonStyle<S>(_ style: S) -> some SwiftUICore.View where S : SwiftUI.ButtonStyle
  nonisolated public func buttonStyle<S>(_ style: S) -> some View where S: ButtonStyle {
    environment(\.pommeButtonStyle, style)
  }
}

/// A bordered button's label in its capsule.
final class BorderedNode: WrapperNode {
  let inset: CGSize
  let fill: UIColor

  init(path: ViewPath, child: Node, inset: CGSize, fill: UIColor) {
    self.inset = inset
    self.fill = fill
    super.init(path: path, child: child)
  }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    let inner = ProposedViewSize(width: proposal.width.map { max(0, $0 - 2 * inset.width) },
                                 height: proposal.height.map { max(0, $0 - 2 * inset.height) })
    let s = child.sizeThatFits(inner)
    return CGSize(width: s.width + 2 * inset.width, height: s.height + 2 * inset.height)
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let v: ContainerView = renderer.view(for: path) { ContainerView() }
    v.frame = frame
    v.backgroundColor = fill
    v.layer.cornerRadius = frame.height / 2
    v.isUserInteractionEnabled = false
    parent.pommeAdopt(v)
    child.place(in: CGRect(x: inset.width, y: inset.height, width: frame.width - 2 * inset.width, height: frame.height - 2 * inset.height), of: v, renderer)
  }
}
