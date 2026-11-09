// pomme's Symbols overlay: the Swift half of Symbols.framework (SF Symbol
// effects), with the public API of the iOS 26 SDK's Symbols.swiftinterface.
// Apple's is closed source; this one is written from that interface. The
// Objective-C half (NSSymbolEffect and friends) is the underlying module.
//
// An effect is a value: its kind and the choice made on each of its axes
// (direction, layering, …), the later choice on an axis replacing the
// earlier one, as the modifiers do. Effects compare and hash by that.

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
public protocol SymbolEffect: Hashable, Sendable {
  var configuration: SymbolEffectConfiguration { get }
}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
public struct SymbolEffectConfiguration: Hashable, Sendable {
  internal var kind: String
  /// Axis → the choice on it.
  internal var choices: [String: String]

  internal init(_ kind: String, _ choices: [String: String] = [:]) {
    self.kind = kind
    self.choices = choices
  }

  internal func with(_ axis: String, _ choice: String) -> SymbolEffectConfiguration {
    var c = self
    c.choices[axis] = choice
    return c
  }
}

// Each effect: a configuration and the modifiers that change it.
// (Axes: "scope" is by layer / whole symbol / individually.)

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
public struct PulseSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("pulse")
  @usableFromInline internal init() {}
  private func with(_ a: String, _ c: String) -> Self { var e = self; e.configuration = configuration.with(a, c); return e }
  public var byLayer: PulseSymbolEffect { with("scope", "byLayer") }
  public var wholeSymbol: PulseSymbolEffect { with("scope", "wholeSymbol") }
}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
public struct BounceSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("bounce")
  @usableFromInline internal init() {}
  private func with(_ a: String, _ c: String) -> Self { var e = self; e.configuration = configuration.with(a, c); return e }
  public var up: BounceSymbolEffect { with("direction", "up") }
  public var down: BounceSymbolEffect { with("direction", "down") }
  public var byLayer: BounceSymbolEffect { with("scope", "byLayer") }
  public var wholeSymbol: BounceSymbolEffect { with("scope", "wholeSymbol") }
}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
public struct VariableColorSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("variableColor")
  @usableFromInline internal init() {}
  private func with(_ a: String, _ c: String) -> Self { var e = self; e.configuration = configuration.with(a, c); return e }
  public var reversing: VariableColorSymbolEffect { with("reversing", "reversing") }
  public var nonReversing: VariableColorSymbolEffect { with("reversing", "nonReversing") }
  public var cumulative: VariableColorSymbolEffect { with("fill", "cumulative") }
  public var iterative: VariableColorSymbolEffect { with("fill", "iterative") }
  public var hideInactiveLayers: VariableColorSymbolEffect { with("inactiveLayers", "hide") }
  public var dimInactiveLayers: VariableColorSymbolEffect { with("inactiveLayers", "dim") }
}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
public struct ScaleSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("scale")
  @usableFromInline internal init() {}
  private func with(_ a: String, _ c: String) -> Self { var e = self; e.configuration = configuration.with(a, c); return e }
  public var up: ScaleSymbolEffect { with("direction", "up") }
  public var down: ScaleSymbolEffect { with("direction", "down") }
  public var byLayer: ScaleSymbolEffect { with("scope", "byLayer") }
  public var wholeSymbol: ScaleSymbolEffect { with("scope", "wholeSymbol") }
}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
public struct AppearSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("appear")
  @usableFromInline internal init() {}
  private func with(_ a: String, _ c: String) -> Self { var e = self; e.configuration = configuration.with(a, c); return e }
  public var up: AppearSymbolEffect { with("direction", "up") }
  public var down: AppearSymbolEffect { with("direction", "down") }
  public var byLayer: AppearSymbolEffect { with("scope", "byLayer") }
  public var wholeSymbol: AppearSymbolEffect { with("scope", "wholeSymbol") }
}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
public struct DisappearSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("disappear")
  @usableFromInline internal init() {}
  private func with(_ a: String, _ c: String) -> Self { var e = self; e.configuration = configuration.with(a, c); return e }
  public var up: DisappearSymbolEffect { with("direction", "up") }
  public var down: DisappearSymbolEffect { with("direction", "down") }
  public var byLayer: DisappearSymbolEffect { with("scope", "byLayer") }
  public var wholeSymbol: DisappearSymbolEffect { with("scope", "wholeSymbol") }
}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
public struct ReplaceSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("replace")
  @usableFromInline internal init() {}
  private func with(_ a: String, _ c: String) -> Self { var e = self; e.configuration = configuration.with(a, c); return e }
  public var downUp: ReplaceSymbolEffect { with("style", "downUp") }
  public var upUp: ReplaceSymbolEffect { with("style", "upUp") }
  public var offUp: ReplaceSymbolEffect { with("style", "offUp") }
  public var byLayer: ReplaceSymbolEffect { with("scope", "byLayer") }
  public var wholeSymbol: ReplaceSymbolEffect { with("scope", "wholeSymbol") }
}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
public struct AutomaticSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("automatic")
  @usableFromInline internal init() {}
}

