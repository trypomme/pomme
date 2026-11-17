// Animations and transitions: values a view describes how it changes with.
// pomme's SwiftUI doesn't animate yet: an animation or a transition is kept
// (what it is, for equality), and the change it describes is shown at once.

import Foundation

// pomme: in internal class AnimationBoxBase : @unchecked Swift.Sendable
extension AnimationBoxBase {
  package init(pommeBox: ()) {}
}

/// What an animation is: its kind and timing, as text (for equality).
final class PommeAnimationBox: AnimationBoxBase, @unchecked Sendable {
  package let name: String
  package init(_ name: String) {
    self.name = name
    super.init(pommeBox: ())
  }
}

// pomme: in public struct Animation : Swift.Equatable, Swift.Sendable
extension Animation {
  package init(pommeName: String) {
    box = PommeAnimationBox(pommeName)
  }

  package var pommeName: String { (box as? PommeAnimationBox)?.name ?? "?" }

  // pomme: replaces public struct Animation : Swift.Equatable, Swift.Sendable :: public static func == (lhs: SwiftUICore.Animation, rhs: SwiftUICore.Animation) -> Swift.Bool
  public static func == (lhs: Animation, rhs: Animation) -> Bool { lhs.pommeName == rhs.pommeName }
}

// pomme: in extension SwiftUICore.Animation : Swift.Hashable
extension Animation {
  // pomme: replaces extension SwiftUICore.Animation : Swift.Hashable :: public func hash(into hasher: inout Swift.Hasher)
  public func hash(into hasher: inout Hasher) { hasher.combine(pommeName) }
  // pomme: replaces extension SwiftUICore.Animation : Swift.Hashable :: public var hashValue: Swift.Int
  public var hashValue: Int { pommeName.hashValue }
}

// pomme: in extension SwiftUICore.Animation
extension Animation {
  // pomme: replaces extension SwiftUICore.Animation :: public static let `default`: SwiftUICore.Animation
  public static let `default` = Animation(pommeName: "default")

  // pomme: replaces extension SwiftUICore.Animation :: public static func easeInOut(duration: Foundation.TimeInterval) -> SwiftUICore.Animation
  public static func easeInOut(duration: TimeInterval) -> Animation { Animation(pommeName: "easeInOut(\(duration))") }
  // pomme: replaces extension SwiftUICore.Animation :: public static var easeInOut: SwiftUICore.Animation
  public static var easeInOut: Animation { easeInOut(duration: 0.35) }
  // pomme: replaces extension SwiftUICore.Animation :: public static func easeIn(duration: Foundation.TimeInterval) -> SwiftUICore.Animation
  public static func easeIn(duration: TimeInterval) -> Animation { Animation(pommeName: "easeIn(\(duration))") }
  // pomme: replaces extension SwiftUICore.Animation :: public static var easeIn: SwiftUICore.Animation
  public static var easeIn: Animation { easeIn(duration: 0.35) }
  // pomme: replaces extension SwiftUICore.Animation :: public static func easeOut(duration: Foundation.TimeInterval) -> SwiftUICore.Animation
  public static func easeOut(duration: TimeInterval) -> Animation { Animation(pommeName: "easeOut(\(duration))") }
  // pomme: replaces extension SwiftUICore.Animation :: public static var easeOut: SwiftUICore.Animation
  public static var easeOut: Animation { easeOut(duration: 0.35) }
  // pomme: replaces extension SwiftUICore.Animation :: public static func linear(duration: Foundation.TimeInterval) -> SwiftUICore.Animation
  public static func linear(duration: TimeInterval) -> Animation { Animation(pommeName: "linear(\(duration))") }
  // pomme: replaces extension SwiftUICore.Animation :: public static var linear: SwiftUICore.Animation
  public static var linear: Animation { linear(duration: 0.35) }

