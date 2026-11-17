// Effects on how a view is drawn, not where it's laid out: scale, rotation,
// offset, opacity, shadow (`scaleEffect`, `rotationEffect`, `offset`,
// `opacity`, `shadow`), and whether it takes touches (`allowsHitTesting`).
// Each puts the view's content in a view of its own, which the effect is
// applied to (a transform about the anchor, its alpha, its layer's shadow).
// Also here: preferences (`preference(key:value:)`, `onPreferenceChange`),
// `position`, and a background shape (`background(_:in:)`).

import UIKit

/// A view drawn with an effect: its content in a view the effect is applied to.
final class EffectNode: WrapperNode {
  var transform: CGAffineTransform = .identity
  var anchor: UnitPoint = .center
  var alpha: CGFloat = 1
  var shadow: (color: UIColor, radius: CGFloat, offset: CGSize)?
  var hitTesting = true

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let v: ContainerView = renderer.view(for: path) { ContainerView() }
    v.transform = .identity
    v.frame = frame
    parent.pommeAdopt(v)
    child.place(in: CGRect(origin: .zero, size: frame.size), of: v, renderer)
    // The transform about the anchor (UIKit's is about the view's center).
    let ax = (anchor.x - 0.5) * frame.width, ay = (anchor.y - 0.5) * frame.height
    v.transform = transform.isIdentity ? .identity
      : CGAffineTransform(translationX: ax, y: ay).concatenating(transform).concatenating(CGAffineTransform(translationX: -ax, y: -ay))
    v.alpha = alpha
    v.isUserInteractionEnabled = hitTesting
    if let shadow {
      v.layer.shadowColor = shadow.color.cgColor
      v.layer.shadowOpacity = 1
      v.layer.shadowRadius = shadow.radius
      v.layer.shadowOffset = shadow.offset
    } else if v.layer.shadowOpacity != 0 {
      v.layer.shadowOpacity = 0
    }
  }
}

/// Wrap each of the content's nodes in an effect node, set up by `change`.
@MainActor func pommeEffect(_ nodes: [Node], at path: ViewPath, _ change: (EffectNode) -> Void) -> [Node] {
  nodes.enumerated().map { i, node in
    let e = EffectNode(path: path.appending(i), child: node)
    e.accessibilityIdentifier = node.accessibilityIdentifier
    change(e)
    return e
  }
}

extension _ScaleEffect: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    pommeEffect(content(), at: path) {
      $0.transform = CGAffineTransform(scaleX: scale.width, y: scale.height)
      $0.anchor = anchor
    }
  }
}

extension _RotationEffect: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    pommeEffect(content(), at: path) {
      $0.transform = CGAffineTransform(rotationAngle: angle.radians)
      $0.anchor = anchor
    }
  }
}

extension _OffsetEffect: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    pommeEffect(content(), at: path) { $0.transform = CGAffineTransform(translationX: offset.width, y: offset.height) }
  }
}

extension _OpacityEffect: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    pommeEffect(content(), at: path) { $0.alpha = opacity }
  }
}

extension _ShadowEffect: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let color = UIColor(self.color)
    return pommeEffect(content(), at: path) { $0.shadow = (color, radius, offset) }
  }
}

extension _AllowsHitTestingModifier: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    allowsHitTesting ? content() : pommeEffect(content(), at: path) { $0.hitTesting = false }
  }
}

extension _SaturationEffect: PrimitiveModifier {
  /// (Drawn at full saturation: pomme has no color filters yet.)
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    content()
  }
}

// ---- Modifiers with nothing to draw ----

extension _AnimationModifier: PrimitiveModifier {
  /// (Changes show at once: pomme's SwiftUI doesn't animate yet.)
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    content()
  }
}

extension _ContentShapeModifier: PrimitiveModifier {
  /// (The area that takes touches: a view's frame, in pomme.)
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    content()
  }
}

// ---- .position ----

/// A view placed with its center at a point of the frame it's offered, which it takes all of.
final class PositionNode: WrapperNode {
  let position: CGPoint

  init(path: ViewPath, child: Node, position: CGPoint) {
    self.position = position
    super.init(path: path, child: child)
  }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    proposal.replacingUnspecifiedDimensions()
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let size = child.sizeThatFits(ProposedViewSize(frame.size))
    child.place(in: CGRect(x: frame.minX + position.x - size.width / 2, y: frame.minY + position.y - size.height / 2,
                           width: size.width, height: size.height), of: parent, renderer)
  }
}

