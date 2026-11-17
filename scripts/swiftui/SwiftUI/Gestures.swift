// Drag gestures (DragGesture), recognized with touches on the view they're
// on: once a touch has moved `minimumDistance` (at once for 0), each move
// runs the `onChanged` actions with the drag's value, and lifting the finger
// the `onEnded` ones. Locations are in the view's own coordinates. The view
// takes touches only where what it shows is.

import UIKit

// pomme: in public struct DragGesture : SwiftUICore.Gesture
extension DragGesture {
  // pomme: replaces public struct DragGesture : SwiftUICore.Gesture :: public init(minimumDistance: CoreFoundation.CGFloat = 10, coordinateSpace: SwiftUICore.CoordinateSpace = .local)
  @_disfavoredOverload public init(minimumDistance: CGFloat = 10, coordinateSpace: CoordinateSpace = .local) {
    _pommeStorage = ()
    self.minimumDistance = minimumDistance
    self.coordinateSpace = coordinateSpace
  }

  // pomme: replaces public struct DragGesture : SwiftUICore.Gesture :: public init(minimumDistance: CoreFoundation.CGFloat = 10, coordinateSpace: some CoordinateSpaceProtocol = .local)
  public init(minimumDistance: CGFloat = 10, coordinateSpace: some CoordinateSpaceProtocol = .local) {
    _pommeStorage = ()
    self.minimumDistance = minimumDistance
    self.coordinateSpace = coordinateSpace.coordinateSpace
  }
}

// pomme: in public struct DragGesture : SwiftUICore.Gesture > public struct Value : Swift.Equatable, Swift.Sendable
extension DragGesture.Value {
  /// Where the drag would end if the finger lifted now (by its velocity).
  package var pommePredictedEnd: CGPoint

  package init(time: Date, location: CGPoint, startLocation: CGPoint, predictedEnd: CGPoint) {
    _pommeStorage = ()
    self.time = time
    self.location = location
    self.startLocation = startLocation
    pommePredictedEnd = predictedEnd
  }

  // pomme: replaces public struct DragGesture : SwiftUICore.Gesture > public struct Value : Swift.Equatable, Swift.Sendable :: public var translation: CoreFoundation.CGSize
  public var translation: CGSize { CGSize(width: location.x - startLocation.x, height: location.y - startLocation.y) }
  // pomme: replaces public struct DragGesture : SwiftUICore.Gesture > public struct Value : Swift.Equatable, Swift.Sendable :: public var predictedEndLocation: CoreFoundation.CGPoint
  public var predictedEndLocation: CGPoint { pommePredictedEnd }
  // pomme: replaces public struct DragGesture : SwiftUICore.Gesture > public struct Value : Swift.Equatable, Swift.Sendable :: public var predictedEndTranslation: CoreFoundation.CGSize
  public var predictedEndTranslation: CGSize { CGSize(width: pommePredictedEnd.x - startLocation.x, height: pommePredictedEnd.y - startLocation.y) }
  // pomme: replaces public struct DragGesture : SwiftUICore.Gesture > public struct Value : Swift.Equatable, Swift.Sendable :: public static func == (a: SwiftUI.DragGesture.Value, b: SwiftUI.DragGesture.Value) -> Swift.Bool
  public static func == (a: DragGesture.Value, b: DragGesture.Value) -> Bool {
    a.time == b.time && a.location == b.location && a.startLocation == b.startLocation && a.pommePredictedEnd == b.pommePredictedEnd
  }
}

extension PommeGestureModifier: PrimitiveModifier {
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    guard let drag = parts.base as? DragGesture else {
      fatalError("SwiftUI in pomme doesn't implement the gesture \(type(of: parts.base)) yet")
    }
    return content().enumerated().map { i, node in
      DragNode(path: path.appending(i), child: node, minimumDistance: drag.minimumDistance, changed: parts.changed, ended: parts.ended)
    }
  }
}

/// A view that recognizes a drag on what it shows.
final class DragNode: WrapperNode {
  let minimumDistance: CGFloat
  let changed: [(Any) -> Void]
  let ended: [(Any) -> Void]

  init(path: ViewPath, child: Node, minimumDistance: CGFloat, changed: [(Any) -> Void], ended: [(Any) -> Void]) {
    self.minimumDistance = minimumDistance
    self.changed = changed
    self.ended = ended
    super.init(path: path, child: child)
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let v: DragView = renderer.view(for: path) { DragView() }
    v.frame = frame
    v.minimumDistance = minimumDistance
    v.changed = changed
    v.ended = ended
    parent.pommeAdopt(v)
    child.place(in: CGRect(origin: .zero, size: frame.size), of: v, renderer)
  }
}

final class DragView: UIView {
  var minimumDistance: CGFloat = 10
  var changed: [(Any) -> Void] = []
  var ended: [(Any) -> Void] = []
  private var start: CGPoint?
  private var dragging = false
  /// The last two samples (time, location), for the velocity.
  private var samples: [(TimeInterval, CGPoint)] = []

  /// Only where a view inside is (its frame's empty space passes touches on).
  override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
    subviews.contains { !$0.isHidden && $0.isUserInteractionEnabled && $0.point(inside: convert(point, to: $0), with: event) }
  }

  private func value(at location: CGPoint) -> DragGesture.Value {
    var predicted = location
    if samples.count == 2 {
      let (t0, p0) = samples[0], (t1, p1) = samples[1]
      let dt = max(t1 - t0, 1.0 / 120)
      // velocity = 4 × (predicted − location), as SwiftUI's Value has it.
      predicted = CGPoint(x: location.x + (p1.x - p0.x) / dt / 4, y: location.y + (p1.y - p0.y) / dt / 4)
    }
    return DragGesture.Value(time: Date(), location: location, startLocation: start ?? location, predictedEnd: predicted)
  }

  private func sample(_ location: CGPoint) {
    samples.append((ProcessInfo.processInfo.systemUptime, location))
    if samples.count > 2 { samples.removeFirst() }
  }

  override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
    guard let location = touches.first?.location(in: self) else { return }
    start = location
    dragging = false
    samples = []
    sample(location)
    if minimumDistance <= 0 {
      dragging = true
      let v = value(at: location)
      for action in changed { action(v) }
    }
  }

  override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
    guard let start, let location = touches.first?.location(in: self) else { return }
    sample(location)
    if !dragging && hypot(location.x - start.x, location.y - start.y) >= minimumDistance {
      dragging = true
    }
    guard dragging else { return }
    let v = value(at: location)
    for action in changed { action(v) }
  }

  override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
    guard let location = touches.first?.location(in: self) else { return }
    sample(location)
    if dragging {
      let v = value(at: location)
      for action in ended { action(v) }
    }
    start = nil
    dragging = false
  }

  override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
    start = nil
    dragging = false
  }
}
