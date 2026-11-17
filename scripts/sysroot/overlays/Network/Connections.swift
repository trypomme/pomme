// pomme's Network overlay, part two: endpoints, parameters, connections,
// browsers and listeners, with the iOS 26 SDK's declarations (the names and
// signatures apps are compiled against), as Expo's dev launcher uses them to
// find Metro on the network.
//
// - NWConnection is TCP over BSD sockets (pomme's, through the host).
// - NWBrowser finds Bonjour services: the computer's network has them
//   (pomme_bonjour_browse, real mDNS on the host), and so do this app's own
//   listeners, as a phone sees its own advertisements.
// - NWListener advertises its service to the app's browsers. It doesn't
//   accept connections.

import Darwin
import Dispatch
import Foundation
import dnssd

@_silgen_name("pomme_bonjour_browse")
internal func _pomme_bonjour_browse(_ kind: UnsafePointer<CChar>, _ out: UnsafeMutablePointer<CChar>?, _ cap: Int) -> Int

// MARK: - Addresses

public protocol IPAddress: Sendable {
  var rawValue: Data { get }
  init?(_ rawValue: Data, _ interface: NWInterface?)
  init?(_ string: String)
  var interface: NWInterface? { get }
  var isLoopback: Bool { get }
  var isLinkLocal: Bool { get }
  var isMulticast: Bool { get }
}

public struct IPv4Address: IPAddress, Hashable, CustomDebugStringConvertible {
  public static let any = IPv4Address(Data([0, 0, 0, 0]))!
  public static let broadcast = IPv4Address(Data([255, 255, 255, 255]))!
  public static let loopback = IPv4Address(Data([127, 0, 0, 1]))!
  public static let allHostsGroup = IPv4Address(Data([224, 0, 0, 1]))!
  public static let allRoutersGroup = IPv4Address(Data([224, 0, 0, 2]))!
  public static let allReportsGroup = IPv4Address(Data([224, 0, 0, 22]))!
  public static let mdnsGroup = IPv4Address(Data([224, 0, 0, 251]))!

  private let bytes: Data
  public let interface: NWInterface?

  public var isLoopback: Bool { bytes.first == 127 }
  public var isLinkLocal: Bool { bytes.starts(with: [169, 254]) }
  public var isMulticast: Bool { (bytes.first ?? 0) & 0xf0 == 0xe0 }
  public var rawValue: Data { bytes }

  public init?(_ rawValue: Data, _ interface: NWInterface? = nil) {
    guard rawValue.count == 4 else { return nil }
    bytes = Data(rawValue)
    self.interface = interface
  }

  public init?(_ string: String) {
    var a = in_addr()
    guard inet_pton(AF_INET, string, &a) == 1 else { return nil }
    self.init(withUnsafeBytes(of: &a) { Data($0) })
  }

  public static func == (lhs: IPv4Address, rhs: IPv4Address) -> Bool { lhs.bytes == rhs.bytes }
  public func hash(into hasher: inout Hasher) { hasher.combine(bytes) }
  public var debugDescription: String { bytes.map(String.init).joined(separator: ".") }
}

public struct IPv6Address: IPAddress, Hashable, CustomDebugStringConvertible {
  public static let any = IPv6Address(Data(count: 16))!
  public static let broadcast = IPv6Address(Data(count: 16))!
  public static let loopback = IPv6Address(Data([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]))!
  public static let nodeLocalNodes = IPv6Address(Data([0xff, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]))!
  public static let linkLocalNodes = IPv6Address(Data([0xff, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]))!
  public static let linkLocalRouters = IPv6Address(Data([0xff, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]))!

  private let bytes: Data
  public let interface: NWInterface?

  public var isLoopback: Bool { self == .loopback }
  public var isLinkLocal: Bool { bytes.first == 0xfe && (bytes[bytes.startIndex + 1] & 0xc0) == 0x80 }
  public var isMulticast: Bool { bytes.first == 0xff }
  public var rawValue: Data { bytes }

