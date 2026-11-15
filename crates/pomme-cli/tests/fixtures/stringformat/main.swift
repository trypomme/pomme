// Swift's String(format:) with each kind of argument (CVarArg), as apps
// and probes format numbers; tests/string_format.rs checks what it prints.

import Foundation
import CoreGraphics

@main
struct Main {
  static func main() {
    print("FMT int \(String(format: "%d|%5d|%x", 42, 7, 255))")
    print("FMT double \(String(format: "%.1f|%.3f|%g", 1.25, 3.14159, 0.5))")
    print("FMT cgfloat \(String(format: "%.1f", CGFloat(92.35)))")
    print("FMT float \(String(format: "%.2f", Float(2.5)))")
    print("FMT string \(String(format: "%@ and %@", "a", "b" as NSString))")
    print("FMT mixed \(String(format: "%@ is %d (%.1f%%)", "x", 3, 42.5))")
    print("FMT int64 \(String(format: "%lld %lu", Int64(-9), UInt(9)))")
    print("FMT done")
  }
}
