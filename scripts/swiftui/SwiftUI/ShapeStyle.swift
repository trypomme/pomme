// Shape styles as pomme fills with them: each is a color in its
// environment. The foreground (`foregroundStyle`) is resolved where it's set,
// against the foreground around it, so a hierarchical level of it (secondary,
// say) is of that one.

import UIKit

/// A shape style pomme knows how to fill with.
protocol PommeShapeStyle {
  func pommeColor(in environment: EnvironmentValues) -> UIColor
}

extension ShapeStyle {
  /// The color this style fills with, in an environment.
  func pommeFill(in environment: EnvironmentValues) -> UIColor {
    guard let style = self as? any PommeShapeStyle else {
      fatalError("SwiftUI in pomme doesn't implement the shape style \(Self.self) yet")
    }
    return style.pommeColor(in: environment)
  }
}

extension EnvironmentValues {
  /// The foreground's color, once a view sets it (else text's own, by where it is).
  var pommeForeground: UIColor? {
    get { pommeByName["pommeForeground"] as? UIColor }
    set { pommeByName["pommeForeground"] = newValue }
  }
}

extension Color: PommeShapeStyle {
  func pommeColor(in environment: EnvironmentValues) -> UIColor { UIColor(self) }
}

extension HierarchicalShapeStyle: PommeShapeStyle {
  /// The system's label colors; of a foreground set, that color at the
  /// opacities the label colors have (iOS 26: 1, 0.6, 0.3, 0.18 in light mode).
  func pommeColor(in environment: EnvironmentValues) -> UIColor {
    let labels: [UIColor] = [.label, .secondaryLabel, .tertiaryLabel, .quaternaryLabel]
    guard Int(id) < labels.count else {
      fatalError("SwiftUI in pomme doesn't implement the hierarchical level \(id) yet")
    }
    guard let foreground = environment.pommeForeground else { return labels[Int(id)] }
    return UIColor { traits in
      let alpha = labels[Int(self.id)].resolvedColor(with: traits).cgColor.alpha
      let base = foreground.resolvedColor(with: traits)
      return base.withAlphaComponent(base.cgColor.alpha * alpha)
    }
  }
}

extension _ForegroundStyleModifier: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let color = style.pommeFill(in: graph.environment)
    return graph.withEnvironment({ $0.pommeForeground = color }, content)
  }
}

extension AnyShapeStyle: PommeShapeStyle {
  func pommeColor(in environment: EnvironmentValues) -> UIColor {
    let base = pommeBase
    guard !(base is AnyShapeStyle) else {
      fatalError("SwiftUI in pomme doesn't implement this AnyShapeStyle's style yet")
    }
    return base.pommeFill(in: environment)
  }
}

// ---- Materials ----

/// A material, as a translucent color: the one that gives what iOS 26 draws
/// over white and over black (tests/fixtures/materials, light and dark). Over
/// a plain background it's the same; pomme doesn't blur what's behind.
extension Material: PommeShapeStyle {
  /// (Over white, over black) gray levels, 0–255, in light and dark appearance.
  private static let measured: [String: (light: (white: CGFloat, black: CGFloat), dark: (white: CGFloat, black: CGFloat))] = [
    "ultraThin": ((245, 88), (177, 31)),
    "thin": ((245, 142), (125, 31)),
    "regular": ((245, 197), (83, 31)),
    "thick": ((245, 233), (37, 31)),
    "ultraThick": ((246, 233), (37, 31)),
    "bar": ((247, 178), (87, 18)),
  ]

  func pommeColor(in environment: EnvironmentValues) -> UIColor {
    guard let m = Self.measured[pommeName] else {
      fatalError("SwiftUI in pomme doesn't implement the material \(pommeName) yet")
    }
    // Over white w = a·c + (1 − a), over black b = a·c: a = 1 − (w − b), c = b / a.
    func color(_ w: CGFloat, _ b: CGFloat) -> UIColor {
      let a = 1 - (w - b) / 255
      return UIColor(white: a > 0 ? b / 255 / a : 0, alpha: a)
    }
    return UIColor { $0.userInterfaceStyle == .dark ? color(m.dark.white, m.dark.black) : color(m.light.white, m.light.black) }
  }
}
