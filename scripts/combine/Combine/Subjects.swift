// Subjects, ObservableObjectPublisher and observable objects: publishers that
// values are sent through, each to the subscriptions it has.

/// One subscriber's subscription to a relay: it gets values as it asks for
/// them (its demand), and a pending first value (a current value) as soon as
/// it asks for anything.
final class Conduit<Output, Failure: Error>: Subscription, CustomStringConvertible {
  private let lock = Lock()
  private var receiveValue: ((Output) -> Subscribers.Demand)?
  private var receiveCompletion: ((Subscribers.Completion<Failure>) -> Void)?
  private var demand = Subscribers.Demand.none
  private var pending: Output?
  private let detach: (Conduit) -> Void

  init<S: Subscriber>(_ subscriber: S, pending: Output?, detach: @escaping (Conduit) -> Void) where S.Input == Output, S.Failure == Failure {
    receiveValue = subscriber.receive(_:)
    receiveCompletion = subscriber.receive(completion:)
    self.pending = pending
    self.detach = detach
  }

  var description: String { "Conduit" }

  func request(_ more: Subscribers.Demand) {
    precondition(more > .none, "a demand must be positive")
    let first = lock { () -> Output? in
      demand += more
      defer { pending = nil }
      return pending
    }
    if let first {
      send(first)
    }
  }

  /// Send a value if the subscriber has asked for one.
  func send(_ value: Output) {
    let receive = lock { () -> ((Output) -> Subscribers.Demand)? in
      guard let r = receiveValue, demand > .none else { return nil }
      demand -= 1
      return r
    }
    guard let receive else { return }
    let more = receive(value)
    if more > .none {
      lock { demand += more }
    }
  }

  func finish(_ completion: Subscribers.Completion<Failure>) {
    let receive = lock { () -> ((Subscribers.Completion<Failure>) -> Void)? in
      defer { receiveValue = nil; receiveCompletion = nil }
      return receiveCompletion
    }
    receive?(completion)
  }

  func cancel() {
    let attached = lock { () -> Bool in
      defer { receiveValue = nil; receiveCompletion = nil }
      return receiveValue != nil
    }
    if attached {
      detach(self)
    }
  }
}

/// Values sent to every subscription, until a completion ends them all.
final class Relay<Output, Failure: Error> {
  private let lock = Lock()
  private var conduits: [Conduit<Output, Failure>] = []
  private var completion: Subscribers.Completion<Failure>?

  func attach<S: Subscriber>(_ subscriber: S, pending: Output? = nil) where S.Input == Output, S.Failure == Failure {
    let conduit = Conduit(subscriber, pending: pending) { [weak self] c in self?.remove(c) }
    let ended = lock { () -> Subscribers.Completion<Failure>? in
      if completion == nil { conduits.append(conduit) }
      return completion
    }
    subscriber.receive(subscription: conduit)
    if let ended {
      conduit.finish(ended)
    }
  }

  private func remove(_ conduit: Conduit<Output, Failure>) {
    lock { conduits.removeAll { $0 === conduit } }
  }

  func send(_ value: Output) {
    for c in lock({ completion == nil ? conduits : [] }) {
      c.send(value)
    }
  }

  func finish(_ completion: Subscribers.Completion<Failure>) {
    let all = lock { () -> [Conduit<Output, Failure>] in
      guard self.completion == nil else { return [] }
      self.completion = completion
      defer { conduits = [] }
      return conduits
    }
    for c in all {
      c.finish(completion)
    }
  }
}

// pomme: in final public class PassthroughSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error
extension PassthroughSubject {
  private let relay = Relay<Output, Failure>()

  // pomme: replaces final public class PassthroughSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: public init()
  public init() {}

  // pomme: replaces final public class PassthroughSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: deinit
  deinit {}

  // pomme: replaces final public class PassthroughSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: final public func send(subscription: any Combine.Subscription)
  /// Subscribed to an upstream publisher: ask it for everything.
  final public func send(subscription: any Subscription) {
    subscription.request(.unlimited)
  }

  // pomme: replaces final public class PassthroughSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: final public func receive<S>(subscriber: __owned S) where Output == S.Input, Failure == S.Failure, S : Combine.Subscriber
  final public func receive<S>(subscriber: __owned S) where Output == S.Input, Failure == S.Failure, S: Subscriber {
    relay.attach(subscriber)
  }

