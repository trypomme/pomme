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

  // pomme: replaces public struct State<Value> : SwiftUICore.DynamicProperty :: public var projectedValue: SwiftUICore.Binding<Value>
  /// `$state`: a binding to the state's value (a constant outside a view in the graph).
  public var projectedValue: Binding<Value> {
    Binding(location: _location ?? ConstantLocation(_value))
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

// ---- @EnvironmentObject ----

/// Where `.environmentObject(_:)` keeps an object: by its type.
package struct PommeObjectKey<T>: Hashable {
  package init() {}
}

extension EnvironmentValues {
  package subscript<T>(pommeObject key: PommeObjectKey<T>) -> T? {
    get { pommeByKey[ObjectIdentifier(PommeObjectKey<T>.self)] as? T }
    set { pommeByKey[ObjectIdentifier(PommeObjectKey<T>.self)] = newValue }
  }
}

// pomme: in extension Combine.ObservableObject
extension ObservableObject {
  // pomme: replaces extension Combine.ObservableObject :: package static var environmentStore: Swift.WritableKeyPath<SwiftUICore.EnvironmentValues, Self?>
  @usableFromInline
  package static var environmentStore: WritableKeyPath<EnvironmentValues, Self?> {
    \EnvironmentValues[pommeObject: PommeObjectKey<Self>()]
  }
}

// pomme: in public struct EnvironmentObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject
extension EnvironmentObject {
  // pomme: replaces public struct EnvironmentObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject :: public init()
  @MainActor @preconcurrency
  public init() {
    _store = nil
  }

  // pomme: replaces public struct EnvironmentObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject :: internal func error() -> Swift.Never
  @usableFromInline
  @MainActor @preconcurrency internal func error() -> Never {
    fatalError("No ObservableObject of type \(ObjectType.self) found. A View.environmentObject(_:) for \(ObjectType.self) may be missing as an ancestor of this view.")
  }

  // pomme: replaces public struct EnvironmentObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject :: public var projectedValue: SwiftUICore.EnvironmentObject<ObjectType>.Wrapper
  @MainActor @preconcurrency public var projectedValue: Wrapper {
    Wrapper(root: wrappedValue)
  }
}

// pomme: in public struct EnvironmentObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject > public struct Wrapper
extension EnvironmentObject.Wrapper {
  // pomme: replaces public struct EnvironmentObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject > public struct Wrapper :: public subscript<Subject>(dynamicMember keyPath: Swift.ReferenceWritableKeyPath<ObjectType, Subject>) -> SwiftUICore.Binding<Subject>
  @MainActor @preconcurrency public subscript<Subject>(dynamicMember keyPath: ReferenceWritableKeyPath<ObjectType, Subject>) -> Binding<Subject> {
    let root = root
    return Binding(get: { root[keyPath: keyPath] }, set: { root[keyPath: keyPath] = $0 })
  }
}

extension EnvironmentObject: PommeDynamicProperty {
  /// The object from the environment; the view updates when it's about to change.
  package mutating func pommeAttach(to storage: ViewStorage, slot: Int, environment: EnvironmentValues) {
    _store = environment[pommeObject: PommeObjectKey<ObjectType>()]
    guard let object = _store else { return }
    let id = ObjectIdentifier(object)
    guard storage.observing[slot]?.object != id else { return }
    let changed = storage.changed
    let subscription = object.objectWillChange.sink { _ in changed() }
    storage.observing[slot] = (id, subscription)
  }
}

// pomme: in public struct ObservedObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject
extension ObservedObject {
  // pomme: replaces public struct ObservedObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject :: public var projectedValue: SwiftUICore.ObservedObject<ObjectType>.Wrapper
  @MainActor @preconcurrency public var projectedValue: Wrapper {
    Wrapper(root: wrappedValue)
  }
}

// pomme: in public struct ObservedObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject > public struct Wrapper
extension ObservedObject.Wrapper {
  // pomme: replaces public struct ObservedObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject > public struct Wrapper :: public subscript<Subject>(dynamicMember keyPath: Swift.ReferenceWritableKeyPath<ObjectType, Subject>) -> SwiftUICore.Binding<Subject>
  @MainActor @preconcurrency public subscript<Subject>(dynamicMember keyPath: ReferenceWritableKeyPath<ObjectType, Subject>) -> Binding<Subject> {
    let root = root
    return Binding(get: { root[keyPath: keyPath] }, set: { root[keyPath: keyPath] = $0 })
  }
}

// ---- @StateObject ----

// pomme: in public struct StateObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject
extension StateObject {
  // pomme: replaces public struct StateObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject :: public var wrappedValue: ObjectType
  /// Outside a view in the graph, a new object each time (as SwiftUI warns).
  @MainActor @preconcurrency public var wrappedValue: ObjectType {
    switch storage {
    case .object(let o): return o.wrappedValue
    case .initially(let make): return make()
    }
  }

  // pomme: replaces public struct StateObject<ObjectType> : SwiftUICore.DynamicProperty where ObjectType : Combine.ObservableObject :: public var projectedValue: SwiftUICore.ObservedObject<ObjectType>.Wrapper
  @MainActor @preconcurrency public var projectedValue: ObservedObject<ObjectType>.Wrapper {
    ObservedObject<ObjectType>.Wrapper(root: wrappedValue)
  }
}

/// A view's state object, made once.
final class StoredObject<ObjectType: ObservableObject>: AnyLocationBase, @unchecked Sendable {
  let object: ObjectType
  init(_ object: ObjectType) {
    self.object = object
    super.init()
  }
}

extension StateObject: PommeDynamicProperty {
  /// The object, made the first time the view is in the graph and kept; the
  /// view updates when it's about to change.
  package mutating func pommeAttach(to storage: ViewStorage, slot: Int, environment: EnvironmentValues) {
    let stored: StoredObject<ObjectType>
    if let s = storage.locations[slot] as? StoredObject<ObjectType> {
      stored = s
    } else {
      guard case .initially(let make) = self.storage else { return }
      stored = StoredObject(make())
      storage.locations[slot] = stored
    }
    self.storage = .object(ObservedObject(wrappedValue: stored.object))
    let id = ObjectIdentifier(stored.object)
    guard storage.observing[slot]?.object != id else { return }
    let changed = storage.changed
    let subscription = stored.object.objectWillChange.sink { _ in changed() }
    storage.observing[slot] = (id, subscription)
  }
}

// ---- withAnimation, withTransaction ----

// pomme: replaces :: public func withAnimation<Result>(_ animation: SwiftUICore.Animation? = .default, _ body: () throws -> Result) rethrows -> Result
/// The changes it makes, shown at once (pomme's SwiftUI doesn't animate them yet).
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
@_originallyDefinedIn(module: "SwiftUI", iOS 18.0)
@_originallyDefinedIn(module: "SwiftUI", macOS 15.0)
@_originallyDefinedIn(module: "SwiftUI", tvOS 18.0)
@_originallyDefinedIn(module: "SwiftUI", watchOS 11.0)
public func withAnimation<Result>(_ animation: Animation? = .default, _ body: () throws -> Result) rethrows -> Result {
  try body()
}

// pomme: replaces :: public func withTransaction<Result>(_ transaction: SwiftUICore.Transaction, _ body: () throws -> Result) rethrows -> Result
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
@_originallyDefinedIn(module: "SwiftUI", iOS 18.0)
@_originallyDefinedIn(module: "SwiftUI", macOS 15.0)
@_originallyDefinedIn(module: "SwiftUI", tvOS 18.0)
@_originallyDefinedIn(module: "SwiftUI", watchOS 11.0)
public func withTransaction<Result>(_ transaction: Transaction, _ body: () throws -> Result) rethrows -> Result {
  try body()
}