@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
public struct WiggleSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("wiggle")
  @usableFromInline internal init() {}
  private func with(_ a: String, _ c: String) -> Self { var e = self; e.configuration = configuration.with(a, c); return e }
  public var clockwise: WiggleSymbolEffect { with("direction", "clockwise") }
  public var counterClockwise: WiggleSymbolEffect { with("direction", "counterClockwise") }
  public var left: WiggleSymbolEffect { with("direction", "left") }
  public var right: WiggleSymbolEffect { with("direction", "right") }
  public var up: WiggleSymbolEffect { with("direction", "up") }
  public var down: WiggleSymbolEffect { with("direction", "down") }
  public var forward: WiggleSymbolEffect { with("direction", "forward") }
  public var backward: WiggleSymbolEffect { with("direction", "backward") }
  public func custom(angle: Double) -> WiggleSymbolEffect { with("direction", "angle \(angle)") }
  public var byLayer: WiggleSymbolEffect { with("scope", "byLayer") }
  public var wholeSymbol: WiggleSymbolEffect { with("scope", "wholeSymbol") }
}

@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
public struct RotateSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("rotate")
  @usableFromInline internal init() {}
  private func with(_ a: String, _ c: String) -> Self { var e = self; e.configuration = configuration.with(a, c); return e }
  public var clockwise: RotateSymbolEffect { with("direction", "clockwise") }
  public var counterClockwise: RotateSymbolEffect { with("direction", "counterClockwise") }
  public var byLayer: RotateSymbolEffect { with("scope", "byLayer") }
  public var wholeSymbol: RotateSymbolEffect { with("scope", "wholeSymbol") }
}

@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
public struct BreatheSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("breathe")
  @usableFromInline internal init() {}
  private func with(_ a: String, _ c: String) -> Self { var e = self; e.configuration = configuration.with(a, c); return e }
  public var pulse: BreatheSymbolEffect { with("style", "pulse") }
  public var plain: BreatheSymbolEffect { with("style", "plain") }
  public var byLayer: BreatheSymbolEffect { with("scope", "byLayer") }
  public var wholeSymbol: BreatheSymbolEffect { with("scope", "wholeSymbol") }
}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension ReplaceSymbolEffect {
  /// A magic replace, falling back to `fallback` where the symbols don't share layers.
  @available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
  public struct MagicReplace: SymbolEffect {
    public private(set) var configuration: SymbolEffectConfiguration
    internal init(fallback: ReplaceSymbolEffect) {
      var c = SymbolEffectConfiguration("magicReplace")
      for (axis, choice) in fallback.configuration.choices {
        c.choices["fallback." + axis] = choice
      }
      configuration = c
    }
  }

  @available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
  public func magic(fallback: ReplaceSymbolEffect) -> MagicReplace { MagicReplace(fallback: fallback) }
  @available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
  public static var downUp: ReplaceSymbolEffect { ReplaceSymbolEffect().downUp }
  @available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
  public static var upUp: ReplaceSymbolEffect { ReplaceSymbolEffect().upUp }
  @available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
  public static var offUp: ReplaceSymbolEffect { ReplaceSymbolEffect().offUp }
}

@available(macOS 26.0, iOS 26.0, tvOS 26.0, watchOS 26.0, visionOS 26.0, *)
public struct DrawOnSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("drawOn")
  @usableFromInline internal init() {}
  private func with(_ a: String, _ c: String) -> Self { var e = self; e.configuration = configuration.with(a, c); return e }
  public var byLayer: DrawOnSymbolEffect { with("scope", "byLayer") }
  public var wholeSymbol: DrawOnSymbolEffect { with("scope", "wholeSymbol") }
  public var individually: DrawOnSymbolEffect { with("scope", "individually") }
}

@available(macOS 26.0, iOS 26.0, tvOS 26.0, watchOS 26.0, visionOS 26.0, *)
public struct DrawOffSymbolEffect: SymbolEffect {
  public private(set) var configuration = SymbolEffectConfiguration("drawOff")
  @usableFromInline internal init() {}
  private func with(_ a: String, _ c: String) -> Self { var e = self; e.configuration = configuration.with(a, c); return e }
  public var byLayer: DrawOffSymbolEffect { with("scope", "byLayer") }
  public var wholeSymbol: DrawOffSymbolEffect { with("scope", "wholeSymbol") }
  public var individually: DrawOffSymbolEffect { with("scope", "individually") }
  public var reversed: DrawOffSymbolEffect { with("order", "reversed") }
  public var nonReversed: DrawOffSymbolEffect { with("order", "nonReversed") }
}

// ---- options ----

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
public struct SymbolEffectOptions: Hashable, Sendable {
  /// How fast the effect plays (1 is its natural speed).
  internal var speed: Double = 1
  internal var repetition: Repetition = .unspecified

