// Demand, cancellation, and the sink subscriber.

// pomme: in public struct Demand : Swift.Equatable, Swift.Comparable, Swift.Hashable, Swift.Codable, Swift.CustomStringConvertible
extension Subscribers.Demand {
  // pomme: replaces public struct Demand : Swift.Equatable, Swift.Comparable, Swift.Hashable, Swift.Codable, Swift.CustomStringConvertible :: public static let unlimited: Combine.Subscribers.Demand
  public static let unlimited = Subscribers.Demand(rawValue: UInt(Int.max) + 1)
  // pomme: replaces public struct Demand : Swift.Equatable, Swift.Comparable, Swift.Hashable, Swift.Codable, Swift.CustomStringConvertible :: public static let none: Combine.Subscribers.Demand
  public static let none = Subscribers.Demand(rawValue: 0)

  // pomme: replaces public struct Demand : Swift.Equatable, Swift.Comparable, Swift.Hashable, Swift.Codable, Swift.CustomStringConvertible :: public var description: Swift.String
  public var description: String { self == .unlimited ? "unlimited" : "max(\(rawValue))" }

  // pomme: replaces public struct Demand : Swift.Equatable, Swift.Comparable, Swift.Hashable, Swift.Codable, Swift.CustomStringConvertible :: public static func == (a: Combine.Subscribers.Demand, b: Combine.Subscribers.Demand) -> Swift.Bool
  public static func == (a: Subscribers.Demand, b: Subscribers.Demand) -> Bool { a.rawValue == b.rawValue }

  // pomme: replaces public struct Demand : Swift.Equatable, Swift.Comparable, Swift.Hashable, Swift.Codable, Swift.CustomStringConvertible :: public func hash(into hasher: inout Swift.Hasher)
  public func hash(into hasher: inout Hasher) { hasher.combine(rawValue) }

  // pomme: replaces public struct Demand : Swift.Equatable, Swift.Comparable, Swift.Hashable, Swift.Codable, Swift.CustomStringConvertible :: public var hashValue: Swift.Int
  public var hashValue: Int { rawValue.hashValue }

  // pomme: replaces public struct Demand : Swift.Equatable, Swift.Comparable, Swift.Hashable, Swift.Codable, Swift.CustomStringConvertible :: public static func * (lhs: Combine.Subscribers.Demand, rhs: Swift.Int) -> Combine.Subscribers.Demand
  /// Unlimited stays unlimited; an overflow is unlimited.
  public static func * (lhs: Subscribers.Demand, rhs: Int) -> Subscribers.Demand {
    precondition(rhs >= 0, "a demand can't be multiplied by a negative number")
    if lhs == .unlimited { return .unlimited }
    let r = Int(lhs.rawValue).multipliedReportingOverflow(by: rhs)
    return r.overflow ? .unlimited : .max(r.partialValue)
  }

  // pomme: replaces public struct Demand : Swift.Equatable, Swift.Comparable, Swift.Hashable, Swift.Codable, Swift.CustomStringConvertible :: public init(from decoder: any Swift.Decoder) throws
  /// Encoded as its raw value.
  public init(from decoder: any Decoder) throws {
    self.init(rawValue: try decoder.singleValueContainer().decode(UInt.self))
  }

  // pomme: replaces public struct Demand : Swift.Equatable, Swift.Comparable, Swift.Hashable, Swift.Codable, Swift.CustomStringConvertible :: public func encode(to encoder: any Swift.Encoder) throws
  public func encode(to encoder: any Encoder) throws {
    var c = encoder.singleValueContainer()
    try c.encode(rawValue)
  }
}

// ---- Cancellation ----

// pomme: in final public class AnyCancellable : Combine.Cancellable, Swift.Hashable
extension AnyCancellable {
  private var body: (() -> Void)?

  // pomme: replaces final public class AnyCancellable : Combine.Cancellable, Swift.Hashable :: public init(_ cancel: @escaping () -> Swift.Void)
  public init(_ cancel: @escaping () -> Void) {
    body = cancel
  }

  // pomme: replaces final public class AnyCancellable : Combine.Cancellable, Swift.Hashable :: public init<C>(_ canceller: C) where C : Combine.Cancellable
  public init<C>(_ canceller: C) where C: Cancellable {
    body = canceller.cancel
  }

  // pomme: replaces final public class AnyCancellable : Combine.Cancellable, Swift.Hashable :: deinit
  /// Released, it cancels.
  deinit {
    body?()
  }

