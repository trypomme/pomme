// @Published: a class's property that publishes its changes, and tells the
// object's `objectWillChange` before each one.

/// A published property's storage: its value and the subscriptions to it.
final class PublishedStorage<Value> {
  let relay = Relay<Value, Never>()
  var value: Value
  /// Tells the enclosing object it's about to change (known once the property is used through it).
  var willChange: (() -> Void)?
  /// What `assign(to:)` keeps going: as long as the property lives.
  var assigned: [AnyCancellable] = []

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
    get {
      let storage = object[keyPath: storageKeyPath].storage
      remember(object, in: storage)
      return storage.value
    }
    set {
      let storage = object[keyPath: storageKeyPath].storage
      remember(object, in: storage)
      storage.willChange?()
      storage.set(newValue)
    }
  }

  /// The enclosing object's change notice, for changes that don't come through it (`assign(to:)`).
  private static func remember<EnclosingSelf: AnyObject>(_ object: EnclosingSelf, in storage: PublishedStorage<Value>) {
    guard storage.willChange == nil, let observable = object as? any ObservableObject else { return }
    let publisher = observable.objectWillChange as any Combine.Publisher as? ObservableObjectPublisher
    storage.willChange = { [weak publisher] in publisher?.send() }
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

  /// `assign(to: &$property)`: each value the publisher gives becomes the property's.
  func pommeAssign<P: Combine.Publisher>(from publisher: P) where P.Output == Value, P.Failure == Never {
    let storage = storage
    storage.assigned.append(publisher.sink { [weak storage] value in
      storage?.willChange?()
      storage?.set(value)
    })
  }

  // pomme: replaces public struct Published<Value> > public struct Publisher : Combine.Publisher :: public func receive<S>(subscriber: S) where Value == S.Input, S : Combine.Subscriber, S.Failure == Swift.Never
  public func receive<S>(subscriber: S) where Value == S.Input, S: Subscriber, S.Failure == Never {
    storage.relay.attach(subscriber, pending: storage.value)
  }
}