  // pomme: replaces extension SwiftUICore.Animation :: public static func spring(response: Swift.Double = 0.5, dampingFraction: Swift.Double = 0.825, blendDuration: Foundation.TimeInterval = 0) -> SwiftUICore.Animation
  public static func spring(response: Double = 0.5, dampingFraction: Double = 0.825, blendDuration: TimeInterval = 0) -> Animation {
    Animation(pommeName: "spring(\(response), \(dampingFraction))")
  }
  // pomme: replaces extension SwiftUICore.Animation :: public static func spring(duration: Foundation.TimeInterval = 0.5, bounce: Swift.Double = 0.0, blendDuration: Swift.Double = 0) -> SwiftUICore.Animation
  public static func spring(duration: TimeInterval = 0.5, bounce: Double = 0.0, blendDuration: Double = 0) -> Animation {
    Animation(pommeName: "spring(duration: \(duration), bounce: \(bounce))")
  }
  // pomme: replaces extension SwiftUICore.Animation :: public static var spring: SwiftUICore.Animation
  public static var spring: Animation { spring(response: 0.5, dampingFraction: 0.825, blendDuration: 0) }
  // pomme: replaces extension SwiftUICore.Animation :: public static func interactiveSpring(response: Swift.Double = 0.15, dampingFraction: Swift.Double = 0.86, blendDuration: Foundation.TimeInterval = 0.25) -> SwiftUICore.Animation
  public static func interactiveSpring(response: Double = 0.15, dampingFraction: Double = 0.86, blendDuration: TimeInterval = 0.25) -> Animation {
    spring(response: response, dampingFraction: dampingFraction, blendDuration: blendDuration)
  }
  // pomme: replaces extension SwiftUICore.Animation :: public static var interactiveSpring: SwiftUICore.Animation
  public static var interactiveSpring: Animation { interactiveSpring(response: 0.15, dampingFraction: 0.86, blendDuration: 0.25) }
  // pomme: replaces extension SwiftUICore.Animation :: public static var smooth: SwiftUICore.Animation
  public static var smooth: Animation { smooth(duration: 0.5, extraBounce: 0) }
  // pomme: replaces extension SwiftUICore.Animation :: public static func smooth(duration: Foundation.TimeInterval = 0.5, extraBounce: Swift.Double = 0.0) -> SwiftUICore.Animation
  public static func smooth(duration: TimeInterval = 0.5, extraBounce: Double = 0.0) -> Animation { spring(duration: duration, bounce: extraBounce, blendDuration: 0) }
  // pomme: replaces extension SwiftUICore.Animation :: public static var snappy: SwiftUICore.Animation
  public static var snappy: Animation { snappy(duration: 0.5, extraBounce: 0) }
  // pomme: replaces extension SwiftUICore.Animation :: public static func snappy(duration: Foundation.TimeInterval = 0.5, extraBounce: Swift.Double = 0.0) -> SwiftUICore.Animation
  public static func snappy(duration: TimeInterval = 0.5, extraBounce: Double = 0.0) -> Animation { spring(duration: duration, bounce: 0.15 + extraBounce, blendDuration: 0) }
  // pomme: replaces extension SwiftUICore.Animation :: public static var bouncy: SwiftUICore.Animation
  public static var bouncy: Animation { bouncy(duration: 0.5, extraBounce: 0) }
  // pomme: replaces extension SwiftUICore.Animation :: public static func bouncy(duration: Foundation.TimeInterval = 0.5, extraBounce: Swift.Double = 0.0) -> SwiftUICore.Animation
  public static func bouncy(duration: TimeInterval = 0.5, extraBounce: Double = 0.0) -> Animation { spring(duration: duration, bounce: 0.3 + extraBounce, blendDuration: 0) }

  // pomme: replaces extension SwiftUICore.Animation :: public func delay(_ delay: Foundation.TimeInterval) -> SwiftUICore.Animation
  public func delay(_ delay: TimeInterval) -> Animation { Animation(pommeName: "\(pommeName).delay(\(delay))") }
  // pomme: replaces extension SwiftUICore.Animation :: public func speed(_ speed: Swift.Double) -> SwiftUICore.Animation
  public func speed(_ speed: Double) -> Animation { Animation(pommeName: "\(pommeName).speed(\(speed))") }
  // pomme: replaces extension SwiftUICore.Animation :: public func repeatCount(_ repeatCount: Swift.Int, autoreverses: Swift.Bool = true) -> SwiftUICore.Animation
  public func repeatCount(_ repeatCount: Int, autoreverses: Bool = true) -> Animation { Animation(pommeName: "\(pommeName).repeatCount(\(repeatCount), \(autoreverses))") }
  // pomme: replaces extension SwiftUICore.Animation :: public func repeatForever(autoreverses: Swift.Bool = true) -> SwiftUICore.Animation
  public func repeatForever(autoreverses: Bool = true) -> Animation { Animation(pommeName: "\(pommeName).repeatForever(\(autoreverses))") }
}

