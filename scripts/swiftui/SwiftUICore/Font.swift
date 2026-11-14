// Fonts: a Font describes a font (the system font at a size or a text
// style's, or a named font), with its weight and traits, in its box (Font is
// frozen: its one stored property, `provider`, is the box). pomme's SwiftUI
// resolves it to a UIFont where text is drawn.

import CoreGraphics

// pomme: in package class AnyFontBox : @unchecked Swift.Sendable
extension AnyFontBox {
  package init(pommeFontBox: ()) {}
}

/// What a Font is.
package final class PommeFontBox: AnyFontBox, @unchecked Sendable {
  package enum Base: Hashable {
    case system(size: CGFloat, design: Font.Design?)
    case textStyle(Font.TextStyle, design: Font.Design?)
    case custom(name: String, size: CGFloat, relativeTo: Font.TextStyle?)
  }

  package let base: Base
  package let weight: Font.Weight?
  package let italic: Bool
  package let smallCaps: Bool
  package let leading: Font.Leading?

  package init(base: Base, weight: Font.Weight? = nil, italic: Bool = false, smallCaps: Bool = false, leading: Font.Leading? = nil) {
    self.base = base
    self.weight = weight
    self.italic = italic
    self.smallCaps = smallCaps
    self.leading = leading
    super.init(pommeFontBox: ())
  }

  /// The same font with changes.
  package func with(weight: Font.Weight?? = nil, italic: Bool? = nil, smallCaps: Bool? = nil, leading: Font.Leading?? = nil) -> PommeFontBox {
    PommeFontBox(base: base, weight: weight ?? self.weight, italic: italic ?? self.italic, smallCaps: smallCaps ?? self.smallCaps, leading: leading ?? self.leading)
  }

  var key: [AnyHashable] { [base, weight?.value, italic, smallCaps, leading] }
}

// pomme: in public struct Font : Swift.Hashable, Swift.Sendable
extension Font {
  // pomme: replaces public struct Font : Swift.Hashable, Swift.Sendable :: internal var provider: SwiftUICore.AnyFontBox
  internal var provider: AnyFontBox

  package init(pommeBox: PommeFontBox) {
    provider = pommeBox
  }

  package var pommeBox: PommeFontBox? { provider as? PommeFontBox }

  // pomme: replaces public struct Font : Swift.Hashable, Swift.Sendable :: public func hash(into hasher: inout Swift.Hasher)
  public func hash(into hasher: inout Hasher) {
    hasher.combine(pommeBox?.key ?? [])
  }

  // pomme: replaces public struct Font : Swift.Hashable, Swift.Sendable :: public static func == (lhs: SwiftUICore.Font, rhs: SwiftUICore.Font) -> Swift.Bool
  public static func == (lhs: Font, rhs: Font) -> Bool {
    lhs.pommeBox?.key == rhs.pommeBox?.key
  }

  // pomme: replaces public struct Font : Swift.Hashable, Swift.Sendable :: public var hashValue: Swift.Int
  public var hashValue: Int {
    var h = Hasher()
    hash(into: &h)
    return h.finalize()
  }

  /// This font changed (or as it is, if it isn't one pomme made).
  func pommeChanged(_ change: (PommeFontBox) -> PommeFontBox) -> Font {
    guard let box = pommeBox else { return self }
    return Font(pommeBox: change(box))
  }
}

