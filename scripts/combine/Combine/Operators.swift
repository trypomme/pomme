// Operators: type erasure (eraseToAnyPublisher), map, receive(on:), and
// assign(to:) a @Published property's publisher.

// pomme: in internal class PublisherBoxBase<Output, Failure> : Combine.Publisher where Failure : Swift.Error
extension PublisherBoxBase {
  // pomme: replaces internal class PublisherBoxBase<Output, Failure> : Combine.Publisher where Failure : Swift.Error :: internal func receive<S>(subscriber: S) where Output == S.Input, Failure == S.Failure, S : Combine.Subscriber
  /// (Each box overrides it: PublisherBox passes the subscriber to its publisher.)
  @usableFromInline
  internal func receive<S>(subscriber: S) where Output == S.Input, Failure == S.Failure, S: Subscriber {
    fatalError("PublisherBoxBase.receive(subscriber:): a subclass provides it")
  }
}

extension Publisher {
  // pomme: replaces extension Combine.Publisher :: public func eraseToAnyPublisher() -> Combine.AnyPublisher<Self.Output, Self.Failure>
  public func eraseToAnyPublisher() -> AnyPublisher<Output, Failure> {
    AnyPublisher(self)
  }

  // pomme: replaces extension Combine.Publisher :: public func map<T>(_ transform: @escaping (Self.Output) -> T) -> Combine.Publishers.Map<Self, T>
  public func map<T>(_ transform: @escaping (Output) -> T) -> Publishers.Map<Self, T> {
    Publishers.Map(upstream: self, transform: transform)
  }

  // pomme: replaces extension Combine.Publisher :: public func receive<S>(on scheduler: S, options: S.SchedulerOptions? = nil) -> Combine.Publishers.ReceiveOn<Self, S> where S : Combine.Scheduler
  public func receive<S>(on scheduler: S, options: S.SchedulerOptions? = nil) -> Publishers.ReceiveOn<Self, S> where S: Scheduler {
    Publishers.ReceiveOn(upstream: self, scheduler: scheduler, options: options)
  }
}

extension Publisher where Failure == Never {
  // pomme: replaces extension Combine.Publisher where Self.Failure == Swift.Never :: public func assign(to published: inout Combine.Published<Self.Output>.Publisher)
  /// Each value goes to the @Published property the publisher is of (for as
  /// long as the property's owner lives, as the subscription is tied to it).
  public func assign(to published: inout Published<Output>.Publisher) {
    published.pommeAssign(from: self)
  }
}

// ---- Map ----

// pomme: in public struct Map<Upstream, Output> : Combine.Publisher where Upstream : Combine.Publisher
extension Publishers.Map {
  // pomme: replaces public struct Map<Upstream, Output> : Combine.Publisher where Upstream : Combine.Publisher :: public init(upstream: Upstream, transform: @escaping (Upstream.Output) -> Output)
  public init(upstream: Upstream, transform: @escaping (Upstream.Output) -> Output) {
    _pommeStorage = ()
    self.upstream = upstream
    self.transform = transform
  }

  // pomme: replaces public struct Map<Upstream, Output> : Combine.Publisher where Upstream : Combine.Publisher :: public func receive<S>(subscriber: S) where Output == S.Input, S : Combine.Subscriber, Upstream.Failure == S.Failure
  public func receive<S>(subscriber: S) where Output == S.Input, S: Subscriber, Upstream.Failure == S.Failure {
    upstream.subscribe(MapSubscriber(downstream: subscriber, transform: transform))
  }
}

/// Each value transformed on its way down; demand and completion as they are.
struct MapSubscriber<Input, Downstream: Subscriber>: Subscriber {
  typealias Failure = Downstream.Failure
  let downstream: Downstream
  let transform: (Input) -> Downstream.Input
  let combineIdentifier = CombineIdentifier()

  func receive(subscription: Subscription) { downstream.receive(subscription: subscription) }
  func receive(_ input: Input) -> Subscribers.Demand { downstream.receive(transform(input)) }
  func receive(completion: Subscribers.Completion<Failure>) { downstream.receive(completion: completion) }
}

// ---- ReceiveOn ----

// pomme: in public struct ReceiveOn<Upstream, Context> : Combine.Publisher where Upstream : Combine.Publisher, Context : Combine.Scheduler
extension Publishers.ReceiveOn {
  // pomme: replaces public struct ReceiveOn<Upstream, Context> : Combine.Publisher where Upstream : Combine.Publisher, Context : Combine.Scheduler :: public init(upstream: Upstream, scheduler: Context, options: Context.SchedulerOptions?)
  public init(upstream: Upstream, scheduler: Context, options: Context.SchedulerOptions?) {
    _pommeStorage = ()
    self.upstream = upstream
    self.scheduler = scheduler
    self.options = options
  }

  // pomme: replaces public struct ReceiveOn<Upstream, Context> : Combine.Publisher where Upstream : Combine.Publisher, Context : Combine.Scheduler :: public func receive<S>(subscriber: S) where S : Combine.Subscriber, Upstream.Failure == S.Failure, Upstream.Output == S.Input
  public func receive<S>(subscriber: S) where S: Subscriber, Upstream.Failure == S.Failure, Upstream.Output == S.Input {
    upstream.subscribe(ReceiveOnSubscriber(downstream: subscriber, scheduler: scheduler, options: options))
  }
}

/// Values and completion delivered on the scheduler; the subscription as it is.
struct ReceiveOnSubscriber<Downstream: Subscriber, Context: Scheduler>: Subscriber {
  typealias Input = Downstream.Input
  typealias Failure = Downstream.Failure
  let downstream: Downstream
  let scheduler: Context
  let options: Context.SchedulerOptions?
  let combineIdentifier = CombineIdentifier()

  func receive(subscription: Subscription) { downstream.receive(subscription: subscription) }

  func receive(_ input: Input) -> Subscribers.Demand {
    let downstream = downstream
    scheduler.schedule(options: options) { _ = downstream.receive(input) }
    return .none
  }

  func receive(completion: Subscribers.Completion<Failure>) {
    let downstream = downstream
    scheduler.schedule(options: options) { downstream.receive(completion: completion) }
  }
}