// ---- Transitions ----

// pomme: in internal class AnyTransitionBox
extension AnyTransitionBox {
  package init(pommeBox: ()) {}
}

final class PommeTransitionBox: AnyTransitionBox {
  package let name: String
  package init(_ name: String) {
    self.name = name
    super.init(pommeBox: ())
  }
}

// pomme: in public struct AnyTransition
extension AnyTransition {
  package init(pommeName: String) {
    box = PommeTransitionBox(pommeName)
  }

  package var pommeName: String { (box as? PommeTransitionBox)?.name ?? "?" }
}

// pomme: in extension SwiftUICore.AnyTransition
extension AnyTransition {
  // pomme: replaces extension SwiftUICore.AnyTransition :: nonisolated(unsafe) public static let opacity: SwiftUICore.AnyTransition
  nonisolated(unsafe) public static let opacity = AnyTransition(pommeName: "opacity")
  // pomme: replaces extension SwiftUICore.AnyTransition :: public static var identity: SwiftUICore.AnyTransition
  public static var identity: AnyTransition { AnyTransition(pommeName: "identity") }
  // pomme: replaces extension SwiftUICore.AnyTransition :: public static var scale: SwiftUICore.AnyTransition
  public static var scale: AnyTransition { AnyTransition(pommeName: "scale") }
  // pomme: replaces extension SwiftUICore.AnyTransition :: public static func scale(scale: CoreFoundation.CGFloat, anchor: SwiftUICore.UnitPoint = .center) -> SwiftUICore.AnyTransition
  public static func scale(scale: CGFloat, anchor: UnitPoint = .center) -> AnyTransition { AnyTransition(pommeName: "scale(\(scale))") }
  // pomme: replaces extension SwiftUICore.AnyTransition :: public static var slide: SwiftUICore.AnyTransition
  public static var slide: AnyTransition { AnyTransition(pommeName: "slide") }
  // pomme: replaces extension SwiftUICore.AnyTransition :: public static func move(edge: SwiftUICore.Edge) -> SwiftUICore.AnyTransition
  public static func move(edge: Edge) -> AnyTransition { AnyTransition(pommeName: "move(\(edge))") }
  // pomme: replaces extension SwiftUICore.AnyTransition :: public static func push(from edge: SwiftUICore.Edge) -> SwiftUICore.AnyTransition
  public static func push(from edge: Edge) -> AnyTransition { AnyTransition(pommeName: "push(\(edge))") }
  // pomme: replaces extension SwiftUICore.AnyTransition :: public static func offset(_ offset: CoreFoundation.CGSize) -> SwiftUICore.AnyTransition
  public static func offset(_ offset: CGSize) -> AnyTransition { AnyTransition(pommeName: "offset(\(offset))") }
  // pomme: replaces extension SwiftUICore.AnyTransition :: public static func offset(x: CoreFoundation.CGFloat = 0, y: CoreFoundation.CGFloat = 0) -> SwiftUICore.AnyTransition
  public static func offset(x: CGFloat = 0, y: CGFloat = 0) -> AnyTransition { offset(CGSize(width: x, height: y)) }
  // pomme: replaces extension SwiftUICore.AnyTransition :: public static func asymmetric(insertion: SwiftUICore.AnyTransition, removal: SwiftUICore.AnyTransition) -> SwiftUICore.AnyTransition
  public static func asymmetric(insertion: AnyTransition, removal: AnyTransition) -> AnyTransition {
    AnyTransition(pommeName: "asymmetric(\(insertion.pommeName), \(removal.pommeName))")
  }
  // pomme: replaces extension SwiftUICore.AnyTransition :: public func combined(with other: SwiftUICore.AnyTransition) -> SwiftUICore.AnyTransition
  public func combined(with other: AnyTransition) -> AnyTransition { AnyTransition(pommeName: "\(pommeName)+\(other.pommeName)") }
  // pomme: replaces extension SwiftUICore.AnyTransition :: public func animation(_ animation: SwiftUICore.Animation?) -> SwiftUICore.AnyTransition
  public func animation(_ animation: Animation?) -> AnyTransition { self }
}

