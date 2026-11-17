// pomme: the Darwin overlay entry points apps import, with the signatures of
// swiftlang/swift stdlib/public/Platform/Platform.swift. C's open and fcntl
// are variadic, which Swift can't call; like Apple's overlay these call
// fixed-arity versions (pomme's libSystem reads them from registers).

@_exported import Darwin

@_silgen_name("open")
internal func _pomme_open(_ path: UnsafePointer<CChar>, _ oflag: Int32, _ mode: mode_t) -> Int32

@_silgen_name("fcntl")
internal func _pomme_fcntl(_ fd: Int32, _ cmd: Int32, _ value: Int32) -> Int32

public func open(_ path: UnsafePointer<CChar>, _ oflag: Int32) -> Int32 {
    _pomme_open(path, oflag, 0)
}

public func open(_ path: UnsafePointer<CChar>, _ oflag: Int32, _ mode: mode_t) -> Int32 {
    _pomme_open(path, oflag, mode)
}

public func fcntl(_ fd: Int32, _ cmd: Int32, _ value: Int32) -> Int32 {
    _pomme_fcntl(fd, cmd, value)
}

/// No error (MacTypes' `noErr`): Debug builds read it rather than inline it.
public var noErr: OSStatus { 0 }

public var MAP_FAILED: UnsafeMutableRawPointer! {
    UnsafeMutableRawPointer(bitPattern: -1)
}

public var S_IFMT: mode_t { mode_t(0o170000) }
public var S_IFIFO: mode_t { mode_t(0o010000) }
public var S_IFCHR: mode_t { mode_t(0o020000) }
public var S_IFDIR: mode_t { mode_t(0o040000) }
public var S_IFBLK: mode_t { mode_t(0o060000) }
public var S_IFREG: mode_t { mode_t(0o100000) }
public var S_IFLNK: mode_t { mode_t(0o120000) }
public var S_IFSOCK: mode_t { mode_t(0o140000) }
public var S_IRWXU: mode_t { mode_t(0o000700) }
public var S_IRUSR: mode_t { mode_t(0o000400) }
public var S_IWUSR: mode_t { mode_t(0o000200) }
public var S_IXUSR: mode_t { mode_t(0o000100) }
public var S_ISUID: mode_t { mode_t(0o004000) }
public var S_ISGID: mode_t { mode_t(0o002000) }
public var S_ISVTX: mode_t { mode_t(0o001000) }