  // pomme: replaces final public class PassthroughSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: final public func send(_ input: Output)
  final public func send(_ input: Output) {
    relay.send(input)
  }

  // pomme: replaces final public class PassthroughSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: final public func send(completion: Combine.Subscribers.Completion<Failure>)
  final public func send(completion: Subscribers.Completion<Failure>) {
    relay.finish(completion)
  }
}

// pomme: in final public class CurrentValueSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error
extension CurrentValueSubject {
  private let relay = Relay<Output, Failure>()
  private let lock = Lock()
  private var current: Output

  // pomme: replaces final public class CurrentValueSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: final public var value: Output
  /// Setting it sends it.
  final public var value: Output {
    get { lock { current } }
    set { send(newValue) }
  }

  // pomme: replaces final public class CurrentValueSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: public init(_ value: Output)
  public init(_ value: Output) {
    current = value
  }

  // pomme: replaces final public class CurrentValueSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: deinit
  deinit {}

  // pomme: replaces final public class CurrentValueSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: final public func send(subscription: any Combine.Subscription)
  final public func send(subscription: any Subscription) {
    subscription.request(.unlimited)
  }

  // pomme: replaces final public class CurrentValueSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: final public func receive<S>(subscriber: S) where Output == S.Input, Failure == S.Failure, S : Combine.Subscriber
  /// A new subscriber gets the current value first.
  final public func receive<S>(subscriber: S) where Output == S.Input, Failure == S.Failure, S: Subscriber {
    relay.attach(subscriber, pending: value)
  }

  // pomme: replaces final public class CurrentValueSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: final public func send(_ input: Output)
  final public func send(_ input: Output) {
    lock { current = input }
    relay.send(input)
  }

  // pomme: replaces final public class CurrentValueSubject<Output, Failure> : Combine.Subject where Failure : Swift.Error :: final public func send(completion: Combine.Subscribers.Completion<Failure>)
  final public func send(completion: Subscribers.Completion<Failure>) {
    relay.finish(completion)
  }
}

// ---- Observable objects ----

// pomme: in final public class ObservableObjectPublisher : Combine.Publisher
extension ObservableObjectPublisher {
  private let relay = Relay<Void, Never>()

  // pomme: replaces final public class ObservableObjectPublisher : Combine.Publisher :: public init()
  public init() {}

  // pomme: replaces final public class ObservableObjectPublisher : Combine.Publisher :: deinit
  deinit {}

  // pomme: replaces final public class ObservableObjectPublisher : Combine.Publisher :: final public func receive<S>(subscriber: S) where S : Combine.Subscriber, S.Failure == Swift.Never, S.Input == ()
  final public func receive<S>(subscriber: S) where S: Subscriber, S.Failure == Never, S.Input == () {
    relay.attach(subscriber)
  }

  // pomme: replaces final public class ObservableObjectPublisher : Combine.Publisher :: final public func send()
  /// The object is about to change.
  final public func send() {
    relay.send(())
  }
}

/// The publishers of observable objects that use the default
/// `objectWillChange`, by object (made the first time it's asked for).
private let publishersLock = Lock()
private var publishers: [ObjectIdentifier: (object: Weak, publisher: ObservableObjectPublisher)] = [:]

private struct Weak {
  weak var object: AnyObject?
}

extension ObservableObject where ObjectWillChangePublisher == ObservableObjectPublisher {
  // pomme: replaces extension Combine.ObservableObject where Self.ObjectWillChangePublisher == Combine.ObservableObjectPublisher :: public var objectWillChange: Combine.ObservableObjectPublisher
  /// The object's own publisher: `@Published` properties send through it as they change.
  public var objectWillChange: ObservableObjectPublisher {
    publishersLock {
      let id = ObjectIdentifier(self)
      if let entry = publishers[id], entry.object.object === self {
        return entry.publisher
      }
      // (An entry for an object that's gone, whose address this one reuses, is replaced.)
      publishers = publishers.filter { $0.value.object.object != nil }
      let publisher = ObservableObjectPublisher()
      publishers[id] = (Weak(object: self), publisher)
      return publisher
    }
  }
}

@available(macOS 10.15, iOS 13.0, tvOS 13.0, watchOS 6.0, *)
extension Subject where Output == Void {
  // pomme: replaces extension Combine.Subject where Self.Output == () :: public func send()
  /// A subject of nothing but signals: send one.
  public func send() { send(()) }
}