// pomme: in extension SwiftUICore.Font
extension Font {
  // pomme: replaces extension SwiftUICore.Font :: public static let largeTitle: SwiftUICore.Font
  public static let largeTitle = Font(pommeBox: PommeFontBox(base: .textStyle(.largeTitle, design: nil)))
  // pomme: replaces extension SwiftUICore.Font :: public static let title: SwiftUICore.Font
  public static let title = Font(pommeBox: PommeFontBox(base: .textStyle(.title, design: nil)))
  // pomme: replaces extension SwiftUICore.Font :: public static let title2: SwiftUICore.Font
  @available(iOS 14.0, macOS 11.0, tvOS 14.0, watchOS 7.0, *)
  public static let title2 = Font(pommeBox: PommeFontBox(base: .textStyle(.title2, design: nil)))
  // pomme: replaces extension SwiftUICore.Font :: public static let title3: SwiftUICore.Font
  @available(iOS 14.0, macOS 11.0, tvOS 14.0, watchOS 7.0, *)
  public static let title3 = Font(pommeBox: PommeFontBox(base: .textStyle(.title3, design: nil)))
  // pomme: replaces extension SwiftUICore.Font :: public static let headline: SwiftUICore.Font
  public static let headline = Font(pommeBox: PommeFontBox(base: .textStyle(.headline, design: nil)))
  // pomme: replaces extension SwiftUICore.Font :: public static let subheadline: SwiftUICore.Font
  public static let subheadline = Font(pommeBox: PommeFontBox(base: .textStyle(.subheadline, design: nil)))
  // pomme: replaces extension SwiftUICore.Font :: public static let body: SwiftUICore.Font
  public static let body = Font(pommeBox: PommeFontBox(base: .textStyle(.body, design: nil)))
  // pomme: replaces extension SwiftUICore.Font :: public static let callout: SwiftUICore.Font
  public static let callout = Font(pommeBox: PommeFontBox(base: .textStyle(.callout, design: nil)))
  // pomme: replaces extension SwiftUICore.Font :: public static let footnote: SwiftUICore.Font
  public static let footnote = Font(pommeBox: PommeFontBox(base: .textStyle(.footnote, design: nil)))
  // pomme: replaces extension SwiftUICore.Font :: public static let caption: SwiftUICore.Font
  public static let caption = Font(pommeBox: PommeFontBox(base: .textStyle(.caption, design: nil)))
  // pomme: replaces extension SwiftUICore.Font :: public static let caption2: SwiftUICore.Font
  @available(iOS 14.0, macOS 11.0, tvOS 14.0, watchOS 7.0, *)
  public static let caption2 = Font(pommeBox: PommeFontBox(base: .textStyle(.caption2, design: nil)))

  // pomme: replaces extension SwiftUICore.Font :: public static func system(_ style: SwiftUICore.Font.TextStyle, design: SwiftUICore.Font.Design? = nil, weight: SwiftUICore.Font.Weight? = nil) -> SwiftUICore.Font
  @available(iOS 16.0, macOS 13.0, tvOS 16.0, watchOS 9.0, *)
  public static func system(_ style: Font.TextStyle, design: Font.Design? = nil, weight: Font.Weight? = nil) -> Font {
    Font(pommeBox: PommeFontBox(base: .textStyle(style, design: design), weight: weight))
  }

  // pomme: replaces extension SwiftUICore.Font :: public static func system(_ style: SwiftUICore.Font.TextStyle, design: SwiftUICore.Font.Design = .default) -> SwiftUICore.Font
  public static func system(_ style: Font.TextStyle, design: Font.Design = .default) -> Font {
    Font(pommeBox: PommeFontBox(base: .textStyle(style, design: design)))
  }

  // pomme: replaces extension SwiftUICore.Font :: public static func system(size: CoreFoundation.CGFloat, weight: SwiftUICore.Font.Weight? = nil, design: SwiftUICore.Font.Design? = nil) -> SwiftUICore.Font
  @available(iOS 16.0, macOS 13.0, tvOS 16.0, watchOS 9.0, *)
  public static func system(size: CGFloat, weight: Font.Weight? = nil, design: Font.Design? = nil) -> Font {
    Font(pommeBox: PommeFontBox(base: .system(size: size, design: design), weight: weight))
  }