// ---- Color.opacity ----

// pomme: in extension SwiftUICore.Color
extension Color {
  // pomme: replaces extension SwiftUICore.Color :: public func opacity(_ opacity: Swift.Double) -> SwiftUICore.Color
  /// The color with its opacity multiplied by `opacity`, in both appearances.
  public func opacity(_ opacity: Double) -> Color {
    let light = pommeBox?.light ?? [0, 0, 0, 0]
    let dark = pommeBox?.dark ?? light
    return Color(pommeLight: [light[0], light[1], light[2], light[3] * opacity],
                 dark: [dark[0], dark[1], dark[2], dark[3] * opacity])
  }
}

// ---- onChange(of:initial:) ----

/// `.onChange(of:initial:_:)`: the action, after an update in which the value
/// changed (and, with `initial`, when the view first appears).
package struct PommeOnChangeModifier<V: Equatable>: ViewModifier {
  package var value: V
  package var initial: Bool
  package var action: (V, V) -> Void
  package typealias Body = Never
  package func body(content: Content) -> Never { fatalError("pomme resolves onChange itself") }
}

// pomme: in extension SwiftUICore.View
extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func onChange<V>(of value: V, initial: Swift.Bool = false, _ action: @escaping (_ oldValue: V, _ newValue: V) -> Swift.Void) -> some SwiftUICore.View where V : Swift.Equatable
  nonisolated public func onChange<V>(of value: V, initial: Bool = false, _ action: @escaping (_ oldValue: V, _ newValue: V) -> Void) -> some View where V: Equatable {
    modifier(PommeOnChangeModifier(value: value, initial: initial, action: action))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func onChange<V>(of value: V, initial: Swift.Bool = false, _ action: @escaping () -> Swift.Void) -> some SwiftUICore.View where V : Swift.Equatable
  nonisolated public func onChange<V>(of value: V, initial: Bool = false, _ action: @escaping () -> Void) -> some View where V: Equatable {
    modifier(PommeOnChangeModifier(value: value, initial: initial) { _, _ in action() })
  }
}

// ---- LazyVStack ----

/// A LazyVStack is a VStack (pomme makes all its views, not only those on screen).
// pomme: in public struct LazyVStack<Content> : SwiftUICore.View where Content : SwiftUICore.View
extension LazyVStack {
  package var pommeAlignment: HorizontalAlignment
  package var pommeSpacing: CGFloat?
  package var pommeContent: Content

  // pomme: replaces public struct LazyVStack<Content> : SwiftUICore.View where Content : SwiftUICore.View :: public init(alignment: SwiftUICore.HorizontalAlignment = .center, spacing: CoreFoundation.CGFloat? = nil, pinnedViews: SwiftUICore.PinnedScrollableViews = .init(), @SwiftUICore.ViewBuilder content: () -> Content)
  public init(alignment: HorizontalAlignment = .center, spacing: CGFloat? = nil, pinnedViews: PinnedScrollableViews = .init(), @ViewBuilder content: () -> Content) {
    _pommeStorage = ()
    pommeAlignment = alignment
    pommeSpacing = spacing
    pommeContent = content()
  }
}

extension LazyVStack {
  package var pommeStack: VStack<Content> {
    let content = pommeContent
    return VStack(alignment: pommeAlignment, spacing: pommeSpacing) { content }
  }
}

// ---- Materials ----

// pomme: in public struct Material : Swift.Sendable
extension Material {
  package var pommeName: String

  package init(pommeName: String) {
    _pommeStorage = ()
    self.pommeName = pommeName
  }
}