  public init?(_ rawValue: Data, _ interface: NWInterface? = nil) {
    guard rawValue.count == 16 else { return nil }
    bytes = Data(rawValue)
    self.interface = interface
  }

  public init?(_ string: String) {
    var a = in6_addr()
    guard inet_pton(AF_INET6, string, &a) == 1 else { return nil }
    self.init(withUnsafeBytes(of: &a) { Data($0) })
  }

  public static func == (lhs: IPv6Address, rhs: IPv6Address) -> Bool { lhs.bytes == rhs.bytes }
  public func hash(into hasher: inout Hasher) { hasher.combine(bytes) }
  public var debugDescription: String {
    var a = in6_addr()
    _ = withUnsafeMutableBytes(of: &a) { bytes.copyBytes(to: $0) }
    var buf = [CChar](repeating: 0, count: Int(INET6_ADDRSTRLEN))
    inet_ntop(AF_INET6, &a, &buf, socklen_t(buf.count))
    return String(cString: buf)
  }
}

// MARK: - Endpoints

public enum NWEndpoint: Hashable, CustomDebugStringConvertible, Sendable {
  case hostPort(host: NWEndpoint.Host, port: NWEndpoint.Port)
  case service(name: String, type: String, domain: String, interface: NWInterface?)
  case unix(path: String)
  case url(_: URL)
  case opaque(_: nw_endpoint_t)

  public enum Host: Hashable, CustomDebugStringConvertible, ExpressibleByStringLiteral, Sendable {
    public typealias StringLiteralType = String
    case name(String, NWInterface?)
    case ipv4(IPv4Address)
    case ipv6(IPv6Address)

    public init(stringLiteral: StringLiteralType) { self.init(stringLiteral) }

    public init(_ string: String) {
      if let a = IPv4Address(string) {
        self = .ipv4(a)
      } else if let a = IPv6Address(string) {
        self = .ipv6(a)
      } else {
        self = .name(string, nil)
      }
    }

    public var interface: NWInterface? {
      switch self {
      case .name(_, let i): return i
      case .ipv4(let a): return a.interface
      case .ipv6(let a): return a.interface
      }
    }

    public var debugDescription: String {
      switch self {
      case .name(let n, _): return n
      case .ipv4(let a): return a.debugDescription
      case .ipv6(let a): return a.debugDescription
      }
    }
  }

  public struct Port: Hashable, CustomDebugStringConvertible, ExpressibleByIntegerLiteral, RawRepresentable, Sendable {
    public typealias IntegerLiteralType = UInt16
    public static let any: Port = 0
    public static let ssh: Port = 22
    public static let smtp: Port = 25
    public static let http: Port = 80
    public static let pop: Port = 110
    public static let imap: Port = 143
    public static let https: Port = 443
    public static let imaps: Port = 993
    public static let socks: Port = 1080

    private let port: UInt16
    public var rawValue: UInt16 { port }

    public init?(_ service: String) {
      if let n = UInt16(service) {
        port = n
      } else if let s = getservbyname(service, "tcp") {
        port = UInt16(bigEndian: UInt16(truncatingIfNeeded: s.pointee.s_port))
      } else {
        return nil
      }
    }

    public init(integerLiteral value: IntegerLiteralType) { port = value }
    public init?(rawValue: UInt16) { port = rawValue }
    public var debugDescription: String { String(port) }
  }

  public var interface: NWInterface? {
    switch self {
    case .hostPort(let host, _): return host.interface
    case .service(_, _, _, let i): return i
    default: return nil
    }
  }

  public var txtRecord: NWTXTRecord? { nil }

  public init?(copying endpoint: NWEndpoint, newPort: Port) {
    guard case .hostPort(let host, _) = endpoint else { return nil }
    self = .hostPort(host: host, port: newPort)
  }

  public static func == (lhs: NWEndpoint, rhs: NWEndpoint) -> Bool {
    switch (lhs, rhs) {
    case let (.hostPort(h1, p1), .hostPort(h2, p2)): return h1 == h2 && p1 == p2
    case let (.service(n1, t1, d1, i1), .service(n2, t2, d2, i2)): return n1 == n2 && t1 == t2 && d1 == d2 && i1 == i2
    case let (.unix(a), .unix(b)): return a == b
    case let (.url(a), .url(b)): return a == b
    case let (.opaque(a), .opaque(b)): return (a as AnyObject) === (b as AnyObject)
    default: return false
    }
  }

