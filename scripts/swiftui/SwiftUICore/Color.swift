// Colors: a Color holds its sRGB value in light and dark appearance (and the
// platform color it was made from, if any) in its box (Color is frozen: its
// one stored property, `provider`, is the box). The named colors are iOS's
// system colors, as UIKit resolves them (recorded on the iOS 26 simulator:
// tests/fixtures/systemcolors).

// pomme: in package class AnyShapeStyleBox : @unchecked Swift.Sendable
extension AnyShapeStyleBox {
  package init(pommeBox: ()) {}
}

// pomme: in package class AnyColorBox : SwiftUICore.AnyShapeStyleBox, @unchecked Swift.Sendable
extension AnyColorBox {
  // (Its own initializer, as the real class has methods of its own.)
  package init(pommeColorBox: ()) {
    super.init(pommeBox: ())
  }
}

/// What a Color is: sRGB components (red, green, blue, opacity) in light and
/// dark appearance, and the platform color (a UIColor) it came from.
package final class PommeColorBox: AnyColorBox, @unchecked Sendable {
  package let light: [Double]
  package let dark: [Double]
  package let object: AnyObject?

  package init(light: [Double], dark: [Double], object: AnyObject?) {
    self.light = light
    self.dark = dark
    self.object = object
    super.init(pommeColorBox: ())
  }
}

// pomme: in public struct Color : Swift.Hashable, Swift.CustomStringConvertible, Swift.Sendable
extension Color {
  /// A color with these components in light appearance and (else the same) dark.
  package init(pommeLight light: [Double], dark: [Double]? = nil, object: AnyObject? = nil) {
    provider = PommeColorBox(light: light, dark: dark ?? light, object: object)
  }

  package var pommeBox: PommeColorBox? { provider as? PommeColorBox }

  // pomme: replaces public struct Color : Swift.Hashable, Swift.CustomStringConvertible, Swift.Sendable :: public func hash(into hasher: inout Swift.Hasher)
  public func hash(into hasher: inout Hasher) {
    hasher.combine(pommeBox?.light ?? [])
    hasher.combine(pommeBox?.dark ?? [])
  }

  // pomme: replaces public struct Color : Swift.Hashable, Swift.CustomStringConvertible, Swift.Sendable :: public static func == (lhs: SwiftUICore.Color, rhs: SwiftUICore.Color) -> Swift.Bool
  public static func == (lhs: Color, rhs: Color) -> Bool {
    lhs.pommeBox?.light == rhs.pommeBox?.light && lhs.pommeBox?.dark == rhs.pommeBox?.dark
  }

  // pomme: replaces public struct Color : Swift.Hashable, Swift.CustomStringConvertible, Swift.Sendable :: public var hashValue: Swift.Int
  public var hashValue: Int {
    var h = Hasher()
    hash(into: &h)
    return h.finalize()
  }

  // pomme: replaces public struct Color : Swift.Hashable, Swift.CustomStringConvertible, Swift.Sendable :: public var description: Swift.String
  public var description: String {
    let c = pommeBox?.light ?? [0, 0, 0, 0]
    return String(format: "#%02X%02X%02X%02X", Int(c[0] * 255), Int(c[1] * 255), Int(c[2] * 255), Int(c[3] * 255))
  }
}

// pomme: in extension SwiftUICore.Color
extension Color {
  // pomme: replaces extension SwiftUICore.Color :: public init(_ colorSpace: SwiftUICore.Color.RGBColorSpace = .sRGB, red: Swift.Double, green: Swift.Double, blue: Swift.Double, opacity: Swift.Double = 1)
  public init(_ colorSpace: Color.RGBColorSpace = .sRGB, red: Double, green: Double, blue: Double, opacity: Double = 1) {
    self.init(pommeLight: [red, green, blue, opacity])
  }

  // pomme: replaces extension SwiftUICore.Color :: public init(_ colorSpace: SwiftUICore.Color.RGBColorSpace = .sRGB, white: Swift.Double, opacity: Swift.Double = 1)
  public init(_ colorSpace: Color.RGBColorSpace = .sRGB, white: Double, opacity: Double = 1) {
    self.init(pommeLight: [white, white, white, opacity])
  }

  // pomme: replaces extension SwiftUICore.Color :: public static let red: SwiftUICore.Color
  public static let red = Color(pommeLight: [1, 0.2196, 0.2353, 1], dark: [1, 0.2588, 0.2706, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let orange: SwiftUICore.Color
  public static let orange = Color(pommeLight: [1, 0.5529, 0.1569, 1], dark: [1, 0.5725, 0.1882, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let yellow: SwiftUICore.Color
  public static let yellow = Color(pommeLight: [1, 0.8, 0, 1], dark: [1, 0.8392, 0, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let green: SwiftUICore.Color
  public static let green = Color(pommeLight: [0.2039, 0.7804, 0.349, 1], dark: [0.1882, 0.8196, 0.3451, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let mint: SwiftUICore.Color
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  public static let mint = Color(pommeLight: [0, 0.7843, 0.702, 1], dark: [0, 0.8549, 0.7647, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let teal: SwiftUICore.Color
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  public static let teal = Color(pommeLight: [0, 0.7647, 0.8157, 1], dark: [0, 0.8235, 0.8784, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let cyan: SwiftUICore.Color
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  public static let cyan = Color(pommeLight: [0, 0.7529, 0.9098, 1], dark: [0.2353, 0.8275, 0.9961, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let blue: SwiftUICore.Color
  public static let blue = Color(pommeLight: [0, 0.5333, 1, 1], dark: [0, 0.5686, 1, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static var accentColor: SwiftUICore.Color
  /// The app's tint: the system blue. (An app's AccentColor asset isn't
  /// read yet: SwiftUICore doesn't see UIKit.)
  public static var accentColor: Color { .blue }
  // pomme: replaces extension SwiftUICore.Color :: public static let indigo: SwiftUICore.Color
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  public static let indigo = Color(pommeLight: [0.3804, 0.3333, 0.9608, 1], dark: [0.4275, 0.4863, 1, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let purple: SwiftUICore.Color
  public static let purple = Color(pommeLight: [0.7961, 0.1882, 0.8784, 1], dark: [0.8588, 0.2039, 0.949, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let pink: SwiftUICore.Color
  public static let pink = Color(pommeLight: [1, 0.1765, 0.3333, 1], dark: [1, 0.2157, 0.3725, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let brown: SwiftUICore.Color
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  public static let brown = Color(pommeLight: [0.6745, 0.498, 0.3686, 1], dark: [0.7176, 0.5412, 0.4, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let gray: SwiftUICore.Color
  public static let gray = Color(pommeLight: [0.5569, 0.5569, 0.5765, 1], dark: [0.5569, 0.5569, 0.5765, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let primary: SwiftUICore.Color
  public static let primary = Color(pommeLight: [0, 0, 0, 1], dark: [1, 1, 1, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let secondary: SwiftUICore.Color
  public static let secondary = Color(pommeLight: [0.2353, 0.2353, 0.2627, 0.6], dark: [0.9216, 0.9216, 0.9608, 0.6])
  // pomme: replaces extension SwiftUICore.Color :: public static let white: SwiftUICore.Color
  public static let white = Color(pommeLight: [1, 1, 1, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let black: SwiftUICore.Color
  public static let black = Color(pommeLight: [0, 0, 0, 1])
  // pomme: replaces extension SwiftUICore.Color :: public static let clear: SwiftUICore.Color
  public static let clear = Color(pommeLight: [0, 0, 0, 0])
}
