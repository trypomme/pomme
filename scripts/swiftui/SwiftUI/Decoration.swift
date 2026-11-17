// What a view is drawn with besides itself: padding around it, a background
// behind it (a view, or a shape style filling its frame), and a shape it's
// clipped to.

import UIKit

extension _PaddingLayout: LayoutModifier {
  var pommeLayout: any Layout { pommePadding }
}

/// A node standing for another, which it lays out as its own: the content of
/// a background, a clip.
class WrapperNode: Node {
  let child: Node

  init(path: ViewPath, child: Node) {
    self.child = child
    super.init(path: path)
    priority = child.priority
    layoutValues = child.layoutValues
    tag = child.tag
    tabItem = child.tabItem
    focus = child.focus
  }

  override var rowAction: (@MainActor () -> Void)? { child.rowAction }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { child.sizeThatFits(proposal) }

  override func explicitGuide(_ key: AlignmentKey, size: CGSize) -> CGFloat? { child.explicitGuide(key, size: size) }

  /// (Its content is placed at its own origin.)
  override var firstTextLeading: CGFloat? { child.firstTextLeading }
}

// ---- Fills ----

/// A shape style filling the frame it's given (a Color as a view, a
/// background style); it takes whatever it's offered.
final class FillNode: Node {
  let color: UIColor
  /// The edges at which it reaches on under the safe area, where it meets it.
  var underSafeArea: Edge.Set = []

  init(path: ViewPath, color: UIColor) {
    self.color = color
    super.init(path: path)
  }

  /// (Where it's offered nothing in particular: SwiftUI's 10 pt.)
  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    proposal.replacingUnspecifiedDimensions()
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let v: UIView = renderer.view(for: path) { UIView() }
    var frame = frame
    if !underSafeArea.isEmpty {
      let insets = renderer.extendingUnderSafeArea(frame, in: parent).insets
      frame = CGRect(
        x: frame.minX - (underSafeArea.contains(.leading) ? insets.left : 0),
        y: frame.minY - (underSafeArea.contains(.top) ? insets.top : 0),
        width: frame.width + (underSafeArea.contains(.leading) ? insets.left : 0) + (underSafeArea.contains(.trailing) ? insets.right : 0),
        height: frame.height + (underSafeArea.contains(.top) ? insets.top : 0) + (underSafeArea.contains(.bottom) ? insets.bottom : 0))
    }
    v.frame = frame
    v.backgroundColor = color
    applyAccessibility(to: v)
    parent.pommeAdopt(v)
  }
}

extension Color: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    [FillNode(path: path, color: UIColor(self))]
  }
}

// ---- Backgrounds ----

/// A view with others behind it (a background) or in front (an overlay),
/// laid out in its frame.
final class BackgroundNode: WrapperNode {
  let background: [Node]
  let alignment: Alignment
  let inFront: Bool

  init(path: ViewPath, child: Node, background: [Node], alignment: Alignment, inFront: Bool = false) {
    self.background = background
    self.alignment = alignment
    self.inFront = inFront
    super.init(path: path, child: child)
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let container: ContainerView = renderer.view(for: path) { ContainerView() }
    container.frame = frame
    parent.pommeAdopt(container)
    let bounds = CGRect(origin: .zero, size: frame.size)
    let box = ViewDimensions(size: frame.size)
    // (Views placed later are in front.)
    if inFront {
      child.place(in: bounds, of: container, renderer)
    }
    for node in background {
      let d = node.pommeDimensions(in: ProposedViewSize(frame.size))
      let origin = CGPoint(x: box[alignment.horizontal] - d[alignment.horizontal], y: box[alignment.vertical] - d[alignment.vertical])
      node.place(in: CGRect(origin: origin, size: CGSize(width: d.width, height: d.height)), of: container, renderer)
    }
    if !inFront {
      child.place(in: bounds, of: container, renderer)
    }
  }
}

extension _BackgroundStyleModifier: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let color = style.pommeFill(in: graph.environment)
    return content().enumerated().map { i, node in
      let fill = FillNode(path: path.appending("background").appending(i), color: color)
      fill.underSafeArea = ignoresSafeAreaEdges
      return BackgroundNode(path: path.appending(i), child: node, background: [fill], alignment: .center)
    }
  }
}

extension _BackgroundModifier: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    content().enumerated().map { i, node in
      let behind = graph.resolve(background, at: path.appending("background").appending(i), preferences)
      return BackgroundNode(path: path.appending(i), child: node, background: behind, alignment: alignment)
    }
  }
}