  public func hash(into hasher: inout Hasher) {
    switch self {
    case let .hostPort(h, p): hasher.combine(h); hasher.combine(p)
    case let .service(n, t, d, _): hasher.combine(n); hasher.combine(t); hasher.combine(d)
    case let .unix(p): hasher.combine(p)
    case let .url(u): hasher.combine(u)
    case let .opaque(e): hasher.combine(ObjectIdentifier(e as AnyObject))
    }
  }

  public var debugDescription: String {
    switch self {
    case let .hostPort(h, p):
      if case .ipv6 = h { return "[\(h.debugDescription)]:\(p.debugDescription)" }
      return "\(h.debugDescription):\(p.debugDescription)"
    case let .service(n, t, d, _): return "\(n).\(t)\(d)"
    case let .unix(p): return p
    case let .url(u): return u.absoluteString
    case .opaque: return "opaque"
    }
  }
}

// MARK: - Errors

public enum NWError: Error, CustomDebugStringConvertible, Equatable {
  case posix(POSIXErrorCode)
  case dns(DNSServiceErrorType)
  case tls(OSStatus)
  case wifiAware(Int32)

  public var debugDescription: String {
    switch self {
    case .posix(let c): return "POSIXErrorCode(rawValue: \(c.rawValue)): \(String(cString: strerror(c.rawValue)))"
    case .dns(let e): return "DNSServiceErrorType(\(e))"
    case .tls(let s): return "TLS error \(s)"
    case .wifiAware(let e): return "Wi-Fi Aware error \(e)"
    }
  }

  public static func == (a: NWError, b: NWError) -> Bool {
    switch (a, b) {
    case let (.posix(x), .posix(y)): return x == y
    case let (.dns(x), .dns(y)): return x == y
    case let (.tls(x), .tls(y)): return x == y
    case let (.wifiAware(x), .wifiAware(y)): return x == y
    default: return false
    }
  }
}

extension NWError: CustomNSError {
  public var errorCode: Int {
    switch self {
    case .posix(let c): return Int(c.rawValue)
    case .dns(let e): return Int(e)
    case .tls(let s): return Int(s)
    case .wifiAware(let e): return Int(e)
    }
  }

  public var errorUserInfo: [String: Any] { [NSLocalizedDescriptionKey: debugDescription] }

  public var _domain: String {
    switch self {
    case .posix: return "kNWErrorDomainPOSIX"
    case .dns: return "kNWErrorDomainDNS"
    case .tls: return "kNWErrorDomainTLS"
    case .wifiAware: return "kNWErrorDomainWiFiAware"
    }
  }
}

/// The calling thread's errno as an NWError.
private func posixError() -> NWError {
  .posix(POSIXErrorCode(rawValue: errno) ?? .EIO)
}

// MARK: - Parameters

@_objcRuntimeName(_TtC7Network13_NWParameters)
final public class NWParameters: CustomDebugStringConvertible, @unchecked Sendable {
  public enum ExpiredDNSBehavior: Sendable {
    case systemDefault
    case allow
    case prohibit
    case persistent
  }

  public init() {}

  final public class var tcp: NWParameters { NWParameters() }

  final public var requiredInterface: NWInterface?
  final public var preferNoProxies: Bool = false
  final public var allowLocalEndpointReuse: Bool = false
  final public var includePeerToPeer: Bool = false
  final public var expiredDNSBehavior: ExpiredDNSBehavior = .systemDefault

  final public var debugDescription: String { "tcp" }
}

extension NWParameters.ExpiredDNSBehavior: Equatable {}
extension NWParameters.ExpiredDNSBehavior: Hashable {}

// MARK: - TXT records

public struct NWTXTRecord: Sendable {
  public enum Entry: Hashable, CustomDebugStringConvertible, Sendable {
    case none
    case empty
    case string(String)
    case data(Data)

