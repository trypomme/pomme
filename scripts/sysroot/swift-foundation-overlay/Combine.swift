// pomme: Foundation's Combine publishers, as Apple's Foundation has them
// (`NotificationCenter.default.publisher(for:)`), with the same names and
// signatures, on pomme's Combine (scripts/combine).

import Combine
import Darwin
import PommeObjC

@available(macOS 10.15, iOS 13.0, tvOS 13.0, watchOS 6.0, *)
extension NSNotificationCenter {
    /// The notifications a center posts with a name (and object).
    public struct Publisher: Combine.Publisher {
        public typealias Output = Notification
        public typealias Failure = Never

        public let center: NSNotificationCenter
        public let name: NSNotificationName
        public let object: AnyObject?

        public init(center: NSNotificationCenter, name: NSNotificationName, object: AnyObject? = nil) {
            self.center = center
            self.name = name
            self.object = object
        }

        public func receive<S>(subscriber: S) where S: Subscriber, S.Failure == Never, S.Input == Notification {
            subscriber.receive(subscription: NotificationSubscription(center: center, name: name, object: object, downstream: subscriber))
        }
    }

    public func publisher(for name: NSNotificationName, object: AnyObject? = nil) -> NSNotificationCenter.Publisher {
        Publisher(center: self, name: name, object: object)
    }
}

/// A subscriber's observation of a center: notifications go to it while it
/// has demand for them; cancelling removes the observer.
private final class NotificationSubscription<Downstream: Subscriber>: Subscription
where Downstream.Input == Notification, Downstream.Failure == Never {
    private let lock = UnfairLock()
    private var demand = Subscribers.Demand.none
    private var downstream: Downstream?
    private var center: NSNotificationCenter?
    private var observer: AnyObject?

    init(center: NSNotificationCenter, name: NSNotificationName, object: AnyObject?, downstream: Downstream) {
        self.downstream = downstream
        self.center = center
        observer = center.addObserver(forName: name, object: object, queue: nil) { [weak self] note in
            self?.deliver(Notification._unconditionallyBridgeFromObjectiveC(note))
        } as AnyObject
    }

    private func deliver(_ note: Notification) {
        lock.lock()
        guard demand > 0, let downstream else { lock.unlock(); return }
        demand -= 1
        lock.unlock()
        let more = downstream.receive(note)
        lock.lock()
        demand += more
        lock.unlock()
    }

    func request(_ d: Subscribers.Demand) {
        lock.lock()
        demand += d
        lock.unlock()
    }

    func cancel() {
        lock.lock()
        let (center, observer) = (self.center, self.observer)
        self.center = nil
        self.observer = nil
        downstream = nil
        lock.unlock()
        if let center, let observer { center.removeObserver(observer) }
    }
}

/// os_unfair_lock, at a fixed address.
private final class UnfairLock {
    private let pointer: UnsafeMutablePointer<os_unfair_lock> = {
        let p = UnsafeMutablePointer<os_unfair_lock>.allocate(capacity: 1)
        p.initialize(to: os_unfair_lock())
        return p
    }()
    deinit { pointer.deallocate() }
    func lock() { os_unfair_lock_lock(pointer) }
    func unlock() { os_unfair_lock_unlock(pointer) }
}

// MARK: URLSession's async API

@available(macOS 12.0, iOS 15.0, tvOS 15.0, watchOS 8.0, *)
extension NSURLSession {
    /// The request's data and response, from a data task (pomme's), once it's done.
    public func data(for request: URLRequest, delegate: (any NSURLSessionTaskDelegate)? = nil) async throws -> (Data, NSURLResponse) {
        let ns = request._bridgeToObjectiveC()
        return try await withCheckedThrowingContinuation { continuation in
            dataTask(with: ns) { data, response, error in
                NSURLSession._pommeResume(continuation, data, response, error)
            }.resume()
        }
    }

    /// The URL's data and response, from a data task (pomme's), once it's done.
    public func data(from url: URL, delegate: (any NSURLSessionTaskDelegate)? = nil) async throws -> (Data, NSURLResponse) {
        let ns = url._bridgeToObjectiveC()
        return try await withCheckedThrowingContinuation { continuation in
            dataTask(with: ns) { data, response, error in
                NSURLSession._pommeResume(continuation, data, response, error)
            }.resume()
        }
    }

    private static func _pommeResume(_ continuation: CheckedContinuation<(Data, NSURLResponse), any Error>, _ data: NSData?, _ response: NSURLResponse?, _ error: NSError?) {
        if let error {
            continuation.resume(throwing: error)
        } else if let response {
            continuation.resume(returning: (data.map { Data._unconditionallyBridgeFromObjectiveC($0) } ?? Data(), response))
        } else {
            continuation.resume(throwing: URLError(.badServerResponse))
        }
    }
}