  // pomme: replaces final public class AnyCancellable : Combine.Cancellable, Swift.Hashable :: final public func cancel()
  /// Cancel, once.
  final public func cancel() {
    let b = body
    body = nil
    b?()
  }

  // pomme: replaces final public class AnyCancellable : Combine.Cancellable, Swift.Hashable :: final public func hash(into hasher: inout Swift.Hasher)
  final public func hash(into hasher: inout Hasher) { hasher.combine(ObjectIdentifier(self)) }

  // pomme: replaces final public class AnyCancellable : Combine.Cancellable, Swift.Hashable :: public static func == (lhs: Combine.AnyCancellable, rhs: Combine.AnyCancellable) -> Swift.Bool
  public static func == (lhs: AnyCancellable, rhs: AnyCancellable) -> Bool { lhs === rhs }

  // pomme: replaces final public class AnyCancellable : Combine.Cancellable, Swift.Hashable :: final public var hashValue: Swift.Int
  final public var hashValue: Int { ObjectIdentifier(self).hashValue }
}

extension AnyCancellable {
  // pomme: replaces extension Combine.AnyCancellable :: final public func store<C>(in collection: inout C) where C : Swift.RangeReplaceableCollection, C.Element == Combine.AnyCancellable
  final public func store<C>(in collection: inout C) where C: RangeReplaceableCollection, C.Element == AnyCancellable {
    collection.append(self)
  }

  // pomme: replaces extension Combine.AnyCancellable :: final public func store(in set: inout Swift.Set<Combine.AnyCancellable>)
  final public func store(in set: inout Set<AnyCancellable>) {
    set.insert(self)
  }
}

extension Cancellable {
  // pomme: replaces extension Combine.Cancellable :: public func store<C>(in collection: inout C) where C : Swift.RangeReplaceableCollection, C.Element == Combine.AnyCancellable
  public func store<C>(in collection: inout C) where C: RangeReplaceableCollection, C.Element == AnyCancellable {
    AnyCancellable(self).store(in: &collection)
  }

  // pomme: replaces extension Combine.Cancellable :: public func store(in set: inout Swift.Set<Combine.AnyCancellable>)
  public func store(in set: inout Set<AnyCancellable>) {
    AnyCancellable(self).store(in: &set)
  }
}

// ---- Subscribing ----

extension Publisher {
  // pomme: replaces extension Combine.Publisher :: public func subscribe<S>(_ subscriber: S) where S : Combine.Subscriber, Self.Failure == S.Failure, Self.Output == S.Input
  @inline(never)
  public func subscribe<S>(_ subscriber: S) where S: Subscriber, Self.Failure == S.Failure, Self.Output == S.Input {
    receive(subscriber: subscriber)
  }

  // pomme: replaces extension Combine.Publisher :: public func sink(receiveCompletion: @escaping (Combine.Subscribers.Completion<Self.Failure>) -> Swift.Void, receiveValue: @escaping (Self.Output) -> Swift.Void) -> Combine.AnyCancellable
  public func sink(receiveCompletion: @escaping (Subscribers.Completion<Failure>) -> Void, receiveValue: @escaping (Output) -> Void) -> AnyCancellable {
    let sink = Subscribers.Sink<Output, Failure>(receiveCompletion: receiveCompletion, receiveValue: receiveValue)
    subscribe(sink)
    return AnyCancellable(sink)
  }
}

extension Publisher where Failure == Never {
  // pomme: replaces extension Combine.Publisher where Self.Failure == Swift.Never :: public func sink(receiveValue: @escaping (Self.Output) -> Swift.Void) -> Combine.AnyCancellable
  public func sink(receiveValue: @escaping (Output) -> Void) -> AnyCancellable {
    sink(receiveCompletion: { _ in }, receiveValue: receiveValue)
  }
}

extension Subscriber where Input == () {
  // pomme: replaces extension Combine.Subscriber where Self.Input == () :: public func receive() -> Combine.Subscribers.Demand
  public func receive() -> Subscribers.Demand {
    receive(())
  }
}

// pomme: in final public class Sink<Input, Failure> : Combine.Subscriber, Combine.Cancellable, Swift.CustomStringConvertible, Swift.CustomReflectable, Swift.CustomPlaygroundDisplayConvertible where Failure : Swift.Error
extension Subscribers.Sink {
  // pomme: replaces final public class Sink<Input, Failure> : Combine.Subscriber, Combine.Cancellable, Swift.CustomStringConvertible, Swift.CustomReflectable, Swift.CustomPlaygroundDisplayConvertible where Failure : Swift.Error :: final public var receiveValue: (Input) -> Swift.Void
  final public let receiveValue: (Input) -> Void
  // pomme: replaces final public class Sink<Input, Failure> : Combine.Subscriber, Combine.Cancellable, Swift.CustomStringConvertible, Swift.CustomReflectable, Swift.CustomPlaygroundDisplayConvertible where Failure : Swift.Error :: final public var receiveCompletion: (Combine.Subscribers.Completion<Failure>) -> Swift.Void
  final public let receiveCompletion: (Subscribers.Completion<Failure>) -> Void
  private var subscription: (any Subscription)?
  private let lock = Lock()

