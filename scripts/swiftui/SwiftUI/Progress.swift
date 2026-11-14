// ProgressView: with a value, iOS's linear bar (a 4-pt capsule track in the
// system fill, filled in the tint to the fraction done); without one, the
// spinning activity indicator (20 pt), at the leading edge. Its label, if any,
// above. (iOS 26, measured on the simulator.)

import UIKit

// pomme: owns public struct ProgressView<Label, CurrentValueLabel> : SwiftUICore.View where Label : SwiftUICore.View, CurrentValueLabel : SwiftUICore.View
@available(iOS 14.0, macOS 11.0, tvOS 14.0, watchOS 7.0, *)
@MainActor @preconcurrency
public struct ProgressView<Label, CurrentValueLabel>: View where Label: View, CurrentValueLabel: View {
  /// The fraction done (nil: indeterminate).
  var pommeFraction: Double?
  var pommeLabel: Label?
}

extension ProgressView where CurrentValueLabel == EmptyView {
  // pomme: replaces extension SwiftUI.ProgressView where CurrentValueLabel == SwiftUICore.EmptyView :: nonisolated public init() where Label == SwiftUICore.EmptyView
  nonisolated public init() where Label == EmptyView {
    self.init(pommeFraction: nil, pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.ProgressView where CurrentValueLabel == SwiftUICore.EmptyView :: nonisolated public init(@SwiftUICore.ViewBuilder label: () -> Label)
  nonisolated public init(@ViewBuilder label: () -> Label) {
    self.init(pommeFraction: nil, pommeLabel: label())
  }

  // pomme: replaces extension SwiftUI.ProgressView where CurrentValueLabel == SwiftUICore.EmptyView :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey) where Label == SwiftUICore.Text
  nonisolated public init(_ titleKey: LocalizedStringKey) where Label == Text {
    self.init(pommeFraction: nil, pommeLabel: Text(titleKey))
  }
}

extension ProgressView {
  /// A value of a total, as a fraction (0…1; nil stays indeterminate).
  static func pommeFraction<V: BinaryFloatingPoint>(_ value: V?, _ total: V) -> Double? {
    guard let value else { return nil }
    return total > 0 ? min(max(Double(value) / Double(total), 0), 1) : 0
  }

  // pomme: replaces extension SwiftUI.ProgressView :: nonisolated public init<V>(value: V?, total: V = 1.0) where Label == SwiftUICore.EmptyView, CurrentValueLabel == SwiftUICore.EmptyView, V : Swift.BinaryFloatingPoint
  nonisolated public init<V>(value: V?, total: V = 1.0) where Label == EmptyView, CurrentValueLabel == EmptyView, V: BinaryFloatingPoint {
    self.init(pommeFraction: Self.pommeFraction(value, total), pommeLabel: nil)
  }

  // pomme: replaces extension SwiftUI.ProgressView :: nonisolated public init<V>(value: V?, total: V = 1.0, @SwiftUICore.ViewBuilder label: () -> Label) where CurrentValueLabel == SwiftUICore.EmptyView, V : Swift.BinaryFloatingPoint
  nonisolated public init<V>(value: V?, total: V = 1.0, @ViewBuilder label: () -> Label) where CurrentValueLabel == EmptyView, V: BinaryFloatingPoint {
    self.init(pommeFraction: Self.pommeFraction(value, total), pommeLabel: label())
  }

  // pomme: replaces extension SwiftUI.ProgressView :: nonisolated public init<V>(_ titleKey: SwiftUICore.LocalizedStringKey, value: V?, total: V = 1.0) where Label == SwiftUICore.Text, CurrentValueLabel == SwiftUICore.EmptyView, V : Swift.BinaryFloatingPoint
  nonisolated public init<V>(_ titleKey: LocalizedStringKey, value: V?, total: V = 1.0) where Label == Text, CurrentValueLabel == EmptyView, V: BinaryFloatingPoint {
    self.init(pommeFraction: Self.pommeFraction(value, total), pommeLabel: Text(titleKey))
  }

  // pomme: replaces extension SwiftUI.ProgressView :: nonisolated public init<S, V>(_ title: S, value: V?, total: V = 1.0) where Label == SwiftUICore.Text, CurrentValueLabel == SwiftUICore.EmptyView, S : Swift.StringProtocol, V : Swift.BinaryFloatingPoint
  nonisolated public init<S, V>(_ title: S, value: V?, total: V = 1.0) where Label == Text, CurrentValueLabel == EmptyView, S: StringProtocol, V: BinaryFloatingPoint {
    self.init(pommeFraction: Self.pommeFraction(value, total), pommeLabel: Text(title))
  }
}

extension ProgressView: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let label = pommeLabel.map { graph.resolve($0, at: path.appending("label"), preferences) } ?? []
    return [ProgressNode(path: path, fraction: pommeFraction, label: label.first)]
  }
}

final class ProgressNode: Node {
  let fraction: Double?
  let label: Node?
  static let barHeight: CGFloat = 4
  static let spinnerSize: CGFloat = 20
  /// Between the label and the bar.
  static let spacing: CGFloat = 4

  init(path: ViewPath, fraction: Double?, label: Node?) {
    self.fraction = fraction
    self.label = label
    super.init(path: path)
  }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    let l = label?.sizeThatFits(ProposedViewSize(width: proposal.width, height: nil))
    guard fraction != nil else {
      // A spinner (and its label below it, centered).
      let w = max(Self.spinnerSize, l?.width ?? 0)
      return CGSize(width: w, height: Self.spinnerSize + (l.map { $0.height + Self.spacing } ?? 0))
    }
    // A bar: as wide as offered.
    return CGSize(width: proposal.width ?? 10, height: Self.barHeight + (l.map { $0.height + Self.spacing } ?? 0))
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    var y = frame.minY
    if let label, fraction != nil {
      let s = label.sizeThatFits(ProposedViewSize(width: frame.width, height: nil))
      label.place(in: CGRect(x: frame.minX, y: y, width: s.width, height: s.height), of: parent, renderer)
      y += s.height + Self.spacing
    }
    guard let fraction else {
      let spinner: UIActivityIndicatorView = renderer.view(for: path.appending("spinner")) {
        let s = UIActivityIndicatorView(style: .medium)
        s.startAnimating()
        return s
      }
      spinner.frame = CGRect(x: frame.minX, y: frame.minY, width: Self.spinnerSize, height: Self.spinnerSize)
      parent.pommeAdopt(spinner)
      if let label {
        let s = label.sizeThatFits(.unspecified)
        label.place(in: CGRect(x: frame.minX + (Self.spinnerSize - s.width) / 2, y: frame.minY + Self.spinnerSize + Self.spacing, width: s.width, height: s.height), of: parent, renderer)
      }
      return
    }
    let track = renderer.view(for: path.appending("track")) {
      let v = UIView()
      v.backgroundColor = .systemFill
      v.layer.cornerRadius = Self.barHeight / 2
      v.clipsToBounds = true
      return v
    }
    track.frame = CGRect(x: frame.minX, y: y, width: frame.width, height: Self.barHeight)
    parent.pommeAdopt(track)
    let fill = renderer.view(for: path.appending("fill")) {
      let v = UIView()
      v.backgroundColor = .tintColor
      v.layer.cornerRadius = Self.barHeight / 2
      return v
    }
    fill.frame = CGRect(x: 0, y: 0, width: frame.width * fraction, height: Self.barHeight)
    track.pommeAdopt(fill)
  }
}
