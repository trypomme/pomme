// Shapes as views: a rectangle, rounded rectangle, capsule or circle filled
// with a style (`Circle().fill(.green)`) or stroked (`.stroke(color,
// lineWidth:)`, centered on its outline; `.strokeBorder`, inside it), drawn
// as a view with that background or border and its corners rounded. A shape
// takes all the space it's offered.

import UIKit

/// The corners of a shape pomme draws, and how far it's inset (`.inset(by:)`,
/// `strokeBorder`'s inset shape).
@MainActor func pommeCornerGeometry(_ shape: Any) -> (corner: any PommeCornerShape, inset: CGFloat)? {
  if let c = shape as? any PommeCornerShape { return (c, 0) }
  // An inset shape (SwiftUICore's internal `X._Inset`): its base and amount.
  let name = String(reflecting: type(of: shape))
  let mirror = Mirror(reflecting: shape)
  guard name.hasSuffix("._Inset"), let amount = mirror.descendant("amount") as? CGFloat else { return nil }
  if let base = mirror.descendant("base"), let g = pommeCornerGeometry(base) {
    return (g.corner, g.inset + amount)
  }
  if name.contains(".Circle.") { return (Circle(), amount) }
  if name.contains(".Capsule.") { return (Capsule(), amount) }
  if name.contains(".Rectangle.") { return (Rectangle(), amount) }
  return nil
}

final class ShapeNode: Node {
  let corner: any PommeCornerShape
  let inset: CGFloat
  let fill: UIColor?
  let stroke: (color: UIColor, width: CGFloat)?

  init(path: ViewPath, corner: any PommeCornerShape, inset: CGFloat, fill: UIColor?, stroke: (color: UIColor, width: CGFloat)?) {
    self.corner = corner
    self.inset = inset
    self.fill = fill
    self.stroke = stroke
    super.init(path: path)
  }

  /// All it's offered (with nothing offered, SwiftUI's 10 × 10).
  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    proposal.replacingUnspecifiedDimensions()
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let v: UIView = renderer.view(for: path) { UIView() }
    var (rect, radius, continuous) = corner.pommeCorners(in: frame.insetBy(dx: inset, dy: inset))
    if let stroke {
      // Centered on the outline: half of it outside.
      rect = rect.insetBy(dx: -stroke.width / 2, dy: -stroke.width / 2)
      if radius > 0 { radius += stroke.width / 2 }
      v.layer.borderWidth = stroke.width
      v.layer.borderColor = stroke.color.cgColor
      v.backgroundColor = nil
    } else {
      v.layer.borderWidth = 0
      v.backgroundColor = fill
    }
    v.frame = rect
    v.layer.cornerRadius = radius
    v.layer.cornerCurve = continuous ? .continuous : .circular
    v.isUserInteractionEnabled = false
    applyAccessibility(to: v)
    parent.pommeAdopt(v)
  }
}

extension _ShapeView: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let color = style.pommeFill(in: graph.environment)
    // A stroked shape: its outline, as wide as its stroke style's line.
    let mirror = Mirror(reflecting: shape)
    if String(reflecting: Content.self).contains("_StrokedShape"),
       let inner = mirror.descendant("shape"), let strokeStyle = mirror.descendant("style") as? StrokeStyle {
      guard let g = pommeCornerGeometry(inner) else {
        fatalError("SwiftUI in pomme doesn't implement stroking the shape \(type(of: inner)) yet")
      }
      return [ShapeNode(path: path, corner: g.corner, inset: g.inset, fill: nil, stroke: (color, strokeStyle.lineWidth))]
    }
    guard let g = pommeCornerGeometry(shape) else {
      fatalError("SwiftUI in pomme doesn't implement the shape \(Content.self) yet")
    }
    return [ShapeNode(path: path, corner: g.corner, inset: g.inset, fill: color, stroke: nil)]
  }
}