extension _PositionLayout: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    content().enumerated().map { i, node in PositionNode(path: path.appending(i), child: node, position: position) }
  }
}

// ---- .background(_:in:) ----

extension _InsettableBackgroundShapeModifier: PrimitiveModifier {
  /// The shape, filled with the style, behind the content.
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    content().enumerated().map { i, node in
      let behind = graph.resolve(shape.fill(style, style: fillStyle), at: path.appending("background").appending(i), preferences)
      return BackgroundNode(path: path.appending(i), child: node, background: behind, alignment: .center)
    }
  }
}

// ---- Preferences ----

extension _PreferenceWritingModifier: PrimitiveModifier {
  /// Its value, combined with the others its key has in the tree.
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let nodes = content()
    let id = ObjectIdentifier(Key.self)
    var combined = preferences.values[id] as? Key.Value ?? Key.defaultValue
    let value = value
    Key.reduce(value: &combined) { value }
    preferences.values[id] = combined
    return nodes
  }
}

extension _PreferenceActionModifier: PrimitiveModifier {
  /// The action, after an update in which its content's value for the key changed.
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let id = ObjectIdentifier(Key.self)
    let outer = preferences.values[id]
    preferences.values[id] = nil
    let nodes = content()
    let inner = preferences.values[id] as? Key.Value ?? Key.defaultValue
    // The content's value goes on up, combined with what was there.
    if var combined = outer as? Key.Value {
      Key.reduce(value: &combined) { inner }
      preferences.values[id] = combined
    } else {
      preferences.values[id] = inner
    }
    let (_, first) = graph.followedValue(at: path, as: Key.Value.self)
    if graph.valueChanged(at: path, to: inner) || first {
      let action = action
      graph.runAfterUpdate { action(inner) }
    }
    return nodes
  }
}

// ---- GeometryReader ----

/// What a GeometryReader was last laid out with: its size, its frame in the
/// window, and its safe-area insets.
struct GeometryInfo: Equatable {
  var size: CGSize = .zero
  var global: CGRect = .zero
  var safe: EdgeInsets = EdgeInsets()

  static func == (a: GeometryInfo, b: GeometryInfo) -> Bool {
    func close(_ x: CGFloat, _ y: CGFloat) -> Bool { abs(x - y) < 0.01 }
    return close(a.size.width, b.size.width) && close(a.size.height, b.size.height)
      && close(a.global.minX, b.global.minX) && close(a.global.minY, b.global.minY)
      && close(a.safe.top, b.safe.top) && close(a.safe.bottom, b.safe.bottom)
      && close(a.safe.leading, b.safe.leading) && close(a.safe.trailing, b.safe.trailing)
  }

  var proxy: GeometryProxy {
    let info = self
    return GeometryProxy(size: size, safeArea: safe) { space in
      switch space {
      case .local: return CGRect(origin: .zero, size: info.size)
      case .global, .named: return info.global
      }
    }
  }
}

extension GeometryReader: PrimitiveView {
  /// Its content, for the geometry it was last laid out with (none, the
  /// first time); once placed, if that changed, the tree updates for it.
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let known = graph.readerGeometry[path] ?? GeometryInfo()
    let children = graph.resolve(content(known.proxy), at: path.appending("content"), preferences)
    return [GeometryReaderNode(path: path, children: children, graph: graph, used: known)]
  }
}

/// A GeometryReader: it takes all it's offered, and its content is at its top leading corner.
final class GeometryReaderNode: Node {
  let children: [Node]
  weak var graph: ViewGraph?
  let used: GeometryInfo

  init(path: ViewPath, children: [Node], graph: ViewGraph, used: GeometryInfo) {
    self.children = children
    self.graph = graph
    self.used = used
    super.init(path: path)
  }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    proposal.replacingUnspecifiedDimensions()
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let container: ContainerView = renderer.view(for: path) { ContainerView() }
    container.frame = frame
    parent.pommeAdopt(container)
    for child in children {
      let size = child.sizeThatFits(ProposedViewSize(frame.size))
      child.place(in: CGRect(origin: .zero, size: size), of: container, renderer)
    }
    let insets = renderer.extendingUnderSafeArea(frame, in: parent).insets
    let now = GeometryInfo(size: frame.size, global: parent.convert(frame, to: nil),
                           safe: EdgeInsets(top: insets.top, leading: insets.left, bottom: insets.bottom, trailing: insets.right))
    if now != used, let graph {
      graph.readerGeometry[path] = now
      graph.setNeedsUpdate()
    }
  }
}
