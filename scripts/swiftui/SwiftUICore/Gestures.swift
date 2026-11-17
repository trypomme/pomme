// Gestures on views: `.gesture(_:)` with a gesture and the actions chained
// on it (`onChanged`, `onEnded`). SwiftUI (pomme's, in the SwiftUI module)
// recognizes the base gesture (a drag) with touches on the view, and runs
// the actions with its values.

import Foundation

/// A gesture as pomme runs it: its base gesture (DragGesture …) and the
/// actions chained on it, outermost last.
package struct PommeGestureParts {
  package var base: Any
  package var changed: [(Any) -> Void] = []
  package var ended: [(Any) -> Void] = []
  package init(base: Any) { self.base = base }
}

package protocol PommeGestureChain {
  func pommeParts() -> PommeGestureParts
}

package func pommeGestureParts(_ gesture: Any) -> PommeGestureParts {
  (gesture as? any PommeGestureChain)?.pommeParts() ?? PommeGestureParts(base: gesture)
}

// pomme: in public struct _EndedGesture<Content> where Content : SwiftUICore.Gesture
extension _EndedGesture {
  package var pommeContent: Content
  package var pommeAction: (Content.Value) -> Void

  package init(pommeContent: Content, pommeAction: @escaping (Content.Value) -> Void) {
    _pommeStorage = ()
    self.pommeContent = pommeContent
    self.pommeAction = pommeAction
  }
}

extension _EndedGesture: PommeGestureChain {
  package func pommeParts() -> PommeGestureParts {
    var parts = pommeGestureParts(pommeContent)
    let action = pommeAction
    parts.ended.append { if let v = $0 as? Content.Value { action(v) } }
    return parts
  }
}

// pomme: in public struct _ChangedGesture<Content> where Content : SwiftUICore.Gesture, Content.Value : Swift.Equatable
extension _ChangedGesture {
  package var pommeContent: Content
  package var pommeAction: (Content.Value) -> Void

  package init(pommeContent: Content, pommeAction: @escaping (Content.Value) -> Void) {
    _pommeStorage = ()
    self.pommeContent = pommeContent
    self.pommeAction = pommeAction
  }
}

extension _ChangedGesture: PommeGestureChain {
  package func pommeParts() -> PommeGestureParts {
    var parts = pommeGestureParts(pommeContent)
    let action = pommeAction
    parts.changed.append { if let v = $0 as? Content.Value { action(v) } }
    return parts
  }
}

// pomme: in extension SwiftUICore.Gesture
extension Gesture {
  // pomme: replaces extension SwiftUICore.Gesture :: nonisolated public func onEnded(_ action: @escaping (Self.Value) -> Swift.Void) -> SwiftUICore._EndedGesture<Self>
  nonisolated public func onEnded(_ action: @escaping (Self.Value) -> Void) -> _EndedGesture<Self> {
    _EndedGesture(pommeContent: self, pommeAction: action)
  }
}

// pomme: in extension SwiftUICore.Gesture where Self.Value : Swift.Equatable
extension Gesture where Value: Equatable {
  // pomme: replaces extension SwiftUICore.Gesture where Self.Value : Swift.Equatable :: public func onChanged(_ action: @escaping (Self.Value) -> Swift.Void) -> SwiftUICore._ChangedGesture<Self>
  public func onChanged(_ action: @escaping (Self.Value) -> Void) -> _ChangedGesture<Self> {
    _ChangedGesture(pommeContent: self, pommeAction: action)
  }
}

/// `.gesture(_:)`: the view, recognizing the gesture (SwiftUI resolves it).
package struct PommeGestureModifier: ViewModifier {
  package var parts: PommeGestureParts
  package typealias Body = Never
  package func body(content: Content) -> Never { fatalError("pomme resolves gestures itself") }
}

// pomme: in extension SwiftUICore.View
extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func gesture<T>(_ gesture: T, including mask: SwiftUICore.GestureMask = .all) -> some SwiftUICore.View where T : SwiftUICore.Gesture
  nonisolated public func gesture<T>(_ gesture: T, including mask: GestureMask = .all) -> some View where T: Gesture {
    modifier(PommeGestureModifier(parts: pommeGestureParts(gesture)))
  }
}
