// Images: an Image is a system symbol, an image named in a bundle, or a
// platform image, with how it renders (template or original), whether it
// resizes, and its label, in its box (Image is frozen: its one stored
// property, `provider`, is the box). pomme's SwiftUI shows it with UIKit's
// images (Image.swift there).

import Foundation

// pomme: in package class AnyImageProviderBox : @unchecked Swift.Sendable
extension AnyImageProviderBox {
  package init(pommeImageBox: ()) {}
}

/// What an Image is.
package final class PommeImageBox: AnyImageProviderBox, @unchecked Sendable {
  package enum Source {
    /// An SF Symbol, by name.
    case system(String)
    /// An image in a bundle's assets or files (the main bundle's for nil).
    case named(String, Bundle?)
    /// A platform image (a UIImage, or a CGImage at a scale).
    case platform(AnyObject, scale: CGFloat)
  }

  package let source: Source
  package let renderingMode: Image.TemplateRenderingMode?
  /// Whether it fills the space it's offered (`resizable`).
  package let resizable: Bool
  package let label: Text?

  package init(source: Source, renderingMode: Image.TemplateRenderingMode? = nil, resizable: Bool = false, label: Text? = nil) {
    self.source = source
    self.renderingMode = renderingMode
    self.resizable = resizable
    self.label = label
    super.init(pommeImageBox: ())
  }

  package func with(renderingMode: Image.TemplateRenderingMode?? = nil, resizable: Bool? = nil) -> PommeImageBox {
    PommeImageBox(source: source, renderingMode: renderingMode ?? self.renderingMode, resizable: resizable ?? self.resizable, label: label)
  }
}

// pomme: in public struct Image : Swift.Equatable, Swift.Sendable
extension Image {
  // pomme: replaces public struct Image : Swift.Equatable, Swift.Sendable :: package var provider: SwiftUICore.AnyImageProviderBox
  package var provider: AnyImageProviderBox

  package init(pommeBox: PommeImageBox) {
    provider = pommeBox
  }

  package var pommeBox: PommeImageBox? { provider as? PommeImageBox }

  // pomme: replaces public struct Image : Swift.Equatable, Swift.Sendable :: public static func == (lhs: SwiftUICore.Image, rhs: SwiftUICore.Image) -> Swift.Bool
  public static func == (lhs: Image, rhs: Image) -> Bool {
    lhs.provider === rhs.provider
  }
}

// pomme: in extension SwiftUICore.Image
extension Image {
  // pomme: replaces extension SwiftUICore.Image :: public init(systemName: Swift.String)
  public init(systemName: String) {
    self.init(pommeBox: PommeImageBox(source: .system(systemName)))
  }

  // pomme: replaces extension SwiftUICore.Image :: public init(_internalSystemName systemName: Swift.String)
  public init(_internalSystemName systemName: String) {
    self.init(systemName: systemName)
  }

  // pomme: replaces extension SwiftUICore.Image :: public init(systemName: Swift.String, variableValue: Swift.Double?)
  /// (A variable symbol is drawn as it is at full value.)
  @available(iOS 16.0, macOS 13.0, tvOS 16.0, watchOS 9.0, *)
  public init(systemName: String, variableValue: Double?) {
    self.init(systemName: systemName)
  }

  // pomme: replaces extension SwiftUICore.Image :: public init(_ name: Swift.String, variableValue: Swift.Double?, bundle: Foundation.Bundle? = nil)
  @available(iOS 16.0, macOS 13.0, tvOS 16.0, watchOS 9.0, *)
  public init(_ name: String, variableValue: Double?, bundle: Bundle? = nil) {
    self.init(name, bundle: bundle)
  }

  // pomme: replaces extension SwiftUICore.Image :: public init(_ name: Swift.String, bundle: Foundation.Bundle? = nil)
  public init(_ name: String, bundle: Bundle? = nil) {
    self.init(pommeBox: PommeImageBox(source: .named(name, bundle), label: Text(verbatim: name)))
  }

  // pomme: replaces extension SwiftUICore.Image :: public init(_ name: Swift.String, bundle: Foundation.Bundle? = nil, label: SwiftUICore.Text)
  public init(_ name: String, bundle: Bundle? = nil, label: Text) {
    self.init(pommeBox: PommeImageBox(source: .named(name, bundle), label: label))
  }

  // pomme: replaces extension SwiftUICore.Image :: public init(decorative name: Swift.String, bundle: Foundation.Bundle? = nil)
  public init(decorative name: String, bundle: Bundle? = nil) {
    self.init(pommeBox: PommeImageBox(source: .named(name, bundle)))
  }

  // pomme: replaces extension SwiftUICore.Image :: public init(_ cgImage: CoreGraphics.CGImage, scale: CoreFoundation.CGFloat, orientation: SwiftUICore.Image.Orientation = .up, label: SwiftUICore.Text)
  /// (Shown upright: the orientation isn't applied yet.)
  public init(_ cgImage: CGImage, scale: CGFloat, orientation: Image.Orientation = .up, label: Text) {
    self.init(pommeBox: PommeImageBox(source: .platform(cgImage, scale: scale), label: label))
  }

  // pomme: replaces extension SwiftUICore.Image :: public init(decorative cgImage: CoreGraphics.CGImage, scale: CoreFoundation.CGFloat, orientation: SwiftUICore.Image.Orientation = .up)
  public init(decorative cgImage: CGImage, scale: CGFloat, orientation: Image.Orientation = .up) {
    self.init(pommeBox: PommeImageBox(source: .platform(cgImage, scale: scale)))
  }

  /// This image changed (as it is, if it isn't one pomme made).
  func pommeChanged(_ change: (PommeImageBox) -> PommeImageBox) -> Image {
    guard let box = pommeBox else { return self }
    return Image(pommeBox: change(box))
  }

  // pomme: replaces extension SwiftUICore.Image :: public func renderingMode(_ renderingMode: SwiftUICore.Image.TemplateRenderingMode?) -> SwiftUICore.Image
  public func renderingMode(_ renderingMode: Image.TemplateRenderingMode?) -> Image {
    pommeChanged { $0.with(renderingMode: .some(renderingMode)) }
  }

  // pomme: replaces extension SwiftUICore.Image :: public func resizable(capInsets: SwiftUICore.EdgeInsets = EdgeInsets(), resizingMode: SwiftUICore.Image.ResizingMode = .stretch) -> SwiftUICore.Image
  /// (Stretched whole: cap insets and tiling aren't applied yet.)
  public func resizable(capInsets: EdgeInsets = EdgeInsets(), resizingMode: Image.ResizingMode = .stretch) -> Image {
    pommeChanged { $0.with(resizable: true) }
  }
}
