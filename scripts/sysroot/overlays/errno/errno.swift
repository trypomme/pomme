// pomme: Swift's `errno`, which Apple's SDKs ship in libswift_errno (module
// _errno) beside POSIXErrorCode (from swiftlang/swift's POSIXError.swift),
// both under their original Darwin names. It reads and writes the calling
// thread's errno through libc's __error().

@_silgen_name("__error")
internal func _pomme_errno_location() -> UnsafeMutablePointer<Int32>

@_originallyDefinedIn(module: "Darwin", macOS 15.0)
@available(macOS 10.0, *)
public var errno: Int32 {
    get { _pomme_errno_location().pointee }
    set { _pomme_errno_location().pointee = newValue }
}
