// SwiftUI's colors and UIKit's: a Color made from a UIColor keeps it (and
// what it is in light and dark); a UIColor made from a Color follows the
// appearance as the Color does.

import UIKit

extension Color {
  // pomme: replaces extension SwiftUICore.Color :: public init(_ color: UIKit.UIColor)
  @_disfavoredOverload
  public init(_ color: UIColor) {
    func components(_ style: UIUserInterfaceStyle) -> [Double] {
      var (r, g, b, a): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
      color.resolvedColor(with: UITraitCollection(userInterfaceStyle: style)).getRed(&r, green: &g, blue: &b, alpha: &a)
      return [r, g, b, a].map(Double.init)
    }
    self.init(pommeLight: components(.light), dark: components(.dark), object: color)
  }

  // pomme: replaces extension SwiftUICore.Color :: public init(uiColor: UIKit.UIColor)
  @available(iOS 15.0, tvOS 15.0, watchOS 8.0, *)
  public init(uiColor: UIColor) {
    self.init(uiColor)
  }
}

extension UIColor {
  // pomme: replaces extension UIKit.UIColor :: convenience public init(_ color: SwiftUICore.Color)
  @available(iOS 14.0, tvOS 14.0, watchOS 7.0, *)
  public convenience init(_ color: Color) {
    let box = color.pommeBox
    self.init { traits in
      if let object = box?.object as? UIColor {
        return object.resolvedColor(with: traits)
      }
      let c = (traits.userInterfaceStyle == .dark ? box?.dark : box?.light) ?? [0, 0, 0, 0]
      return UIColor(red: c[0], green: c[1], blue: c[2], alpha: c[3])
    }
  }
}