  // pomme: replaces final public class Sink<Input, Failure> : Combine.Subscriber, Combine.Cancellable, Swift.CustomStringConvertible, Swift.CustomReflectable, Swift.CustomPlaygroundDisplayConvertible where Failure : Swift.Error :: final public var description: Swift.String
  final public var description: String { "Sink" }
  // pomme: replaces final public class Sink<Input, Failure> : Combine.Subscriber, Combine.Cancellable, Swift.CustomStringConvertible, Swift.CustomReflectable, Swift.CustomPlaygroundDisplayConvertible where Failure : Swift.Error :: final public var customMirror: Swift.Mirror
  final public var customMirror: Mirror { Mirror(self, children: []) }
  // pomme: replaces final public class Sink<Input, Failure> : Combine.Subscriber, Combine.Cancellable, Swift.CustomStringConvertible, Swift.CustomReflectable, Swift.CustomPlaygroundDisplayConvertible where Failure : Swift.Error :: final public var playgroundDescription: Any
  final public var playgroundDescription: Any { description }

  // pomme: replaces final public class Sink<Input, Failure> : Combine.Subscriber, Combine.Cancellable, Swift.CustomStringConvertible, Swift.CustomReflectable, Swift.CustomPlaygroundDisplayConvertible where Failure : Swift.Error :: public init(receiveCompletion: @escaping (Combine.Subscribers.Completion<Failure>) -> Swift.Void, receiveValue: @escaping (Input) -> Swift.Void)
  public init(receiveCompletion: @escaping (Subscribers.Completion<Failure>) -> Void, receiveValue: @escaping (Input) -> Void) {
    self.receiveCompletion = receiveCompletion
    self.receiveValue = receiveValue
  }

  // pomme: replaces final public class Sink<Input, Failure> : Combine.Subscriber, Combine.Cancellable, Swift.CustomStringConvertible, Swift.CustomReflectable, Swift.CustomPlaygroundDisplayConvertible where Failure : Swift.Error :: deinit
  deinit {}

  // pomme: replaces final public class Sink<Input, Failure> : Combine.Subscriber, Combine.Cancellable, Swift.CustomStringConvertible, Swift.CustomReflectable, Swift.CustomPlaygroundDisplayConvertible where Failure : Swift.Error :: final public func receive(subscription: any Combine.Subscription)
  /// A sink asks for every value.
  final public func receive(subscription: any Subscription) {
    let first = lock { () -> Bool in
      guard self.subscription == nil else { return false }
      self.subscription = subscription
      return true
    }
    if first {
      subscription.request(.unlimited)
    } else {
      subscription.cancel()
    }
  }

  // pomme: replaces final public class Sink<Input, Failure> : Combine.Subscriber, Combine.Cancellable, Swift.CustomStringConvertible, Swift.CustomReflectable, Swift.CustomPlaygroundDisplayConvertible where Failure : Swift.Error :: final public func receive(_ value: Input) -> Combine.Subscribers.Demand
  final public func receive(_ value: Input) -> Subscribers.Demand {
    receiveValue(value)
    return .none
  }

  // pomme: replaces final public class Sink<Input, Failure> : Combine.Subscriber, Combine.Cancellable, Swift.CustomStringConvertible, Swift.CustomReflectable, Swift.CustomPlaygroundDisplayConvertible where Failure : Swift.Error :: final public func receive(completion: Combine.Subscribers.Completion<Failure>)
  final public func receive(completion: Subscribers.Completion<Failure>) {
    receiveCompletion(completion)
    lock { subscription = nil }
  }

  // pomme: replaces final public class Sink<Input, Failure> : Combine.Subscriber, Combine.Cancellable, Swift.CustomStringConvertible, Swift.CustomReflectable, Swift.CustomPlaygroundDisplayConvertible where Failure : Swift.Error :: final public func cancel()
  final public func cancel() {
    let s = lock { () -> (any Subscription)? in
      defer { subscription = nil }
      return subscription
    }
    s?.cancel()
  }
}
