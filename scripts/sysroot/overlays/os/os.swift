// pomme: the parts of Apple's `os` Swift overlay that apps use most: Logger,
// the OSLogType constants and OSAllocatedUnfairLock. (Apple's isn't open
// source; these follow its public API.) Logging goes to pomme's os_log.

@_exported import os
@_exported import os.log

extension OSLogType {
    public static var `default`: OSLogType { OSLogType(rawValue: 0x00) }
    public static var info: OSLogType { OSLogType(rawValue: 0x01) }
    public static var debug: OSLogType { OSLogType(rawValue: 0x02) }
    public static var error: OSLogType { OSLogType(rawValue: 0x10) }
    public static var fault: OSLogType { OSLogType(rawValue: 0x11) }
}

public struct Logger: @unchecked Sendable {
    public let logObject: OSLog

    public init(subsystem: String, category: String) {
        logObject = OSLog(__subsystem: subsystem, category: category)
    }

    public init(_ logObj: OSLog) {
        logObject = logObj
    }

    public init() {
        logObject = OSLog(__subsystem: "", category: "")
    }
}

/// A lock with protected state. One guest thread runs at a time and threads
/// only switch at blocking points, so the critical section needs no lock.
public struct OSAllocatedUnfairLock<State>: @unchecked Sendable {
    final class Box {
        var state: State
        init(_ state: State) { self.state = state }
    }

    let box: Box

    public init(initialState: State) {
        box = Box(initialState)
    }

    public func withLock<R>(_ body: (inout State) throws -> R) rethrows -> R {
        try body(&box.state)
    }

    public func withLockUnchecked<R>(_ body: (inout State) throws -> R) rethrows -> R {
        try body(&box.state)
    }

    public func lock() {}
    public func unlock() {}
}

extension OSAllocatedUnfairLock where State == () {
    public init() {
        box = Box(())
    }
}