    public var data: Data? {
      switch self {
      case .none, .empty: return nil
      case .string(let s): return Data(s.utf8)
      case .data(let d): return d
      }
    }

    /// A value as a TXT record holds it: text when it's UTF-8.
    public init(_ data: Data?) {
      guard let data else { self = .none; return }
      if data.isEmpty {
        self = .empty
      } else if let s = String(data: data, encoding: .utf8) {
        self = .string(s)
      } else {
        self = .data(data)
      }
    }

    public var debugDescription: String {
      switch self {
      case .none: return "none"
      case .empty: return "empty"
      case .string(let s): return s
      case .data(let d): return "\(d.count) bytes"
      }
    }
  }

  /// In order, as the record has them.
  private var list: [(String, Entry)] = []

  public init(_ dictionary: [String: String] = [:]) {
    list = dictionary.sorted { $0.key < $1.key }.map { ($0.key, .string($0.value)) }
  }

  public init(_ dataDictionary: [String: Data]) {
    list = dataDictionary.sorted { $0.key < $1.key }.map { ($0.key, Entry($0.value)) }
  }

  public init(_ entryDictionary: [String: Entry]) {
    list = entryDictionary.sorted { $0.key < $1.key }.map { ($0.key, $0.value) }
  }

  /// The record's wire form: length-prefixed `key=value` strings.
  public init(_ data: Data) {
    var i = data.startIndex
    while i < data.endIndex {
      let n = Int(data[i])
      i = data.index(after: i)
      let end = data.index(i, offsetBy: n, limitedBy: data.endIndex) ?? data.endIndex
      let item = data[i..<end]
      i = end
      if let eq = item.firstIndex(of: UInt8(ascii: "=")) {
        list.append((String(decoding: item[..<eq], as: UTF8.self), Entry(Data(item[item.index(after: eq)...]))))
      } else {
        list.append((String(decoding: item, as: UTF8.self), .none))
      }
    }
  }

  internal init(pairs: [(String, Entry)]) { list = pairs }

  @discardableResult
  public mutating func removeEntry(key: String) -> Bool {
    let before = list.count
    list.removeAll { $0.0.caseInsensitiveCompare(key) == .orderedSame }
    return list.count != before
  }

  public func getEntry(for key: String) -> Entry? {
    list.first { $0.0.caseInsensitiveCompare(key) == .orderedSame }?.1
  }

  public var entries: [String: Entry] { Dictionary(list, uniquingKeysWith: { a, _ in a }) }

  public var data: Data {
    var out = Data()
    for (k, v) in list {
      var item = Data(k.utf8)
      if let d = v.data {
        item.append(UInt8(ascii: "="))
        item.append(d)
      } else if case .empty = v {
        item.append(UInt8(ascii: "="))
      }
      out.append(UInt8(min(item.count, 255)))
      out.append(item.prefix(255))
    }
    return out
  }

  @discardableResult
  public mutating func setEntry(_ entry: Entry, for key: String) -> Bool {
    removeEntry(key: key)
    list.append((key, entry))
    return true
  }

  public subscript(key: String) -> String? {
    get {
      if case .string(let s)? = getEntry(for: key) { return s }
      return nil
    }
    set {
      if let newValue { setEntry(.string(newValue), for: key) } else { removeEntry(key: key) }
    }
  }

  public var dictionary: [String: String] {
    var d: [String: String] = [:]
    for (k, v) in list { if case .string(let s) = v { d[k] = s } }
    return d
  }

  public var dataDictionary: [String: Data] {
    var d: [String: Data] = [:]
    for (k, v) in list { if let x = v.data { d[k] = x } }
    return d
  }
}

extension NWTXTRecord: Equatable {
  public static func == (lhs: NWTXTRecord, rhs: NWTXTRecord) -> Bool {
    lhs.list.count == rhs.list.count && zip(lhs.list, rhs.list).allSatisfy { $0.0 == $1.0 && $0.1 == $1.1 }
  }
}

// MARK: - Bonjour: the services found