// pomme: in extension SwiftUICore.Material
extension Material {
  // pomme: replaces extension SwiftUICore.Material :: public static let regular: SwiftUICore.Material
  public static let regular = Material(pommeName: "regular")
  // pomme: replaces extension SwiftUICore.Material :: public static let thick: SwiftUICore.Material
  public static let thick = Material(pommeName: "thick")
  // pomme: replaces extension SwiftUICore.Material :: public static let thin: SwiftUICore.Material
  public static let thin = Material(pommeName: "thin")
  // pomme: replaces extension SwiftUICore.Material :: public static let ultraThin: SwiftUICore.Material
  public static let ultraThin = Material(pommeName: "ultraThin")
  // pomme: replaces extension SwiftUICore.Material :: public static let ultraThick: SwiftUICore.Material
  public static let ultraThick = Material(pommeName: "ultraThick")
  // pomme: replaces extension SwiftUICore.Material :: public static let bar: SwiftUICore.Material
  public static let bar = Material(pommeName: "bar")
}

// pomme: in extension SwiftUICore.ShapeStyle where Self == SwiftUICore.Material
extension ShapeStyle where Self == Material {
  // pomme: replaces extension SwiftUICore.ShapeStyle where Self == SwiftUICore.Material :: public static var regularMaterial: SwiftUICore.Material
  public static var regularMaterial: Material { .regular }
  // pomme: replaces extension SwiftUICore.ShapeStyle where Self == SwiftUICore.Material :: public static var thickMaterial: SwiftUICore.Material
  public static var thickMaterial: Material { .thick }
  // pomme: replaces extension SwiftUICore.ShapeStyle where Self == SwiftUICore.Material :: public static var thinMaterial: SwiftUICore.Material
  public static var thinMaterial: Material { .thin }
  // pomme: replaces extension SwiftUICore.ShapeStyle where Self == SwiftUICore.Material :: public static var ultraThinMaterial: SwiftUICore.Material
  public static var ultraThinMaterial: Material { .ultraThin }
  // pomme: replaces extension SwiftUICore.ShapeStyle where Self == SwiftUICore.Material :: public static var ultraThickMaterial: SwiftUICore.Material
  public static var ultraThickMaterial: Material { .ultraThick }
  // pomme: replaces extension SwiftUICore.ShapeStyle where Self == SwiftUICore.Material :: public static var bar: SwiftUICore.Material
  public static var bar: Material { .bar }
}

// ---- StrokeStyle ----

// pomme: in public struct StrokeStyle : Swift.Equatable
extension StrokeStyle {
  // pomme: replaces public struct StrokeStyle : Swift.Equatable :: public init(lineWidth: CoreFoundation.CGFloat = 1, lineCap: CoreGraphics.CGLineCap = .butt, lineJoin: CoreGraphics.CGLineJoin = .miter, miterLimit: CoreFoundation.CGFloat = 10, dash: [CoreFoundation.CGFloat] = [CGFloat](), dashPhase: CoreFoundation.CGFloat = 0)
  public init(lineWidth: CGFloat = 1, lineCap: CGLineCap = .butt, lineJoin: CGLineJoin = .miter, miterLimit: CGFloat = 10, dash: [CGFloat] = [CGFloat](), dashPhase: CGFloat = 0) {
    self.lineWidth = lineWidth
    self.lineCap = lineCap
    self.lineJoin = lineJoin
    self.miterLimit = miterLimit
    self.dash = dash
    self.dashPhase = dashPhase
  }
}

// ---- Transaction ----

// pomme: in extension SwiftUICore.Transaction
extension Transaction {
  // pomme: replaces extension SwiftUICore.Transaction :: public init(animation: SwiftUICore.Animation?)
  public init(animation: Animation?) {
    self.init()
  }

  // pomme: replaces extension SwiftUICore.Transaction :: public var animation: SwiftUICore.Animation?
  /// (None: pomme's SwiftUI shows changes at once.)
  public var animation: Animation? {
    get { nil }
    set {}
  }

  // pomme: replaces extension SwiftUICore.Transaction :: public var disablesAnimations: Swift.Bool
  public var disablesAnimations: Bool {
    get { true }
    set {}
  }

  // pomme: replaces extension SwiftUICore.Transaction :: public var isContinuous: Swift.Bool
  public var isContinuous: Bool {
    get { false }
    set {}
  }
}

// ---- .id(_:) ----

// pomme: in package struct IDView<Content, ID> : SwiftUICore.View where Content : SwiftUICore.View, ID : Swift.Hashable
extension IDView {
  package var pommeContent: Content { content }
  package var pommeID: ID { id }
}
