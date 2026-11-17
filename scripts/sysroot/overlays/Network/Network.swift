// pomme's Network overlay: the Swift API of Network.framework for watching
// the network (NWPathMonitor, NWPath, NWInterface), with the iOS 26 SDK's
// interface. Apple's is closed source; this one is written from that
// interface, on the C API (nw_path_monitor_*, nw_path_*, nw_interface_*)
// that pomme implements, as Apple's Swift layer sits on its C one.
// Connections, listeners and browsers are in Connections.swift.

import Dispatch

public struct NWInterface: Hashable, CustomDebugStringConvertible, Sendable {
  public enum InterfaceType: Sendable {
    case other
    case wifi
    case cellular
    case wiredEthernet
    case loopback
  }

  public let type: InterfaceType
  public let name: String
  public let index: Int

  public var debugDescription: String { name }

  internal init(type: InterfaceType, name: String, index: Int) {
    self.type = type
    self.name = name
    self.index = index
  }

  internal init(_ i: nw_interface_t) {
    self.init(type: InterfaceType(i: nw_interface_get_type(i)), name: String(cString: nw_interface_get_name(i)), index: Int(nw_interface_get_index(i)))
  }
}

extension NWInterface.InterfaceType: Equatable {}
extension NWInterface.InterfaceType: Hashable {}

extension NWInterface.InterfaceType {
  internal init(i: nw_interface_type_t) {
    switch i {
    case nw_interface_type_wifi: self = .wifi
    case nw_interface_type_cellular: self = .cellular
    case nw_interface_type_wired: self = .wiredEthernet
    case nw_interface_type_loopback: self = .loopback
    default: self = .other
    }
  }

  internal var nw: nw_interface_type_t {
    switch self {
    case .other: return nw_interface_type_other
    case .wifi: return nw_interface_type_wifi
    case .cellular: return nw_interface_type_cellular
    case .wiredEthernet: return nw_interface_type_wired
    case .loopback: return nw_interface_type_loopback
    }
  }
}

public struct NWPath: Equatable, CustomDebugStringConvertible, Sendable {
  public enum Status: Sendable {
    case satisfied
    case unsatisfied
    case requiresConnection
  }

  public enum UnsatisfiedReason: Sendable {
    case notAvailable
    case cellularDenied
    case wifiDenied
    case localNetworkDenied
    case vpnInactive
  }

  public let status: Status
  public let availableInterfaces: [NWInterface]
  public let isExpensive: Bool
  public let supportsIPv4: Bool
  public let supportsIPv6: Bool
  public let supportsDNS: Bool
  internal let constrained: Bool
  internal let reason: UnsatisfiedReason
  public let localEndpoint: NWEndpoint?
  public let remoteEndpoint: NWEndpoint?

  public var isConstrained: Bool { constrained }

  public var unsatisfiedReason: UnsatisfiedReason { reason }

  public func usesInterfaceType(_ type: NWInterface.InterfaceType) -> Bool {
    availableInterfaces.contains { $0.type == type }
  }

  public var debugDescription: String {
    switch status {
    case .satisfied: return "satisfied (\(availableInterfaces.map(\.name).joined(separator: ", ")))"
    case .unsatisfied: return "unsatisfied"
    case .requiresConnection: return "requires connection"
    }
  }

  public static func == (lhs: NWPath, rhs: NWPath) -> Bool {
    lhs.status == rhs.status && lhs.availableInterfaces == rhs.availableInterfaces && lhs.isExpensive == rhs.isExpensive
      && lhs.constrained == rhs.constrained
  }

  internal init(_ p: nw_path_t) {
    switch nw_path_get_status(p) {
    case nw_path_status_satisfied: status = .satisfied
    case nw_path_status_satisfiable: status = .requiresConnection
    default: status = .unsatisfied
    }
    var list: [NWInterface] = []
    nw_path_enumerate_interfaces(p) { i in
      list.append(NWInterface(i))
      return true
    }
    availableInterfaces = list
    isExpensive = nw_path_is_expensive(p)
    constrained = nw_path_is_constrained(p)
    supportsIPv4 = nw_path_has_ipv4(p)
    supportsIPv6 = nw_path_has_ipv6(p)
    supportsDNS = nw_path_has_dns(p)
    reason = .notAvailable
    localEndpoint = nil
    remoteEndpoint = nil
  }

  /// A connection's path: satisfied, to `remote`.
  internal init(remote: NWEndpoint) {
    status = .satisfied
    availableInterfaces = []
    isExpensive = false
    constrained = false
    supportsIPv4 = true
    supportsIPv6 = true
    supportsDNS = true
    reason = .notAvailable
    localEndpoint = nil
    remoteEndpoint = remote
  }

  internal init() {
    status = .unsatisfied
    availableInterfaces = []
    isExpensive = false
    constrained = false
    supportsIPv4 = false
    supportsIPv6 = false
    supportsDNS = false
    reason = .notAvailable
    localEndpoint = nil
    remoteEndpoint = nil
  }
}

extension NWPath.Status: Equatable {}
extension NWPath.Status: Hashable {}
extension NWPath.UnsatisfiedReason: Equatable {}
extension NWPath.UnsatisfiedReason: Hashable {}

@_objcRuntimeName(_TtC7Network14_NWPathMonitor)
final public class NWPathMonitor: @unchecked Sendable {
  private let monitor: nw_path_monitor_t
  private var path = NWPath()
  private var handler: (@Sendable (_ newPath: NWPath) -> Void)?
  private var startedOn: DispatchQueue?

  public init() {
    monitor = nw_path_monitor_create()
    listen()
  }

  public init(requiredInterfaceType: NWInterface.InterfaceType) {
    monitor = nw_path_monitor_create_with_type(requiredInterfaceType.nw)
    listen()
  }

  public init(prohibitedInterfaceTypes: [NWInterface.InterfaceType]) {
    monitor = nw_path_monitor_create()
    for t in prohibitedInterfaceTypes {
      nw_path_monitor_prohibit_interface_type(monitor, t.nw)
    }
    listen()
  }

  private func listen() {
    nw_path_monitor_set_update_handler(monitor) { [weak self] p in
      guard let self else { return }
      let path = NWPath(p)
      self.path = path
      self.handler?(path)
    }
  }

  /// The latest path (unsatisfied until the monitor has started and heard one).
  final public var currentPath: NWPath {
    get { path }
    set { path = newValue }
  }

  @preconcurrency final public var pathUpdateHandler: (@Sendable (_ newPath: NWPath) -> Void)? {
    get { handler }
    set { handler = newValue }
  }

  final public func start(queue: DispatchQueue) {
    startedOn = queue
    nw_path_monitor_set_queue(monitor, queue)
    nw_path_monitor_start(monitor)
  }

  final public func cancel() {
    nw_path_monitor_cancel(monitor)
  }

  final public var queue: DispatchQueue? { startedOn }
}

extension NWPathMonitor: CustomDebugStringConvertible {
  final public var debugDescription: String { "NWPathMonitor (\(path.debugDescription))" }
}
