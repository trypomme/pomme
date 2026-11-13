// State: a view's own values, kept by pomme's view graph across updates.
//
// A `State` is created with its initial value (`_value`). Before the graph
// asks a view for its body, it gives each of the view's dynamic properties
// their storage: a State gets a location (`_location`) that lives as long as
// the view keeps its place in the tree. Reads and writes go through the
// location; a write tells the graph to update.

import Combine
import Foundation

// pomme: in open class AnyLocationBase : @unchecked Swift.Sendable
extension AnyLocationBase {
  package init() {}
}

// pomme: in open class AnyLocation<Value> : SwiftUICore.AnyLocationBase, @unchecked Swift.Sendable
extension AnyLocation {
  package override init() {}

  /// The value: a subclass keeps it.
  package var value: Value {
    get { fatalError("AnyLocation.value: a subclass provides it") }
    set { fatalError("AnyLocation.value: a subclass provides it") }
  }
}

/// A view's storage in the graph: its states' locations, by the order of the
/// view's stored properties, and what to do when one of them changes.
package final class ViewStorage {
  package var locations: [Int: AnyLocationBase] = [:]
  /// The view's subscriptions to the objects it observes, by slot.
  package var observing: [Int: (object: ObjectIdentifier, subscription: AnyCancellable)] = [:]
  package let changed: () -> Void

  package init(changed: @escaping () -> Void) {
    self.changed = changed
  }
}

/// A State's value as the graph keeps it.
package final class StoredLocation<Value>: AnyLocation<Value>, @unchecked Sendable {
  private var stored: Value
  private weak var storage: ViewStorage?

  package init(_ value: Value, storage: ViewStorage) {
    stored = value
    self.storage = storage
  }

  package override var value: Value {
    get { stored }
    set {
      stored = newValue
      storage?.changed()
    }
  }
}

/// A dynamic property the graph prepares before asking a view for its body.
package protocol PommeDynamicProperty {
  mutating func pommeAttach(to storage: ViewStorage, slot: Int, environment: EnvironmentValues)
}

extension State: PommeDynamicProperty {
  package mutating func pommeAttach(to storage: ViewStorage, slot: Int, environment: EnvironmentValues) {
    if let location = storage.locations[slot] as? StoredLocation<Value> {
      _location = location
    } else {
      let location = StoredLocation(_value, storage: storage)
      storage.locations[slot] = location
      _location = location
    }
  }
}

// pomme: in public struct State<Value> : SwiftUICore.DynamicProperty
extension State {
  // pomme: replaces public struct State<Value> : SwiftUICore.DynamicProperty :: public init(wrappedValue value: Value)
  public init(wrappedValue value: Value) {
    _value = value
    _location = nil
  }

  // pomme: replaces public struct State<Value> : SwiftUICore.DynamicProperty :: public var wrappedValue: Value
  /// Outside a view in the graph (no location yet), the initial value; a
  /// write there changes nothing, as in SwiftUI.
  public var wrappedValue: Value {
    get { _location?.value ?? _value }
    nonmutating set { _location?.value = newValue }
  }
}

// pomme: in public struct ObservedObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject
extension ObservedObject {
  // pomme: replaces public struct ObservedObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject :: public init(wrappedValue: ObjectType)
  @MainActor @preconcurrency
  public init(wrappedValue: ObjectType) {
    self.wrappedValue = wrappedValue
  }
}

extension ObservedObject: PommeDynamicProperty {
  /// The view updates when its object is about to change.
  package mutating func pommeAttach(to storage: ViewStorage, slot: Int, environment: EnvironmentValues) {
    let id = ObjectIdentifier(wrappedValue)
    guard storage.observing[slot]?.object != id else { return }
    let changed = storage.changed
    let subscription = wrappedValue.objectWillChange.sink { _ in changed() }
    storage.observing[slot] = (id, subscription)
  }
}

// pomme: in extension SwiftUICore.DynamicProperty
extension DynamicProperty {
  // pomme: replaces extension SwiftUICore.DynamicProperty :: public mutating func update()
  public mutating func update() {}
}

/// Give a view's dynamic properties (its stored properties that are, such as
/// `@State`) their storage, in the order the view declares them.
package func attachDynamicProperties<V>(_ view: inout V, to storage: ViewStorage, environment: EnvironmentValues) {
  let count = _pommeFieldCount(V.self)
  guard count > 0 else { return }
  withUnsafeMutableBytes(of: &view) { bytes in
    for i in 0..<count {
      var field = _PommeFieldMetadata()
      let type = _pommeFieldType(V.self, index: i, fieldMetadata: &field)
      field.freeName?(field.name)
      guard let property = type as? any PommeDynamicProperty.Type else { continue }
      let at = bytes.baseAddress! + _pommeFieldOffset(V.self, index: i)
      func attach<P: PommeDynamicProperty>(_: P.Type) {
        at.assumingMemoryBound(to: P.self).pointee.pommeAttach(to: storage, slot: i, environment: environment)
      }
      attach(property)
    }
  }
}

// The Swift runtime's field reflection (what Mirror is built on): a struct's
// stored properties, with their types and offsets.
private struct _PommeFieldMetadata {
  var name: UnsafePointer<CChar>? = nil
  var freeName: (@convention(c) (UnsafePointer<CChar>?) -> Void)? = nil
  var isStrong = false
  var isVar = false
}

@_silgen_name("swift_reflectionMirror_recursiveCount")
private func _pommeFieldCount(_ type: Any.Type) -> Int

@_silgen_name("swift_reflectionMirror_recursiveChildMetadata")
private func _pommeFieldType(_ type: Any.Type, index: Int, fieldMetadata: UnsafeMutablePointer<_PommeFieldMetadata>) -> Any.Type

@_silgen_name("swift_reflectionMirror_recursiveChildOffset")
private func _pommeFieldOffset(_ type: Any.Type, index: Int) -> Int
