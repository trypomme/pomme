// The environment: values a view passes down to the views it contains
// (`.environment(\.key, value)`), read with `@Environment`. The built-in values
// (`layoutDirection`, `colorScheme` …) are kept by name: what's written is
// read back, and otherwise the system's default, from `EnvironmentValues.pommeDefaults`.

import Foundation

// pomme: in public struct EnvironmentValues : Swift.CustomStringConvertible
extension EnvironmentValues {
  /// Values set by key type (`EnvironmentKey`) or, for the built-in ones, by name.
  package var pommeByKey: [ObjectIdentifier: Any]
  package var pommeByName: [String: Any]

  // pomme: replaces public struct EnvironmentValues : Swift.CustomStringConvertible :: public init()
  public init() {
    _pommeStorage = ()
    pommeByKey = [:]
    pommeByName = [:]
  }

  // pomme: replaces public struct EnvironmentValues : Swift.CustomStringConvertible :: public subscript<K>(key: K.Type) -> K.Value where K : SwiftUICore.EnvironmentKey
  public subscript<K>(key: K.Type) -> K.Value where K: EnvironmentKey {
    get { pommeByKey[ObjectIdentifier(key)] as? K.Value ?? K.defaultValue }
    set { pommeByKey[ObjectIdentifier(key)] = newValue }
  }

  // pomme: replaces public struct EnvironmentValues : Swift.CustomStringConvertible :: public var description: Swift.String
  public var description: String {
    "EnvironmentValues(\(pommeByName.keys.sorted().joined(separator: ", ")))"
  }
}

extension EnvironmentValues {
  /// The system's values for the built-in keys a view hasn't set (filled in by
  /// the code that implements each one).
  nonisolated(unsafe) package static var pommeDefaults: [String: Any] = [
    "layoutDirection": LayoutDirection.leftToRight,
    "colorScheme": ColorScheme.light,
    "isEnabled": true,
  ]

  /// A built-in value, by name: as set, else the system's default.
  package func pommeEnvironmentValue<T>(_ name: StaticString) -> T {
    let key = "\(name)"
    if let v = pommeByName[key] ?? Self.pommeDefaults[key] {
      return v as! T
    }
    if let none = Optional<Any>.none as? T {
      return none  // an optional value: nil until set
    }
    fatalError("SwiftUI in pomme doesn't implement the default of EnvironmentValues.\(key) yet")
  }

  package mutating func pommeSetEnvironmentValue<T>(_ name: StaticString, _ value: T) {
    pommeByName["\(name)"] = value
  }
}

// pomme: in extension SwiftUICore.EnvironmentKey
extension EnvironmentKey {
  // pomme: replaces extension SwiftUICore.EnvironmentKey :: public static func _valuesEqual(_ lhs: Self.Value, _ rhs: Self.Value) -> Swift.Bool
  /// Values that can't be compared count as different.
  public static func _valuesEqual(_ lhs: Value, _ rhs: Value) -> Bool { false }
}

// pomme: in extension SwiftUICore.EnvironmentKey where Self.Value : Swift.Equatable
extension EnvironmentKey where Value: Equatable {
  // pomme: replaces extension SwiftUICore.EnvironmentKey where Self.Value : Swift.Equatable :: public static func _valuesEqual(_ lhs: Self.Value, _ rhs: Self.Value) -> Swift.Bool
  public static func _valuesEqual(_ lhs: Value, _ rhs: Value) -> Bool { lhs == rhs }
}

extension Environment: PommeDynamicProperty {
  /// Read from the environment the view is in.
  package mutating func pommeAttach(to storage: ViewStorage, slot: Int, environment: EnvironmentValues) {
    if case .keyPath(let keyPath) = content {
      content = .value(environment[keyPath: keyPath])
    }
  }
}

extension _EnvironmentKeyWritingModifier {
  /// Set the value in the environment its content gets.
  package func pommeWrite(_ environment: inout EnvironmentValues) {
    environment[keyPath: keyPath] = value
  }
}

// ---- A custom modifier's content ----

extension _ViewModifier_Content {
  /// The view a custom modifier's `body(content:)` gets: it stands for the
  /// modified view (pomme's view graph resolves it to that).
  package init() {
    _pommeStorage = ()
  }
}