  internal enum Repetition: Hashable, Sendable {
    /// The effect's own default.
    case unspecified
    case nonRepeating
    /// Periodic: `count` times (nil: forever), `delay` seconds apart (nil: the effect's own).
    case periodic(count: Int?, delay: Double?)
    case continuous
  }

  internal init() {}

  public static var `default`: SymbolEffectOptions { SymbolEffectOptions() }
  public static func speed(_ speed: Double) -> SymbolEffectOptions { SymbolEffectOptions().speed(speed) }
  public func speed(_ speed: Double) -> SymbolEffectOptions {
    var o = self
    o.speed = speed
    return o
  }
  public static func `repeat`(_ count: Int?) -> SymbolEffectOptions { SymbolEffectOptions().repeat(count) }
  public func `repeat`(_ count: Int?) -> SymbolEffectOptions {
    var o = self
    o.repetition = .periodic(count: count, delay: nil)
    return o
  }
  public static var repeating: SymbolEffectOptions { SymbolEffectOptions().repeating }
  public var repeating: SymbolEffectOptions { self.repeat(nil) }
  public static var nonRepeating: SymbolEffectOptions { SymbolEffectOptions().nonRepeating }
  public var nonRepeating: SymbolEffectOptions {
    var o = self
    o.repetition = .nonRepeating
    return o
  }
}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension SymbolEffectOptions {
  @available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
  public struct RepeatBehavior {
    internal var repetition: Repetition
    public static var periodic: RepeatBehavior { RepeatBehavior(repetition: .periodic(count: nil, delay: nil)) }
    public static func periodic(_ count: Int? = nil, delay: Double? = nil) -> RepeatBehavior {
      RepeatBehavior(repetition: .periodic(count: count, delay: delay))
    }
    public static var continuous: RepeatBehavior { RepeatBehavior(repetition: .continuous) }
  }

  @available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
  public func `repeat`(_ behavior: RepeatBehavior) -> SymbolEffectOptions {
    var o = self
    o.repetition = behavior.repetition
    return o
  }
  @available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
  public static func `repeat`(_ behavior: RepeatBehavior) -> SymbolEffectOptions { SymbolEffectOptions().repeat(behavior) }
}

// ---- what each effect can be used for ----

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
@_marker public protocol TransitionSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension AppearSymbolEffect: TransitionSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension DisappearSymbolEffect: TransitionSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension AutomaticSymbolEffect: TransitionSymbolEffect {}
@available(macOS 26.0, iOS 26.0, tvOS 26.0, watchOS 26.0, visionOS 26.0, *)
extension DrawOnSymbolEffect: TransitionSymbolEffect {}
@available(macOS 26.0, iOS 26.0, tvOS 26.0, watchOS 26.0, visionOS 26.0, *)
extension DrawOffSymbolEffect: TransitionSymbolEffect {}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
@_marker public protocol ContentTransitionSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension ReplaceSymbolEffect: ContentTransitionSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension AutomaticSymbolEffect: ContentTransitionSymbolEffect {}
@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension ReplaceSymbolEffect.MagicReplace: ContentTransitionSymbolEffect {}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
@_marker public protocol IndefiniteSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension PulseSymbolEffect: IndefiniteSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension VariableColorSymbolEffect: IndefiniteSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension ScaleSymbolEffect: IndefiniteSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension AppearSymbolEffect: IndefiniteSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension DisappearSymbolEffect: IndefiniteSymbolEffect {}
@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension WiggleSymbolEffect: IndefiniteSymbolEffect {}
@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension RotateSymbolEffect: IndefiniteSymbolEffect {}
@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension BreatheSymbolEffect: IndefiniteSymbolEffect {}
@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension BounceSymbolEffect: IndefiniteSymbolEffect {}
@available(macOS 26.0, iOS 26.0, tvOS 26.0, watchOS 26.0, visionOS 26.0, *)
extension DrawOnSymbolEffect: IndefiniteSymbolEffect {}
@available(macOS 26.0, iOS 26.0, tvOS 26.0, watchOS 26.0, visionOS 26.0, *)
extension DrawOffSymbolEffect: IndefiniteSymbolEffect {}

@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
@_marker public protocol DiscreteSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension BounceSymbolEffect: DiscreteSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension PulseSymbolEffect: DiscreteSymbolEffect {}
@available(macOS 14.0, iOS 17.0, tvOS 17.0, watchOS 10.0, visionOS 1.0, *)
extension VariableColorSymbolEffect: DiscreteSymbolEffect {}
@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension WiggleSymbolEffect: DiscreteSymbolEffect {}
@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension RotateSymbolEffect: DiscreteSymbolEffect {}
@available(macOS 15.0, iOS 18.0, tvOS 18.0, watchOS 11.0, visionOS 2.0, *)
extension BreatheSymbolEffect: DiscreteSymbolEffect {}
