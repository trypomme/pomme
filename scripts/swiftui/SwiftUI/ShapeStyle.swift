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
