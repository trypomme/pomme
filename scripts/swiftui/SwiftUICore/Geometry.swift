// Geometry: what a view knows of where it is (`GeometryReader`,
// `onGeometryChange`): its size, its frame in a coordinate space, its safe area.

import CoreGraphics
import Foundation

// pomme: in public struct GeometryProxy
extension GeometryProxy {
  package let pommeSize: CGSize
  /// Its frame in a coordinate space (the view's own, the screen's, or a named one's).
  package let pommeFrame: (CoordinateSpace) -> CGRect
  package let pommeSafeArea: EdgeInsets

  package init(size: CGSize, safeArea: EdgeInsets, frame: @escaping (CoordinateSpace) -> CGRect) {
    _pommeStorage = ()
    pommeSize = size
    pommeSafeArea = safeArea
    pommeFrame = frame
  }

  // pomme: replaces public struct GeometryProxy :: public var size: CoreFoundation.CGSize
  public var size: CGSize { pommeSize }

  // pomme: replaces public struct GeometryProxy :: public var safeAreaInsets: SwiftUICore.EdgeInsets
  public var safeAreaInsets: EdgeInsets { pommeSafeArea }

  // pomme: replaces public struct GeometryProxy :: public func frame(in coordinateSpace: SwiftUICore.CoordinateSpace) -> CoreFoundation.CGRect
  @_disfavoredOverload
  public func frame(in coordinateSpace: CoordinateSpace) -> CGRect { pommeFrame(coordinateSpace) }
}

// pomme: in extension SwiftUICore.GeometryProxy
extension GeometryProxy {
  // pomme: replaces extension SwiftUICore.GeometryProxy :: public func frame(in coordinateSpace: some CoordinateSpaceProtocol) -> CoreFoundation.CGRect
  public func frame(in coordinateSpace: some CoordinateSpaceProtocol) -> CGRect {
    pommeFrame(coordinateSpace.coordinateSpace)
  }

  // pomme: replaces extension SwiftUICore.GeometryProxy :: public func bounds(of coordinateSpace: SwiftUICore.NamedCoordinateSpace) -> CoreFoundation.CGRect?
  /// A named space's bounds, in the view's own coordinates.
  public func bounds(of coordinateSpace: NamedCoordinateSpace) -> CGRect? {
    let own = pommeFrame(.global)
    let named = pommeFrame(coordinateSpace.coordinateSpace)
    return CGRect(x: own.minX - named.minX, y: own.minY - named.minY, width: named.width, height: named.height)
  }
}

extension _GeometryActionModifier {
  /// What the modified view's geometry gives, and what to do when that changes.
  package var pommeValue: (GeometryProxy) -> Value { value }
}

// ---- Coordinate spaces ----

// pomme: in public struct GlobalCoordinateSpace : SwiftUICore.CoordinateSpaceProtocol
extension GlobalCoordinateSpace {
  // pomme: replaces public struct GlobalCoordinateSpace : SwiftUICore.CoordinateSpaceProtocol :: public init()
  public init() { _pommeStorage = () }
  // pomme: replaces public struct GlobalCoordinateSpace : SwiftUICore.CoordinateSpaceProtocol :: public var coordinateSpace: SwiftUICore.CoordinateSpace
  public var coordinateSpace: CoordinateSpace { .global }
}

// pomme: in public struct LocalCoordinateSpace : SwiftUICore.CoordinateSpaceProtocol
extension LocalCoordinateSpace {
  // pomme: replaces public struct LocalCoordinateSpace : SwiftUICore.CoordinateSpaceProtocol :: public init()
  public init() { _pommeStorage = () }
  // pomme: replaces public struct LocalCoordinateSpace : SwiftUICore.CoordinateSpaceProtocol :: public var coordinateSpace: SwiftUICore.CoordinateSpace
  public var coordinateSpace: CoordinateSpace { .local }
}

// pomme: in public struct NamedCoordinateSpace : SwiftUICore.CoordinateSpaceProtocol, Swift.Equatable
extension NamedCoordinateSpace {
  /// The space's name.
  package let pommeName: AnyHashable

  package init(pommeName: AnyHashable) {
    _pommeStorage = ()
    self.pommeName = pommeName
  }

  // pomme: replaces public struct NamedCoordinateSpace : SwiftUICore.CoordinateSpaceProtocol, Swift.Equatable :: public var coordinateSpace: SwiftUICore.CoordinateSpace
  public var coordinateSpace: CoordinateSpace { .named(pommeName) }
  // pomme: replaces public struct NamedCoordinateSpace : SwiftUICore.CoordinateSpaceProtocol, Swift.Equatable :: public static func == (a: SwiftUICore.NamedCoordinateSpace, b: SwiftUICore.NamedCoordinateSpace) -> Swift.Bool
  public static func == (a: NamedCoordinateSpace, b: NamedCoordinateSpace) -> Bool { a.pommeName == b.pommeName }
}

// pomme: in extension SwiftUICore.CoordinateSpaceProtocol where Self == SwiftUICore.GlobalCoordinateSpace
extension CoordinateSpaceProtocol where Self == GlobalCoordinateSpace {
  // pomme: replaces extension SwiftUICore.CoordinateSpaceProtocol where Self == SwiftUICore.GlobalCoordinateSpace :: public static var global: SwiftUICore.GlobalCoordinateSpace
  public static var global: GlobalCoordinateSpace { GlobalCoordinateSpace() }
}

// pomme: in extension SwiftUICore.CoordinateSpaceProtocol where Self == SwiftUICore.LocalCoordinateSpace
extension CoordinateSpaceProtocol where Self == LocalCoordinateSpace {
  // pomme: replaces extension SwiftUICore.CoordinateSpaceProtocol where Self == SwiftUICore.LocalCoordinateSpace :: public static var local: SwiftUICore.LocalCoordinateSpace
  public static var local: LocalCoordinateSpace { LocalCoordinateSpace() }
}

// pomme: in extension SwiftUICore.CoordinateSpaceProtocol where Self == SwiftUICore.NamedCoordinateSpace
extension CoordinateSpaceProtocol where Self == NamedCoordinateSpace {
  // pomme: replaces extension SwiftUICore.CoordinateSpaceProtocol where Self == SwiftUICore.NamedCoordinateSpace :: public static func named(_ name: some Hashable) -> SwiftUICore.NamedCoordinateSpace
  public static func named(_ name: some Hashable) -> NamedCoordinateSpace { NamedCoordinateSpace(pommeName: AnyHashable(name)) }
}