extension _OverlayModifier: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    content().enumerated().map { i, node in
      let front = graph.resolve(overlay, at: path.appending("overlay").appending(i), preferences)
      return BackgroundNode(path: path.appending(i), child: node, background: front, alignment: alignment, inFront: true)
    }
  }
}

// ---- Clipping ----

/// A shape a layer's corners can be: a rectangle in the frame (or within it),
/// its corners rounded.
protocol PommeCornerShape {
  /// The rectangle in `bounds`, its corner radius, and whether its corners are continuous.
  func pommeCorners(in bounds: CGRect) -> (rect: CGRect, radius: CGFloat, continuous: Bool)
}

extension Rectangle: PommeCornerShape {
  func pommeCorners(in bounds: CGRect) -> (rect: CGRect, radius: CGFloat, continuous: Bool) { (bounds, 0, false) }
}

extension RoundedRectangle: PommeCornerShape {
  func pommeCorners(in bounds: CGRect) -> (rect: CGRect, radius: CGFloat, continuous: Bool) {
    let r = min(cornerSize.width, cornerSize.height, bounds.width / 2, bounds.height / 2)
    return (bounds, r, style == .continuous)
  }
}

extension Capsule: PommeCornerShape {
  func pommeCorners(in bounds: CGRect) -> (rect: CGRect, radius: CGFloat, continuous: Bool) {
    (bounds, min(bounds.width, bounds.height) / 2, style == .continuous)
  }
}

extension Circle: PommeCornerShape {
  /// The circle in the middle of the frame, as wide as its shorter side.
  func pommeCorners(in bounds: CGRect) -> (rect: CGRect, radius: CGFloat, continuous: Bool) {
    let d = min(bounds.width, bounds.height)
    return (CGRect(x: bounds.midX - d / 2, y: bounds.midY - d / 2, width: d, height: d), d / 2, false)
  }
}

/// A view shown only within a shape.
final class ClipNode: WrapperNode {
  let shape: any PommeCornerShape

  init(path: ViewPath, child: Node, shape: any PommeCornerShape) {
    self.shape = shape
    super.init(path: path, child: child)
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let clip = shape.pommeCorners(in: frame)
    let v: ContainerView = renderer.view(for: path) { ContainerView() }
    v.frame = clip.rect
    v.clipsToBounds = true
    v.layer.cornerRadius = clip.radius
    v.layer.cornerCurve = clip.continuous ? .continuous : .circular
    parent.pommeAdopt(v)
    child.place(in: CGRect(x: frame.minX - clip.rect.minX, y: frame.minY - clip.rect.minY, width: frame.width, height: frame.height), of: v, renderer)
  }
}

extension _ClipEffect: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    guard let corners = shape as? any PommeCornerShape else {
      fatalError("SwiftUI in pomme doesn't implement clipping to \(ClipShape.self) yet")
    }
    return content().enumerated().map { i, node in ClipNode(path: path.appending(i), child: node, shape: corners) }
  }
}

// ---- .ignoresSafeArea ----

/// Content that reaches on under the safe area at some edges, where its
/// frame meets it (`.ignoresSafeArea`: a background colour under the status
/// bar and the home indicator).
final class SafeAreaIgnoringNode: WrapperNode {
  let edges: Edge.Set

  init(path: ViewPath, child: Node, edges: Edge.Set) {
    self.edges = edges
    super.init(path: path, child: child)
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let insets = renderer.extendingUnderSafeArea(frame, in: parent).insets
    let grown = CGRect(
      x: frame.minX - (edges.contains(.leading) ? insets.left : 0),
      y: frame.minY - (edges.contains(.top) ? insets.top : 0),
      width: frame.width + (edges.contains(.leading) ? insets.left : 0) + (edges.contains(.trailing) ? insets.right : 0),
      height: frame.height + (edges.contains(.top) ? insets.top : 0) + (edges.contains(.bottom) ? insets.bottom : 0))
    child.place(in: grown, of: parent, renderer)
  }
}

extension _SafeAreaRegionsIgnoringLayout: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    content().enumerated().map { i, node in
      guard regions.contains(.container) else { return node }
      if let fill = node as? FillNode {
        fill.underSafeArea = edges
        return fill
      }
      return SafeAreaIgnoringNode(path: path.appending(i), child: node, edges: edges)
    }
  }
}

extension _SafeAreaIgnoringLayout: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    _SafeAreaRegionsIgnoringLayout(regions: .container, edges: edges).pommeResolve(content: content, at: path, in: graph, preferences)
  }
}

extension _AspectRatioLayout: LayoutModifier {
  var pommeLayout: any Layout { pommeAspectRatio }
}