/// A service as found: on the computer's network, or advertised by this app.
internal struct FoundService: Hashable {
  var name: String
  var type: String
  var domain: String
  var port: UInt16
  var addresses: [String]
  var txt: [String]

  var record: NWTXTRecord {
    NWTXTRecord(pairs: txt.map { item in
      guard let eq = item.firstIndex(of: "=") else { return (item, .none) }
      let hex = item[item.index(after: eq)...]
      var bytes: [UInt8] = []
      var i = hex.startIndex
      while i < hex.endIndex, let j = hex.index(i, offsetBy: 2, limitedBy: hex.endIndex), let b = UInt8(hex[i..<j], radix: 16) {
        bytes.append(b)
        i = j
      }
      return (String(item[..<eq]), NWTXTRecord.Entry(Data(bytes)))
    })
  }
}

internal enum Bonjour {
  private static let lock = NSLock()
  nonisolated(unsafe) private static var advertised: [ObjectIdentifier: FoundService] = [:]

  /// `_expo._tcp` from `_expo._tcp.` or `_expo._tcp`.
  static func kind(_ type: String) -> String { type.hasSuffix(".") ? String(type.dropLast()) : type }

  static func advertise(_ s: FoundService?, by owner: ObjectIdentifier) {
    lock.lock()
    advertised[owner] = s
    lock.unlock()
  }

  /// What the network and this app have of `type`, now.
  static func find(_ type: String) -> [FoundService] {
    let kind = kind(type)
    var found: [FoundService] = []
    let n = kind.withCString { _pomme_bonjour_browse($0, nil, 0) }
    if n > 0 {
      var buf = [CChar](repeating: 0, count: n + 1)
      _ = kind.withCString { _pomme_bonjour_browse($0, &buf, buf.count) }
      for line in String(cString: buf).split(separator: "\n") {
        let f = line.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
        guard f.count == 7, let port = UInt16(f[4]) else { continue }
        found.append(FoundService(name: f[0], type: f[1], domain: f[2], port: port,
                                  addresses: f[5].split(separator: ",").map(String.init),
                                  txt: f[6].split(separator: ";").map(String.init)))
      }
    }
    lock.lock()
    found += advertised.values.filter { $0.type == kind }
    lock.unlock()
    return found
  }
}

// MARK: - Browser

final public class NWBrowser: CustomDebugStringConvertible, @unchecked Sendable {
  public enum Descriptor: Sendable {
    case bonjour(type: String, domain: String?)
    case bonjourWithTXTRecord(type: String, domain: String?)
    case applicationService(name: String)
  }

  public struct Result: Hashable, Sendable {
    public let endpoint: NWEndpoint
    public let interfaces: [NWInterface]
    public let metadata: Metadata

    public enum Metadata: Equatable, CustomDebugStringConvertible, Sendable {
      case none
      case bonjour(_: NWTXTRecord)

      public static func == (lhs: Metadata, rhs: Metadata) -> Bool {
        switch (lhs, rhs) {
        case (.none, .none): return true
        case let (.bonjour(a), .bonjour(b)): return a == b
        default: return false
        }
      }

      public var debugDescription: String {
        switch self {
        case .none: return "none"
        case .bonjour(let r): return r.dictionary.description
        }
      }
    }

    public func hash(into hasher: inout Hasher) { hasher.combine(endpoint) }

    public static func == (lhs: Result, rhs: Result) -> Bool {
      lhs.endpoint == rhs.endpoint && lhs.interfaces == rhs.interfaces && lhs.metadata == rhs.metadata
    }

    public enum Change: Hashable, Sendable {
      case identical
      case added(Result)
      case removed(Result)
      case changed(old: Result, new: Result, flags: Flags)

      public struct Flags: OptionSet, Hashable, Sendable {
        public let rawValue: UInt8
        public init(rawValue: UInt8) { self.rawValue = rawValue }
        public static let identical = Flags(rawValue: 1 << 0)
        public static let interfaceAdded = Flags(rawValue: 1 << 1)
        public static let interfaceRemoved = Flags(rawValue: 1 << 2)
        public static let metadataChanged = Flags(rawValue: 1 << 3)
      }