  // pomme: replaces extension SwiftUICore.Font :: public static func system(size: CoreFoundation.CGFloat, weight: SwiftUICore.Font.Weight = .regular, design: SwiftUICore.Font.Design = .default) -> SwiftUICore.Font
  public static func system(size: CGFloat, weight: Font.Weight = .regular, design: Font.Design = .default) -> Font {
    Font(pommeBox: PommeFontBox(base: .system(size: size, design: design), weight: weight))
  }

  // pomme: replaces extension SwiftUICore.Font :: public static func custom(_ name: Swift.String, size: CoreFoundation.CGFloat) -> SwiftUICore.Font
  public static func custom(_ name: String, size: CGFloat) -> Font {
    Font(pommeBox: PommeFontBox(base: .custom(name: name, size: size, relativeTo: .body)))
  }

  // pomme: replaces extension SwiftUICore.Font :: public static func custom(_ name: Swift.String, size: CoreFoundation.CGFloat, relativeTo textStyle: SwiftUICore.Font.TextStyle) -> SwiftUICore.Font
  @available(iOS 14.0, macOS 11.0, tvOS 14.0, watchOS 7.0, *)
  public static func custom(_ name: String, size: CGFloat, relativeTo textStyle: Font.TextStyle) -> Font {
    Font(pommeBox: PommeFontBox(base: .custom(name: name, size: size, relativeTo: textStyle)))
  }

  // pomme: replaces extension SwiftUICore.Font :: public static func custom(_ name: Swift.String, fixedSize: CoreFoundation.CGFloat) -> SwiftUICore.Font
  @available(iOS 14.0, macOS 11.0, tvOS 14.0, watchOS 7.0, *)
  public static func custom(_ name: String, fixedSize: CGFloat) -> Font {
    Font(pommeBox: PommeFontBox(base: .custom(name: name, size: fixedSize, relativeTo: nil)))
  }

  // pomme: replaces extension SwiftUICore.Font :: public func weight(_ weight: SwiftUICore.Font.Weight) -> SwiftUICore.Font
  public func weight(_ weight: Font.Weight) -> Font { pommeChanged { $0.with(weight: .some(weight)) } }
  // pomme: replaces extension SwiftUICore.Font :: public func bold() -> SwiftUICore.Font
  public func bold() -> Font { weight(.bold) }
  // pomme: replaces extension SwiftUICore.Font :: public func bold(_ isActive: Swift.Bool) -> SwiftUICore.Font
  @available(iOS 16.0, macOS 13.0, tvOS 16.0, watchOS 9.0, *)
  public func bold(_ isActive: Bool) -> Font { isActive ? bold() : self }
  // pomme: replaces extension SwiftUICore.Font :: public func italic() -> SwiftUICore.Font
  public func italic() -> Font { pommeChanged { $0.with(italic: true) } }
  // pomme: replaces extension SwiftUICore.Font :: public func italic(_ isActive: Swift.Bool) -> SwiftUICore.Font
  @available(iOS 16.0, macOS 13.0, tvOS 16.0, watchOS 9.0, *)
  public func italic(_ isActive: Bool) -> Font { pommeChanged { $0.with(italic: isActive) } }
  // pomme: replaces extension SwiftUICore.Font :: public func smallCaps() -> SwiftUICore.Font
  public func smallCaps() -> Font { pommeChanged { $0.with(smallCaps: true) } }
  // pomme: replaces extension SwiftUICore.Font :: public func smallCaps(_ isActive: Swift.Bool) -> SwiftUICore.Font
  @available(iOS 26.0, macOS 26.0, tvOS 26.0, watchOS 26.0, visionOS 26.0, *)
  public func smallCaps(_ isActive: Bool) -> Font { pommeChanged { $0.with(smallCaps: isActive) } }
  // pomme: replaces extension SwiftUICore.Font :: public func leading(_ leading: SwiftUICore.Font.Leading) -> SwiftUICore.Font
  @available(iOS 14.0, macOS 11.0, tvOS 14.0, watchOS 7.0, *)
  public func leading(_ leading: Font.Leading) -> Font { pommeChanged { $0.with(leading: .some(leading)) } }
}

