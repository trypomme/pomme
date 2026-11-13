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