      public init(between old: Result?, _ new: Result?) {
        switch (old, new) {
        case let (nil, n?): self = .added(n)
        case let (o?, nil): self = .removed(o)
        case let (o?, n?): self = o == n ? .identical : .changed(old: o, new: n, flags: o.metadata == n.metadata ? [] : .metadataChanged)
        default: self = .identical
        }
      }
    }
  }

  public enum State: Equatable, Sendable {
    case setup
    case ready
    case failed(NWError)
    case cancelled
    case waiting(NWError)
  }

  final public let descriptor: Descriptor
  final public let parameters: NWParameters
  final public private(set) var state: State = .setup
  final public private(set) var browseResults: Set<Result> = []
  final public private(set) var queue: DispatchQueue?
  @preconcurrency final public var stateUpdateHandler: (@Sendable (_ newState: State) -> Void)?
  @preconcurrency final public var browseResultsChangedHandler: (@Sendable (_ newResults: Set<Result>, _ changes: Set<Result.Change>) -> Void)?

  public init(for descriptor: Descriptor, using parameters: NWParameters) {
    self.descriptor = descriptor
    self.parameters = parameters
  }

  final public var debugDescription: String { "NWBrowser \(descriptor)" }

  private func set(_ s: State) {
    state = s
    stateUpdateHandler?(s)
  }

  /// Ready at once; then what's found, looked at again every second while it runs.
  final public func start(queue: DispatchQueue) {
    guard state == .setup else { return }
    self.queue = queue
    queue.async {
      self.set(.ready)
      self.look()
    }
  }

  private func look() {
    guard state == .ready, let queue else { return }
    let (type, txt): (String, Bool) = {
      switch descriptor {
      case .bonjour(let t, _): return (t, false)
      case .bonjourWithTXTRecord(let t, _): return (t, true)
      case .applicationService(let n): return (n, false)
      }
    }()
    let now = Set(Bonjour.find(type).map { s in
      Result(endpoint: .service(name: s.name, type: s.type, domain: s.domain, interface: nil), interfaces: [],
             metadata: txt ? .bonjour(s.record) : .none)
    })
    if now != browseResults {
      var changes = Set<Result.Change>()
      for r in now where !browseResults.contains(r) { changes.insert(.added(r)) }
      for r in browseResults where !now.contains(r) { changes.insert(.removed(r)) }
      browseResults = now
      browseResultsChangedHandler?(now, changes)
    }
    queue.asyncAfter(deadline: .now() + 1) { self.look() }
  }

  final public func cancel() {
    guard state != .cancelled else { return }
    let q = queue
    state = .cancelled
    q?.async { self.stateUpdateHandler?(.cancelled) }
  }
}

// MARK: - Listener

@_objcRuntimeName(_TtC7Network11_NWListener)
final public class NWListener: CustomDebugStringConvertible, @unchecked Sendable {
  public struct Service: Equatable, CustomDebugStringConvertible, Sendable {
    public let name: String?
    public let type: String
    public let domain: String?
    public let txtRecord: Data?
    public var noAutoRename: Bool = false

    public var txtRecordObject: NWTXTRecord? {
      get { txtRecord.map { NWTXTRecord($0) } }
      set { }
    }

    public init(name: String? = nil, type: String, domain: String? = nil, txtRecord: Data? = nil) {
      self.name = name
      self.type = type
      self.domain = domain
      self.txtRecord = txtRecord
    }

    public init(name: String? = nil, type: String, domain: String? = nil, txtRecord: NWTXTRecord) {
      self.init(name: name, type: type, domain: domain, txtRecord: txtRecord.data)
    }

    public init(applicationService: String) {
      self.init(type: applicationService)
    }

    public var debugDescription: String { "\(name ?? "")\(type)\(domain ?? "local.")" }
  }

  public enum State: Equatable, Sendable {
    case setup
    case waiting(NWError)
    case ready
    case failed(NWError)
    case cancelled
  }

