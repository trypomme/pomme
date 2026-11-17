// Arithmetic on UIKit's numeric raw-value types (window levels, layout
// priorities, font weights and widths): `.statusBar - 1`,
// `.defaultHigh + 1`. Written from the SDK's interface (the same names and
// signatures), as Apple's overlay has it.

@available(swift 4)
@available(iOS 7.0, *)
public protocol _UIKitNumericRawRepresentable: Swift.Comparable, Swift.RawRepresentable where Self.RawValue: Swift.Comparable, Self.RawValue: Swift.Numeric {
}

@available(swift 4)
@available(iOS 7.0, *)
extension _UIKitNumericRawRepresentable {
  public static func < (lhs: Self, rhs: Self) -> Swift.Bool { lhs.rawValue < rhs.rawValue }
  public static func + (lhs: Self, rhs: Self.RawValue) -> Self { Self(rawValue: lhs.rawValue + rhs)! }
  public static func + (lhs: Self.RawValue, rhs: Self) -> Self { Self(rawValue: lhs + rhs.rawValue)! }
  public static func - (lhs: Self, rhs: Self.RawValue) -> Self { Self(rawValue: lhs.rawValue - rhs)! }
  public static func - (lhs: Self, rhs: Self) -> Self.RawValue { lhs.rawValue - rhs.rawValue }
  public static func += (lhs: inout Self, rhs: Self.RawValue) { lhs = lhs + rhs }
  public static func -= (lhs: inout Self, rhs: Self.RawValue) { lhs = lhs - rhs }
}

@available(iOS 11.0, *)
extension UIFont.Weight: _UIKitNumericRawRepresentable {}

@available(iOS 11.0, *)
extension UIFont.Width: _UIKitNumericRawRepresentable {}

@available(iOS 7.0, *)
extension UILayoutPriority: _UIKitNumericRawRepresentable {}

@available(iOS 7.0, *)
extension UIWindow.Level: _UIKitNumericRawRepresentable {}
