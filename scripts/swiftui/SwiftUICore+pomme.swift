// pomme's additions to the generated SwiftUICore module: what Apple's
// binary has but its interface doesn't show (defaults that let primitive
// types conform), and placeholder types for opaque results that pomme
// doesn't implement yet (never returned: the stub stops first).

import CoreFoundation
import Foundation

// ---- Defaults the interface doesn't show ----

/// A primitive view (Body == Never) has no body to ask for.
extension View where Body == Never {
  public var body: Never { _pommeUnimplemented("the body of a primitive view") }
}

extension Gesture where Body == Never {
  public var body: Never { _pommeUnimplemented("the body of a primitive gesture") }
}

/// Keyframe content that is its own body.
extension KeyframeTrackContent where Body == Self {
  public var body: Self { self }
}

extension Keyframes where Body == Self {
  public var body: Self { self }
}

extension Keyframes where Body == Never {
  public var body: Never { _pommeUnimplemented("the body of primitive keyframes") }
}

extension KeyframeTrackContentBuilder.Conditional {
  public var body: Body { return _pommeUnimplementedValue("KeyframeTrackContentBuilder.Conditional.body") }
}

/// The stack layouts' Layout methods (Apple's binary has them through an
/// internal protocol).
extension Layout where Cache == _StackLayoutCache {
  public func makeCache(subviews: Subviews) -> Cache { _pommeUnimplemented("makeCache(subviews:) of a stack layout") }
  public func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) -> CGSize {
    _pommeUnimplemented("sizeThatFits(proposal:subviews:cache:) of a stack layout")
  }
  public func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) {
    _pommeUnimplemented("placeSubviews(in:proposal:subviews:cache:) of a stack layout")
  }
}

extension ZStackLayout {
  public func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    _pommeUnimplemented("ZStackLayout.sizeThatFits(proposal:subviews:cache:)")
  }
  public func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    _pommeUnimplemented("ZStackLayout.placeSubviews(in:proposal:subviews:cache:)")
  }
}

extension Spring {
  /// `Spring()`, as default arguments use it: the default spring.
  @usableFromInline
  internal init() {
    _duration = 0.5
    _bounce = 0.0
  }
}

// ---- Placeholders for opaque results pomme doesn't implement yet ----

internal struct _PommeOpaqueVisualEffect: VisualEffect {
  static func _makeVisualEffect(effect: _GraphValue<Self>, inputs: _ViewInputs, body: @escaping (_Graph, _ViewInputs) -> _ViewOutputs) -> _ViewOutputs {
    _pommeUnimplemented("a visual effect")
  }
  static func _makeTransform(effect: _GraphValue<Self>, inputs: _ViewInputs) -> _VisualEffectTransformOutputs {
    _pommeUnimplemented("a visual effect")
  }
}

internal struct _PommeOpaqueKeyframeTrackContent<Value: Animatable>: KeyframeTrackContent {
  var body: Self { self }
  func _resolve(into resolved: inout _ResolvedKeyframeTrackContent<Value>) { _pommeUnimplemented("keyframe track content") }
}

internal struct _PommeOpaqueKeyframes<Value>: Keyframes {
  var body: Self { self }
  func _resolve(into resolved: inout _ResolvedKeyframes<Value>, initialValue: Value, initialVelocity: Value?) {
    _pommeUnimplemented("keyframes")
  }
}

internal struct _PommeOpaqueShape: Shape {
  nonisolated func path(in rect: CGRect) -> Path { _pommeUnimplemented("a shape") }
}

internal struct _PommeOpaqueTransition: Transition {
  func body(content: Content, phase: TransitionPhase) -> some View { return _pommeUnimplemented("a transition") }
}

internal struct _PommeOpaqueAttributedTextFormattingDefinition<Scope: AttributeScope>: AttributedTextFormattingDefinition {
  var body: Self { self }
  static func _makeDefinition(_ inputs: AttributedTextFormatting._Inputs<Self>) -> AttributedTextFormatting._Outputs<Self> {
    _pommeUnimplemented("an attributed text formatting definition")
  }
}