  @preconcurrency final public var newConnectionHandler: (@Sendable (_ connection: NWConnection) -> Void)?
  final public private(set) var state: State = .setup
  @preconcurrency final public var stateUpdateHandler: (@Sendable (_ newState: State) -> Void)?
  final public let parameters: NWParameters
  final public var service: Service?
  final public private(set) var port: NWEndpoint.Port?

  nonisolated(unsafe) private static var nextPort: UInt16 = 52000

  public init(using: NWParameters, on: NWEndpoint.Port = .any) throws {
    parameters = using
    if on == .any {
      NWListener.nextPort &+= 1
      port = NWEndpoint.Port(rawValue: NWListener.nextPort)
    } else {
      port = on
    }
  }

  final public var debugDescription: String { "NWListener \(service?.debugDescription ?? "")" }

  /// Ready at once, its service (if any) advertised to this app's browsers.
  final public func start(queue: DispatchQueue) {
    if let s = service {
      var host = [CChar](repeating: 0, count: 256)
      gethostname(&host, host.count)
      Bonjour.advertise(FoundService(name: s.name ?? String(cString: host), type: Bonjour.kind(s.type), domain: s.domain ?? "local.",
                                     port: port?.rawValue ?? 0, addresses: ["127.0.0.1"], txt: []), by: ObjectIdentifier(self))
    }
    queue.async {
      self.state = .ready
      self.stateUpdateHandler?(.ready)
    }
  }

  final public func cancel() {
    Bonjour.advertise(nil, by: ObjectIdentifier(self))
    state = .cancelled
    stateUpdateHandler?(.cancelled)
  }

  deinit {
    Bonjour.advertise(nil, by: ObjectIdentifier(self))
  }
}

// MARK: - Connection

@_objcRuntimeName(_TtC7Network13_NWConnection)
final public class NWConnection: CustomDebugStringConvertible, @unchecked Sendable {
  public enum State: Equatable, Sendable {
    case setup
    case waiting(NWError)
    case preparing
    case ready
    case failed(NWError)
    case cancelled
  }

  @_hasMissingDesignatedInitializers
  @_objcRuntimeName(_TtCC7Network13_NWConnection14ContentContext)
  public class ContentContext: @unchecked Sendable {
    internal let final: Bool
    internal init(final: Bool) { self.final = final }
    public static let defaultMessage = ContentContext(final: false)
    public static let finalMessage = ContentContext(final: true)
  }

  public enum SendCompletion {
    @preconcurrency case contentProcessed(@Sendable (_ error: NWError?) -> Void)
    case idempotent
  }

  final public let endpoint: NWEndpoint
  final public let parameters: NWParameters
  final public private(set) var state: State = .setup
  @preconcurrency final public var stateUpdateHandler: (@Sendable (_ state: State) -> Void)?
  final public private(set) var currentPath: NWPath?
  final public private(set) var queue: DispatchQueue?

  /// The socket, once open; its work (opening, sending, receiving, each of
  /// which may wait) happens in order on a queue of its own.
  private var fd: Int32 = -1
  private let io = DispatchQueue(label: "pomme.nwconnection")

  public init(to: NWEndpoint, using: NWParameters) {
    endpoint = to
    parameters = using
  }

  final public var debugDescription: String { "NWConnection to \(endpoint.debugDescription)" }

  private func set(_ s: State) {
    queue?.async {
      guard self.state != .cancelled || s == .cancelled else { return }
      self.state = s
      self.stateUpdateHandler?(s)
    }
  }

  /// Where to connect: (address as text, port), in the order to try.
  private func targets() -> [(String, UInt16)] {
    switch endpoint {
    case let .hostPort(.ipv4(a), p): return [(a.debugDescription, p.rawValue)]
    case let .hostPort(.ipv6(a), p): return [(a.debugDescription, p.rawValue)]
    case let .hostPort(.name(n, _), p): return [(n, p.rawValue)]
    case let .service(name, type, _, _):
      guard let s = Bonjour.find(type).first(where: { $0.name == name }) else { return [] }
      // IPv4 first (the LAN address a phone would reach), then IPv6.
      let v4 = s.addresses.filter { !$0.contains(":") }, v6 = s.addresses.filter { $0.contains(":") }
      return (v4 + v6).map { ($0, s.port) }
    default: return []
    }
  }

