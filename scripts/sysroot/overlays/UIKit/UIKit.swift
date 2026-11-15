// The Swift half of UIKit that is Swift types of its own (not Objective-C
// seen from Swift), ours, written from the SDK's interface with the same
// names and signatures, so apps that use them link: iOS 26's corners
// (UICornerRadius, UICornerConfiguration, UIView.cornerConfiguration),
// which Liquid Glass views are shaped with. Each wraps the Objective-C
// object UIKit takes (__UICornerRadius, __UICornerConfiguration), which
// pomme implements (crates/pomme-hle/src/glass.rs). Everything else of
// UIKit's is pomme's host code, as before.

@available(iOS 26.0, tvOS 26.0, visionOS 26.0, *)
@available(watchOS, unavailable)
public struct UICornerRadius: Swift.Equatable, Swift.Hashable, Swift.ExpressibleByFloatLiteral, Swift.ExpressibleByIntegerLiteral, Swift.CustomStringConvertible {
  /// A fixed radius, or (nil) concentric with the container, at least `minimum`.
  let fixedRadius: Double?
  let minimum: CGFloat?

  init(fixedRadius: Double?, minimum: CGFloat?) {
    self.fixedRadius = fixedRadius
    self.minimum = minimum
  }

  public static func fixed(_ radius: Swift.Double) -> UICornerRadius {
    UICornerRadius(fixedRadius: radius, minimum: nil)
  }

  public static func containerConcentric(minimum: CGFloat? = nil) -> UICornerRadius {
    UICornerRadius(fixedRadius: nil, minimum: minimum)
  }

  public init(floatLiteral value: Swift.FloatLiteralType) {
    self.init(fixedRadius: value, minimum: nil)
  }

  public init(integerLiteral value: Swift.IntegerLiteralType) {
    self.init(fixedRadius: Double(value), minimum: nil)
  }

  public var description: Swift.String {
    if let r = fixedRadius { return "UICornerRadius.fixed(\(r))" }
    if let m = minimum { return "UICornerRadius.containerConcentric(minimum: \(m))" }
    return "UICornerRadius.containerConcentric()"
  }

  public static func == (a: UICornerRadius, b: UICornerRadius) -> Swift.Bool {
    a.fixedRadius == b.fixedRadius && a.minimum == b.minimum
  }

  public func hash(into hasher: inout Swift.Hasher) {
    hasher.combine(fixedRadius)
    hasher.combine(minimum)
  }

  var objc: __UICornerRadius {
    if let r = fixedRadius { return __UICornerRadius.fixedRadius(CGFloat(r)) }
    if let m = minimum { return __UICornerRadius.containerConcentricRadius(withMinimum: m) }
    return __UICornerRadius.containerConcentric()
  }
}

@available(iOS 26.0, tvOS 26.0, visionOS 26.0, *)
@available(watchOS, unavailable)
public struct UICornerConfiguration: Swift.Equatable, Swift.Hashable, Swift.CustomStringConvertible {
  let objc: __UICornerConfiguration

  init(_ objc: __UICornerConfiguration) {
    self.objc = objc
  }

  public static func corners(radius: UICornerRadius) -> UICornerConfiguration {
    UICornerConfiguration(__UICornerConfiguration.corners(radius: radius.objc))
  }

  public static func corners(topLeftRadius: UICornerRadius?, topRightRadius: UICornerRadius?, bottomLeftRadius: UICornerRadius?, bottomRightRadius: UICornerRadius?) -> UICornerConfiguration {
    UICornerConfiguration(__UICornerConfiguration.corners(topLeftRadius: topLeftRadius?.objc, topRightRadius: topRightRadius?.objc, bottomLeftRadius: bottomLeftRadius?.objc, bottomRightRadius: bottomRightRadius?.objc))
  }

  public static func capsule(maximumRadius: Swift.Double? = nil) -> UICornerConfiguration {
    if let m = maximumRadius {
      return UICornerConfiguration(__UICornerConfiguration.capsule(maximumRadius: CGFloat(m)))
    }
    return UICornerConfiguration(__UICornerConfiguration.capsule())
  }

  public static func uniformCorners(radius: UICornerRadius) -> UICornerConfiguration {
    UICornerConfiguration(__UICornerConfiguration.uniformCorners(radius: radius.objc))
  }

  public static func uniformEdges(topRadius: UICornerRadius, bottomRadius: UICornerRadius) -> UICornerConfiguration {
    UICornerConfiguration(__UICornerConfiguration.uniformEdges(topRadius: topRadius.objc, bottomRadius: bottomRadius.objc))
  }

  public static func uniformEdges(leftRadius: UICornerRadius, rightRadius: UICornerRadius) -> UICornerConfiguration {
    UICornerConfiguration(__UICornerConfiguration.uniformEdges(leftRadius: leftRadius.objc, rightRadius: rightRadius.objc))
  }

  public static func uniformTopRadius(_ topRadius: UICornerRadius, bottomLeftRadius: UICornerRadius? = nil, bottomRightRadius: UICornerRadius? = nil) -> UICornerConfiguration {
    UICornerConfiguration(__UICornerConfiguration.uniformTopRadius(topRadius.objc, bottomLeftRadius: bottomLeftRadius?.objc, bottomRightRadius: bottomRightRadius?.objc))
  }

  public static func uniformBottomRadius(_ bottomRadius: UICornerRadius, topLeftRadius: UICornerRadius? = nil, topRightRadius: UICornerRadius? = nil) -> UICornerConfiguration {
    UICornerConfiguration(__UICornerConfiguration.uniformBottomRadius(bottomRadius.objc, topLeftRadius: topLeftRadius?.objc, topRightRadius: topRightRadius?.objc))
  }

  public static func uniformLeftRadius(_ leftRadius: UICornerRadius, topRightRadius: UICornerRadius? = nil, bottomRightRadius: UICornerRadius? = nil) -> UICornerConfiguration {
    UICornerConfiguration(__UICornerConfiguration.uniformLeftRadius(leftRadius.objc, topRightRadius: topRightRadius?.objc, bottomRightRadius: bottomRightRadius?.objc))
  }

  public static func uniformRightRadius(_ rightRadius: UICornerRadius, topLeftRadius: UICornerRadius? = nil, bottomLeftRadius: UICornerRadius? = nil) -> UICornerConfiguration {
    UICornerConfiguration(__UICornerConfiguration.uniformRightRadius(rightRadius.objc, topLeftRadius: topLeftRadius?.objc, bottomLeftRadius: bottomLeftRadius?.objc))
  }

  public func hash(into hasher: inout Swift.Hasher) {
    hasher.combine(objc.hash)
  }

  public var description: Swift.String {
    objc.description
  }

  public static func == (a: UICornerConfiguration, b: UICornerConfiguration) -> Swift.Bool {
    a.objc.isEqual(b.objc)
  }
}

@available(iOS 26.0, tvOS 26.0, visionOS 26.0, *)
@available(watchOS, unavailable)
extension UIView {
  @MainActor @preconcurrency public var cornerConfiguration: UICornerConfiguration {
    get { UICornerConfiguration(__cornerConfiguration) }
    set { __cornerConfiguration = newValue.objc }
  }
}
