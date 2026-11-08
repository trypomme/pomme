// pomme: the CoreGraphics overlay. CGFloat itself lives in the CoreFoundation
// overlay (as on Apple platforms); this library re-exports it.

@_exported import CoreGraphics

extension CGPoint: @retroactive Equatable {
    public static func == (lhs: CGPoint, rhs: CGPoint) -> Bool {
        lhs.x == rhs.x && lhs.y == rhs.y
    }
}

extension CGSize: @retroactive Equatable {
    public static func == (lhs: CGSize, rhs: CGSize) -> Bool {
        lhs.width == rhs.width && lhs.height == rhs.height
    }
}

extension CGRect: @retroactive Equatable {
    public static func == (lhs: CGRect, rhs: CGRect) -> Bool {
        lhs.origin == rhs.origin && lhs.size == rhs.size
    }
}

extension CGPoint {
    public static var zero: CGPoint { CGPoint(x: 0, y: 0) }
}

extension CGSize {
    public static var zero: CGSize { CGSize(width: 0, height: 0) }
}

extension CGRect {
    public static var zero: CGRect { CGRect(origin: .zero, size: .zero) }
}