  final public func start(queue: DispatchQueue) {
    guard self.queue == nil else { return }
    self.queue = queue
    set(.preparing)
    io.async { self.open() }
  }

  private func open() {
    var lastError = NWError.posix(.EHOSTUNREACH)
    for (host, port) in targets() {
      var hints = addrinfo()
      hints.ai_socktype = SOCK_STREAM
      var res: UnsafeMutablePointer<addrinfo>?
      guard getaddrinfo(host, String(port), &hints, &res) == 0, let first = res else { continue }
      defer { freeaddrinfo(res) }
      var ai: UnsafeMutablePointer<addrinfo>? = first
      while let a = ai {
        let s = socket(a.pointee.ai_family, a.pointee.ai_socktype, a.pointee.ai_protocol)
        if s >= 0 {
          if connect(s, a.pointee.ai_addr, a.pointee.ai_addrlen) == 0 {
            fd = s
            let remote = NWConnection.endpoint(of: a.pointee.ai_addr, port: port)
            let path = NWPath(remote: remote)
            queue?.async { self.currentPath = path }
            set(.ready)
            return
          }
          lastError = posixError()
          close(s)
        }
        ai = a.pointee.ai_next
      }
    }
    set(.failed(lastError))
  }

  private static func endpoint(of sa: UnsafeMutablePointer<sockaddr>, port: UInt16) -> NWEndpoint {
    if sa.pointee.sa_family == sa_family_t(AF_INET6) {
      let a = sa.withMemoryRebound(to: sockaddr_in6.self, capacity: 1) { $0.pointee.sin6_addr }
      let ip = withUnsafeBytes(of: a) { IPv6Address(Data($0)) } ?? .loopback
      return .hostPort(host: .ipv6(ip), port: NWEndpoint.Port(rawValue: port) ?? .any)
    }
    let a = sa.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { $0.pointee.sin_addr }
    let ip = withUnsafeBytes(of: a) { IPv4Address(Data($0)) } ?? .loopback
    return .hostPort(host: .ipv4(ip), port: NWEndpoint.Port(rawValue: port) ?? .any)
  }

  @preconcurrency final public func send(content: Data?, contentContext: ContentContext = .defaultMessage, isComplete: Bool = true, completion: SendCompletion) {
    io.async {
      var error: NWError?
      if let content, !content.isEmpty {
        let ok = content.withUnsafeBytes { (b: UnsafeRawBufferPointer) -> Bool in
          var off = 0
          while off < b.count {
            let n = Darwin.send(self.fd, b.baseAddress! + off, b.count - off, 0)
            if n <= 0 { return false }
            off += n
          }
          return true
        }
        if !ok { error = posixError() }
      }
      if isComplete && contentContext.final { shutdown(self.fd, SHUT_WR) }
      if case .contentProcessed(let done) = completion {
        self.queue?.async { done(error) }
      }
    }
  }

  @preconcurrency final public func receive(minimumIncompleteLength: Int, maximumLength: Int, completion: @escaping @Sendable (_ content: Data?, _ contentContext: ContentContext?, _ isComplete: Bool, _ error: NWError?) -> Void) {
    io.async {
      var buf = [UInt8](repeating: 0, count: max(maximumLength, 1))
      let n = recv(self.fd, &buf, buf.count, 0)
      let q = self.queue
      if n > 0 {
        let data = Data(buf[0..<n])
        q?.async { completion(data, .defaultMessage, false, nil) }
      } else if n == 0 {
        q?.async { completion(nil, .finalMessage, true, nil) }
      } else {
        let e = posixError()
        q?.async { completion(nil, nil, false, e) }
      }
    }
  }

  final public func cancel() {
    io.async {
      if self.fd >= 0 {
        close(self.fd)
        self.fd = -1
      }
    }
    set(.cancelled)
  }

  final public func forceCancel() { cancel() }
}
