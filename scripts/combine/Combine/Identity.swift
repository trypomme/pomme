// Identities of subscribers and subscriptions, and the lock Combine's shared
// state is kept under.

import Darwin

/// A lock for state that more than one thread can reach.
final class Lock {
  private let mutex: UnsafeMutablePointer<os_unfair_lock> = {
    let m = UnsafeMutablePointer<os_unfair_lock>.allocate(capacity: 1)
    m.initialize(to: os_unfair_lock())
    return m
  }()

  deinit {
    mutex.deinitialize(count: 1)
    mutex.deallocate()
  }

  func callAsFunction<T>(_ body: () throws -> T) rethrows -> T {
    os_unfair_lock_lock(mutex)
    defer { os_unfair_lock_unlock(mutex) }
    return try body()
  }
}

/// The next new identity. (Internal: the members using it are moved into
/// CombineIdentifier's declaration, in the generated source.)
let identifiers = Lock()
nonisolated(unsafe) var nextIdentifier: UInt64 = 1

// pomme: in public struct CombineIdentifier : Swift.Hashable, Swift.CustomStringConvertible
extension CombineIdentifier {
  private let value: UInt64

  // pomme: replaces public struct CombineIdentifier : Swift.Hashable, Swift.CustomStringConvertible :: public init()
  /// A new identity, unlike any other.
  public init() {
    _pommeStorage = ()
    value = identifiers {
      defer { nextIdentifier += 1 }
      return nextIdentifier
    }
  }

  // pomme: replaces public struct CombineIdentifier : Swift.Hashable, Swift.CustomStringConvertible :: public init(_ obj: Swift.AnyObject)
  /// An object's identity.
  public init(_ obj: AnyObject) {
    _pommeStorage = ()
    value = UInt64(UInt(bitPattern: ObjectIdentifier(obj).hashValue))
  }

  // pomme: replaces public struct CombineIdentifier : Swift.Hashable, Swift.CustomStringConvertible :: public var description: Swift.String
  public var description: String { "0x" + String(value, radix: 16) }

  // pomme: replaces public struct CombineIdentifier : Swift.Hashable, Swift.CustomStringConvertible :: public static func == (a: Combine.CombineIdentifier, b: Combine.CombineIdentifier) -> Swift.Bool
  public static func == (a: CombineIdentifier, b: CombineIdentifier) -> Bool { a.value == b.value }

  // pomme: replaces public struct CombineIdentifier : Swift.Hashable, Swift.CustomStringConvertible :: public func hash(into hasher: inout Swift.Hasher)
  public func hash(into hasher: inout Hasher) { hasher.combine(value) }

  // pomme: replaces public struct CombineIdentifier : Swift.Hashable, Swift.CustomStringConvertible :: public var hashValue: Swift.Int
  public var hashValue: Int { value.hashValue }
}

extension CustomCombineIdentifierConvertible where Self: AnyObject {
  // pomme: replaces extension Combine.CustomCombineIdentifierConvertible where Self : AnyObject :: public var combineIdentifier: Combine.CombineIdentifier
  public var combineIdentifier: CombineIdentifier { CombineIdentifier(self) }
}
