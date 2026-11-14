// Bindings: a value that lives elsewhere (a view's state, a model's property,
// a pair of closures), read and written through a location.

import Foundation

/// A binding's value is what a pair of closures get and set.
final class ClosureLocation<Value>: AnyLocation<Value>, @unchecked Sendable {
  private let get: () -> Value
  private let set: (Value) -> Void

  init(get: @escaping () -> Value, set: @escaping (Value) -> Void) {
    self.get = get
    self.set = set
  }

  override var value: Value {
    get { get() }
    set { set(newValue) }
  }
}

/// A part of another location's value (`$model.name`).
final class KeyPathLocation<Root, Value>: AnyLocation<Value>, @unchecked Sendable {
  private let base: AnyLocation<Root>
  private let keyPath: WritableKeyPath<Root, Value>

  init(_ base: AnyLocation<Root>, _ keyPath: WritableKeyPath<Root, Value>) {
    self.base = base
    self.keyPath = keyPath
  }

  override var value: Value {
    get { base.value[keyPath: keyPath] }
    set { base.value[keyPath: keyPath] = newValue }
  }
}

/// A value that never changes (`.constant`).
package final class ConstantLocation<Value>: AnyLocation<Value>, @unchecked Sendable {
  private let constant: Value

  package init(_ value: Value) {
    constant = value
  }

  package override var value: Value {
    get { constant }
    set {}
  }
}

// pomme: in public struct Binding<Value>
extension Binding {
  // pomme: replaces public struct Binding<Value> :: public var transaction: SwiftUICore.Transaction
  public var transaction: Transaction
  // pomme: replaces public struct Binding<Value> :: package var location: SwiftUICore.AnyLocation<Value>
  package var location: AnyLocation<Value>
  // pomme: replaces public struct Binding<Value> :: package var _value: Value
  package var _value: Value

  /// A binding to a location.
  package init(location: AnyLocation<Value>) {
    transaction = Transaction()
    self.location = location
    _value = location.value
  }

  // pomme: replaces public struct Binding<Value> :: public init(@_inheritActorContext get: @escaping @isolated(any) @Sendable () -> Value, @_inheritActorContext set: @escaping @isolated(any) @Sendable (Value) -> Swift.Void)
  @preconcurrency
  nonisolated public init(@_inheritActorContext get: @escaping @isolated(any) @Sendable () -> Value, @_inheritActorContext set: @escaping @isolated(any) @Sendable (Value) -> Void) {
    // (The closures run where SwiftUI calls them, on the main actor, synchronously.)
    self.init(location: ClosureLocation(get: get as @Sendable () -> Value, set: set as @Sendable (Value) -> Void))
  }

  // pomme: replaces public struct Binding<Value> :: public init(@_inheritActorContext get: @escaping @isolated(any) @Sendable () -> Value, @_inheritActorContext set: @escaping @isolated(any) @Sendable (Value, SwiftUICore.Transaction) -> Swift.Void)
  @preconcurrency
  nonisolated public init(@_inheritActorContext get: @escaping @isolated(any) @Sendable () -> Value, @_inheritActorContext set: @escaping @isolated(any) @Sendable (Value, Transaction) -> Void) {
    let set = set as @Sendable (Value, Transaction) -> Void
    self.init(location: ClosureLocation(get: get as @Sendable () -> Value, set: { set($0, Transaction()) }))
  }

  // pomme: replaces public struct Binding<Value> :: public static func constant(_ value: Value) -> SwiftUICore.Binding<Value>
  public static func constant(_ value: Value) -> Binding<Value> {
    Binding(location: ConstantLocation(value))
  }

  // pomme: replaces public struct Binding<Value> :: public var wrappedValue: Value
  public var wrappedValue: Value {
    get { location.value }
    nonmutating set { location.value = newValue }
  }

  // pomme: replaces public struct Binding<Value> :: public var projectedValue: SwiftUICore.Binding<Value>
  public var projectedValue: Binding<Value> { self }

  // pomme: replaces public struct Binding<Value> :: public subscript<Subject>(dynamicMember keyPath: Swift.WritableKeyPath<Value, Subject>) -> SwiftUICore.Binding<Subject>
  public subscript<Subject>(dynamicMember keyPath: WritableKeyPath<Value, Subject>) -> Binding<Subject> {
    Binding<Subject>(location: KeyPathLocation(location, keyPath))
  }
}