// pomme: in public struct Weight : Swift.Hashable
extension Font.Weight {
  // UIKit's weights (UIFont.Weight), which they stand for.
  // pomme: replaces public struct Weight : Swift.Hashable :: public static let ultraLight: SwiftUICore.Font.Weight
  public static let ultraLight = Font.Weight(value: -0.8)
  // pomme: replaces public struct Weight : Swift.Hashable :: public static let thin: SwiftUICore.Font.Weight
  public static let thin = Font.Weight(value: -0.6)
  // pomme: replaces public struct Weight : Swift.Hashable :: public static let light: SwiftUICore.Font.Weight
  public static let light = Font.Weight(value: -0.4)
  // pomme: replaces public struct Weight : Swift.Hashable :: public static let regular: SwiftUICore.Font.Weight
  public static let regular = Font.Weight(value: 0)
  // pomme: replaces public struct Weight : Swift.Hashable :: public static let medium: SwiftUICore.Font.Weight
  public static let medium = Font.Weight(value: 0.23)
  // pomme: replaces public struct Weight : Swift.Hashable :: public static let semibold: SwiftUICore.Font.Weight
  public static let semibold = Font.Weight(value: 0.3)
  // pomme: replaces public struct Weight : Swift.Hashable :: public static let bold: SwiftUICore.Font.Weight
  public static let bold = Font.Weight(value: 0.4)
  // pomme: replaces public struct Weight : Swift.Hashable :: public static let heavy: SwiftUICore.Font.Weight
  public static let heavy = Font.Weight(value: 0.56)
  // pomme: replaces public struct Weight : Swift.Hashable :: public static let black: SwiftUICore.Font.Weight
  public static let black = Font.Weight(value: 0.62)
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func font(_ font: SwiftUICore.Font?) -> some SwiftUICore.View
  /// The font of the text it holds (the environment's `font`).
  nonisolated public func font(_ font: Font?) -> some View {
    environment(\.font, font)
  }
}

// pomme: in extension SwiftUICore.Text
extension Text {
  // pomme: replaces extension SwiftUICore.Text :: nonisolated public func font(_ font: SwiftUICore.Font?) -> SwiftUICore.Text
  nonisolated public func font(_ font: Font?) -> Text {
    var t = self
    t.modifiers.append(.font(font))
    return t
  }

  func pommeModified(_ modifier: Text.Modifier) -> Text {
    var t = self
    t.modifiers.append(modifier)
    return t
  }

  // pomme: replaces extension SwiftUICore.Text :: nonisolated public func fontWeight(_ weight: SwiftUICore.Font.Weight?) -> SwiftUICore.Text
  nonisolated public func fontWeight(_ weight: Font.Weight?) -> Text { pommeModified(.weight(weight)) }
  // pomme: replaces extension SwiftUICore.Text :: nonisolated public func bold() -> SwiftUICore.Text
  nonisolated public func bold() -> Text { fontWeight(.bold) }
  // pomme: replaces extension SwiftUICore.Text :: nonisolated public func bold(_ isActive: Swift.Bool) -> SwiftUICore.Text
  nonisolated public func bold(_ isActive: Bool) -> Text { isActive ? bold() : self }
  // pomme: replaces extension SwiftUICore.Text :: nonisolated public func italic() -> SwiftUICore.Text
  nonisolated public func italic() -> Text { pommeModified(.italic) }
  // pomme: replaces extension SwiftUICore.Text :: nonisolated public func italic(_ isActive: Swift.Bool) -> SwiftUICore.Text
  nonisolated public func italic(_ isActive: Bool) -> Text { isActive ? italic() : self }
  // pomme: replaces extension SwiftUICore.Text :: nonisolated public func foregroundColor(_ color: SwiftUICore.Color?) -> SwiftUICore.Text
  nonisolated public func foregroundColor(_ color: Color?) -> Text { pommeModified(.color(color)) }
}
