// Shape styles that stand for others: the hierarchical levels of the
// foreground (primary, secondary…), and the modifier that sets the
// foreground. pomme's SwiftUI resolves them where it fills (ShapeStyle.swift
// there).

// pomme: in public struct HierarchicalShapeStyle : SwiftUICore.ShapeStyle
extension HierarchicalShapeStyle {
  // pomme: replaces public struct HierarchicalShapeStyle : SwiftUICore.ShapeStyle :: public static let primary: SwiftUICore.HierarchicalShapeStyle
  public static let primary = HierarchicalShapeStyle(id: 0)
  // pomme: replaces public struct HierarchicalShapeStyle : SwiftUICore.ShapeStyle :: public static let secondary: SwiftUICore.HierarchicalShapeStyle
  public static let secondary = HierarchicalShapeStyle(id: 1)
  // pomme: replaces public struct HierarchicalShapeStyle : SwiftUICore.ShapeStyle :: public static let tertiary: SwiftUICore.HierarchicalShapeStyle
  public static let tertiary = HierarchicalShapeStyle(id: 2)
  // pomme: replaces public struct HierarchicalShapeStyle : SwiftUICore.ShapeStyle :: public static let quaternary: SwiftUICore.HierarchicalShapeStyle
  public static let quaternary = HierarchicalShapeStyle(id: 3)
}

extension ShapeStyle where Self == HierarchicalShapeStyle {
  // pomme: replaces extension SwiftUICore.ShapeStyle where Self == SwiftUICore.HierarchicalShapeStyle :: public static var primary: SwiftUICore.HierarchicalShapeStyle
  public static var primary: HierarchicalShapeStyle { HierarchicalShapeStyle.primary }
  // pomme: replaces extension SwiftUICore.ShapeStyle where Self == SwiftUICore.HierarchicalShapeStyle :: public static var secondary: SwiftUICore.HierarchicalShapeStyle
  public static var secondary: HierarchicalShapeStyle { HierarchicalShapeStyle.secondary }
  // pomme: replaces extension SwiftUICore.ShapeStyle where Self == SwiftUICore.HierarchicalShapeStyle :: public static var tertiary: SwiftUICore.HierarchicalShapeStyle
  public static var tertiary: HierarchicalShapeStyle { HierarchicalShapeStyle.tertiary }
  // pomme: replaces extension SwiftUICore.ShapeStyle where Self == SwiftUICore.HierarchicalShapeStyle :: public static var quaternary: SwiftUICore.HierarchicalShapeStyle
  public static var quaternary: HierarchicalShapeStyle { HierarchicalShapeStyle.quaternary }
}

// pomme: owns public struct _ForegroundStyleModifier<Style> where Style : SwiftUICore.ShapeStyle
@frozen public struct _ForegroundStyleModifier<Style> where Style: ShapeStyle {
  // pomme: replaces public struct _ForegroundStyleModifier<Style> where Style : SwiftUICore.ShapeStyle :: public var style: Style
  public var style: Style
  // pomme: replaces public struct _ForegroundStyleModifier<Style> where Style : SwiftUICore.ShapeStyle :: public init(style: Style)
  @inlinable public init(style: Style) {
    self.style = style
  }
  // pomme: replaces public struct _ForegroundStyleModifier<Style> where Style : SwiftUICore.ShapeStyle :: nonisolated public static func _makeViewInputs(modifier: SwiftUICore._GraphValue<SwiftUICore._ForegroundStyleModifier<Style>>, inputs: inout SwiftUICore._ViewInputs)
  nonisolated public static func _makeViewInputs(modifier: _GraphValue<_ForegroundStyleModifier<Style>>, inputs: inout _ViewInputs) {}
  // pomme: replaces public struct _ForegroundStyleModifier<Style> where Style : SwiftUICore.ShapeStyle :: public typealias Body = Swift.Never
  public typealias Body = Never
}

/// A shape style, type-erased (AnyShapeStyle's box).
package final class PommeShapeStyleBox: AnyShapeStyleBox, @unchecked Sendable {
  package let base: any ShapeStyle

  package init(_ base: any ShapeStyle) {
    self.base = base
    super.init(pommeBox: ())
  }
}

// pomme: in public struct AnyShapeStyle : SwiftUICore.ShapeStyle
extension AnyShapeStyle {
  // pomme: replaces public struct AnyShapeStyle : SwiftUICore.ShapeStyle :: public init<S>(_ style: S) where S : SwiftUICore.ShapeStyle
  public init<S>(_ style: S) where S: ShapeStyle {
    if let erased = style as? AnyShapeStyle {
      self = erased
    } else {
      storage = Storage(box: PommeShapeStyleBox(style))
    }
  }

  /// The style it erases.
  package var pommeBase: any ShapeStyle {
    (storage.box as? PommeShapeStyleBox)?.base ?? self
  }
}

// pomme: in public struct AnyShapeStyle : SwiftUICore.ShapeStyle > package struct Storage : Swift.Equatable
extension AnyShapeStyle.Storage {
  // pomme: replaces public struct AnyShapeStyle : SwiftUICore.ShapeStyle > package struct Storage : Swift.Equatable :: package static func == (lhs: SwiftUICore.AnyShapeStyle.Storage, rhs: SwiftUICore.AnyShapeStyle.Storage) -> Swift.Bool
  @usableFromInline
  package static func == (lhs: AnyShapeStyle.Storage, rhs: AnyShapeStyle.Storage) -> Bool {
    lhs.box === rhs.box
  }
}
