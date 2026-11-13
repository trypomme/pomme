// @Published: a class's property that publishes its changes, and tells the
// object's `objectWillChange` before each one.

/// A published property's storage: its value and the subscriptions to it.
final class PublishedStorage<Value> {
  let relay = Relay<Value, Never>()
  var value: Value

  init(_ value: Value) {
    self.value = value
  }

  /// Subscribers hear the new value before the property has it, as in Combine.
  func set(_ newValue: Value) {
    relay.send(newValue)
    value = newValue
  }
}

// pomme: in public struct Published<Value>
extension Published {
  private let storage: PublishedStorage<Value>

  // pomme: replaces public struct Published<Value> :: public init(initialValue: Value)
  public init(initialValue: Value) {
    _pommeStorage = ()
    storage = PublishedStorage(initialValue)
  }

  // pomme: replaces public struct Published<Value> :: public var wrappedValue: Value
  @available(*, unavailable, message: "@Published is only available on properties of classes")
  public var wrappedValue: Value {
    get { fatalError("@Published is only available on properties of classes") }
    set { fatalError("@Published is only available on properties of classes") }
  }

  // pomme: replaces public struct Published<Value> :: public static subscript<EnclosingSelf>(_enclosingInstance object: EnclosingSelf, wrapped wrappedKeyPath: Swift.ReferenceWritableKeyPath<EnclosingSelf, Value>, storage storageKeyPath: Swift.ReferenceWritableKeyPath<EnclosingSelf, Combine.Published<Value>>) -> Value where EnclosingSelf : AnyObject
  /// The property of `object`: setting it tells the object it's about to change.
  public static subscript<EnclosingSelf: AnyObject>(_enclosingInstance object: EnclosingSelf, wrapped wrappedKeyPath: ReferenceWritableKeyPath<EnclosingSelf, Value>, storage storageKeyPath: ReferenceWritableKeyPath<EnclosingSelf, Published<Value>>) -> Value {
    get { object[keyPath: storageKeyPath].storage.value }
    set {
      if let observable = object as? any ObservableObject {
        (observable.objectWillChange as any Combine.Publisher as? ObservableObjectPublisher)?.send()
      }
      object[keyPath: storageKeyPath].storage.set(newValue)
    }
  }

  // pomme: replaces public struct Published<Value> :: public var projectedValue: Combine.Published<Value>.Publisher
  /// `$property`: the property's values, the current one first.
  public var projectedValue: Published<Value>.Publisher {
    mutating get { Publisher(storage: storage) }
    @available(iOS 14.0, macOS 11.0, tvOS 14.0, watchOS 7.0, *)
    _modify {
      var p = Publisher(storage: storage)
      yield &p
    }
  }
}

// pomme: in public struct Published<Value> > public struct Publisher : Combine.Publisher
extension Published.Publisher {
  fileprivate let storage: PublishedStorage<Value>

  fileprivate init(storage: PublishedStorage<Value>) {
    _pommeStorage = ()
    self.storage = storage
  }

  // pomme: replaces public struct Published<Value> > public struct Publisher : Combine.Publisher :: public func receive<S>(subscriber: S) where Value == S.Input, S : Combine.Subscriber, S.Failure == Swift.Never
  public func receive<S>(subscriber: S) where Value == S.Input, S: Subscriber, S.Failure == Never {
    storage.relay.attach(subscriber, pending: storage.value)
  }
}
