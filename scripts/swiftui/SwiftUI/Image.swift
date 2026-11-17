// Images and labels, shown with UIKit's images: a system symbol at its
// font's size and weight and the image scale, in the foreground color
// (symbols are templates); a named or platform image as it is. A Label is
// its icon and title: in a list, the icon (large) centered in a column
// before the title, the icon in the tint; elsewhere, side by side on the
// title's baseline, as iOS 26 lays them out (tests/fixtures/symbolmetrics).

import UIKit

extension Image {
  // pomme: replaces extension SwiftUICore.Image :: public init(uiImage: UIKit.UIImage)
  public init(uiImage: UIImage) {
    self.init(pommeBox: PommeImageBox(source: .platform(uiImage, scale: uiImage.scale)))
  }
}

extension EnvironmentValues {
  /// Whether views are a list's rows (or within them), which some lay out
  /// their own way (a Label).
  var pommeInList: Bool {
    get { pommeByName["pommeInList"] as? Bool ?? false }
    set { pommeByName["pommeInList"] = newValue }
  }
}

extension Image.Scale {
  var pommeUIKit: UIImage.SymbolScale {
    switch self {
    case .small: return .small
    case .medium: return .medium
    case .large: return .large
    }
  }
}

extension UIFont {
  /// The symbol weight nearest the font's.
  var pommeSymbolWeight: UIImage.SymbolWeight {
    let traits = fontDescriptor.object(forKey: .traits) as? [UIFontDescriptor.TraitKey: Any]
    let w = traits?[.weight] as? CGFloat ?? 0
    let stops: [(CGFloat, UIImage.SymbolWeight)] = [
      (-0.8, .ultraLight), (-0.6, .thin), (-0.4, .light), (0, .regular), (0.23, .medium),
      (0.3, .semibold), (0.4, .bold), (0.56, .heavy), (0.62, .black),
    ]
    return stops.min { abs($0.0 - w) < abs($1.0 - w) }!.1
  }
}

extension Image: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    guard let box = pommeBox else { return [] }
    let env = graph.environment
    var image: UIImage?
    switch box.source {
    case .system(let name):
      let font = env.font?.pommeUIFont ?? .preferredFont(forTextStyle: .body)
      let config = UIImage.SymbolConfiguration(pointSize: font.pointSize, weight: font.pommeSymbolWeight, scale: env.imageScale.pommeUIKit)
      image = UIImage(systemName: name, withConfiguration: config)
    case .named(let name, let bundle):
      image = UIImage(named: name, in: bundle, with: nil)
    case .platform(let object, let scale):
      if let ui = object as? UIImage {
        image = ui
      } else if CFGetTypeID(object) == CGImage.typeID {
        image = UIImage(cgImage: object as! CGImage, scale: scale, orientation: .up)
      }
    }
    // Symbols are templates, other images originals, unless the view says.
    let template: Bool
    switch box.renderingMode {
    case .template?: template = true
    case .original?: template = false
    case nil: template = image?.isSymbolImage ?? false
    }
    let node = ImageNode(path: path, image: image?.withRenderingMode(template ? .alwaysTemplate : .alwaysOriginal), resizable: box.resizable)
    node.color = env.pommeForeground ?? .label
    return [node]
  }
}

/// An image: at its own size, or (resizable) the size it's offered.
final class ImageNode: Node {
  let image: UIImage?
  let resizable: Bool
  /// A template's color.
  var color: UIColor = .label

  init(path: ViewPath, image: UIImage?, resizable: Bool) {
    self.image = image
    self.resizable = resizable
    super.init(path: path)
  }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    let own = image?.size ?? .zero
    return resizable ? proposal.replacingUnspecifiedDimensions(by: own) : own
  }

  /// A symbol's baseline is its text baseline.
  override func explicitGuide(_ key: AlignmentKey, size: CGSize) -> CGFloat? {
    guard let image, image.isSymbolImage, let baseline = image.baselineOffsetFromBottom,
          key == VerticalAlignment.firstTextBaseline.key || key == VerticalAlignment.lastTextBaseline.key else { return nil }
    return size.height - baseline * (image.size.height > 0 ? size.height / image.size.height : 1)
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let view: UIImageView = renderer.view(for: path) { UIImageView() }
    if view.image !== image {
      view.image = image
    }
    view.tintColor = color
    view.contentMode = .scaleToFill
    view.frame = frame
    applyAccessibility(to: view)
    parent.pommeAdopt(view)
  }
}

// ---- Label ----

// pomme: owns public struct Label<Title, Icon> : SwiftUICore.View where Title : SwiftUICore.View, Icon : SwiftUICore.View
@available(iOS 14.0, macOS 11.0, tvOS 14.0, watchOS 7.0, *)
@MainActor @preconcurrency
public struct Label<Title, Icon>: View where Title: View, Icon: View {
  var pommeTitle: Title
  var pommeIcon: Icon
}

extension Label {
  // pomme: replaces public struct Label<Title, Icon> : SwiftUICore.View where Title : SwiftUICore.View, Icon : SwiftUICore.View :: public init(@SwiftUICore.ViewBuilder title: () -> Title, @SwiftUICore.ViewBuilder icon: () -> Icon)
  public init(@ViewBuilder title: () -> Title, @ViewBuilder icon: () -> Icon) {
    self.init(pommeTitle: title(), pommeIcon: icon())
  }
}

extension Label where Title == Text, Icon == Image {
  // pomme: replaces extension SwiftUI.Label where Title == SwiftUICore.Text, Icon == SwiftUICore.Image :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, image name: Swift.String)
  nonisolated public init(_ titleKey: LocalizedStringKey, image name: String) {
    self.init(pommeTitle: Text(titleKey), pommeIcon: Image(name))
  }

  // pomme: replaces extension SwiftUI.Label where Title == SwiftUICore.Text, Icon == SwiftUICore.Image :: nonisolated public init(_ titleKey: SwiftUICore.LocalizedStringKey, systemImage name: Swift.String)
  nonisolated public init(_ titleKey: LocalizedStringKey, systemImage name: String) {
    self.init(pommeTitle: Text(titleKey), pommeIcon: Image(systemName: name))
  }

  // pomme: replaces extension SwiftUI.Label where Title == SwiftUICore.Text, Icon == SwiftUICore.Image :: nonisolated public init<S>(_ title: S, image name: Swift.String) where S : Swift.StringProtocol
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, image name: String) where S: StringProtocol {
    self.init(pommeTitle: Text(title), pommeIcon: Image(name))
  }

  // pomme: replaces extension SwiftUI.Label where Title == SwiftUICore.Text, Icon == SwiftUICore.Image :: nonisolated public init<S>(_ title: S, systemImage name: Swift.String) where S : Swift.StringProtocol
  @_disfavoredOverload
  nonisolated public init<S>(_ title: S, systemImage name: String) where S: StringProtocol {
    self.init(pommeTitle: Text(title), pommeIcon: Image(systemName: name))
  }
}

// The built-in label styles.

// pomme: in public struct DefaultLabelStyle : SwiftUI.LabelStyle
extension DefaultLabelStyle {
  // pomme: replaces public struct DefaultLabelStyle : SwiftUI.LabelStyle :: public init()
  public init() { _pommeStorage = () }
}

// pomme: in public struct TitleOnlyLabelStyle : SwiftUI.LabelStyle
extension TitleOnlyLabelStyle {
  // pomme: replaces public struct TitleOnlyLabelStyle : SwiftUI.LabelStyle :: public init()
  public init() { _pommeStorage = () }
}

// pomme: in public struct IconOnlyLabelStyle : SwiftUI.LabelStyle
extension IconOnlyLabelStyle {
  // pomme: replaces public struct IconOnlyLabelStyle : SwiftUI.LabelStyle :: public init()
  public init() { _pommeStorage = () }
}

// pomme: in public struct TitleAndIconLabelStyle : SwiftUI.LabelStyle
extension TitleAndIconLabelStyle {
  // pomme: replaces public struct TitleAndIconLabelStyle : SwiftUI.LabelStyle :: public init()
  public init() { _pommeStorage = () }
}

extension LabelStyle where Self == DefaultLabelStyle {
  // pomme: replaces extension SwiftUI.LabelStyle where Self == SwiftUI.DefaultLabelStyle :: public static var automatic: SwiftUI.DefaultLabelStyle
  public static var automatic: DefaultLabelStyle { DefaultLabelStyle() }
}

extension LabelStyle where Self == TitleOnlyLabelStyle {
  // pomme: replaces extension SwiftUI.LabelStyle where Self == SwiftUI.TitleOnlyLabelStyle :: public static var titleOnly: SwiftUI.TitleOnlyLabelStyle
  public static var titleOnly: TitleOnlyLabelStyle { TitleOnlyLabelStyle() }
}

extension LabelStyle where Self == IconOnlyLabelStyle {
  // pomme: replaces extension SwiftUI.LabelStyle where Self == SwiftUI.IconOnlyLabelStyle :: public static var iconOnly: SwiftUI.IconOnlyLabelStyle
  public static var iconOnly: IconOnlyLabelStyle { IconOnlyLabelStyle() }
}

extension LabelStyle where Self == TitleAndIconLabelStyle {
  // pomme: replaces extension SwiftUI.LabelStyle where Self == SwiftUI.TitleAndIconLabelStyle :: public static var titleAndIcon: SwiftUI.TitleAndIconLabelStyle
  public static var titleAndIcon: TitleAndIconLabelStyle { TitleAndIconLabelStyle() }
}

extension EnvironmentValues {
  /// The style of the labels within (`labelStyle`); automatic until set.
  var pommeLabelStyle: Any? {
    get { pommeByName["pommeLabelStyle"] }
    set { pommeByName["pommeLabelStyle"] = newValue }
  }
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func labelStyle<S>(_ style: S) -> some SwiftUICore.View where S : SwiftUI.LabelStyle
  nonisolated public func labelStyle<S>(_ style: S) -> some View where S: LabelStyle {
    environment(\.pommeLabelStyle, style)
  }
}

extension Label: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    var showsTitle = true, showsIcon = true
    switch graph.environment.pommeLabelStyle {
    case nil, is DefaultLabelStyle, is TitleAndIconLabelStyle: break
    case is TitleOnlyLabelStyle: showsIcon = false
    case is IconOnlyLabelStyle: showsTitle = false
    case let style?:
      fatalError("SwiftUI in pomme doesn't implement the label style \(type(of: style)) yet")
    }
    let inList = graph.environment.pommeInList
    let title = showsTitle ? graph.resolve(pommeTitle, at: path.appending("title"), preferences) : []
    // (In a list, its icon at the large scale, in the tint unless the
    // foreground is set.)
    let icon = showsIcon
      ? graph.withEnvironment({ env in
          if inList {
            env.imageScale = .large
            if env.pommeForeground == nil { env.pommeForeground = .tintColor }
          }
        }) { graph.resolve(pommeIcon, at: path.appending("icon"), preferences) }
      : []
    return [LabelNode(path: path, icon: icon.first, title: title.first, inList: inList)]
  }
}

/// A label's icon and title.
final class LabelNode: Node {
  let icon: Node?
  let title: Node?
  let inList: Bool

  // iOS 26's (the symbolmetrics probe): in a list, the icon centered 12 pt
  // in and the title 40 pt in; elsewhere, the title 8 pt after the icon.
  static let listIconCenter: CGFloat = 12
  static let listTitleLeading: CGFloat = 40
  static let spacing: CGFloat = 8

  init(path: ViewPath, icon: Node?, title: Node?, inList: Bool) {
    self.icon = icon
    self.title = title
    self.inList = inList
    super.init(path: path)
  }

  /// Where the icon and the title go, in a label of the size it takes.
  private func layout(_ proposal: ProposedViewSize) -> (size: CGSize, icon: CGRect, title: CGRect) {
    guard let title else {
      let s = icon?.sizeThatFits(proposal) ?? .zero
      return (s, CGRect(origin: .zero, size: s), .zero)
    }
    let iconSize = icon?.sizeThatFits(.unspecified) ?? .zero
    let titleX = icon == nil ? 0 : inList ? Self.listTitleLeading : iconSize.width + Self.spacing
    let titleSize = title.sizeThatFits(ProposedViewSize(width: proposal.width.map { max(0, $0 - titleX) }, height: proposal.height))
    guard let icon else { return (titleSize, .zero, CGRect(origin: .zero, size: titleSize)) }
    if inList {
      // The icon centered on the title, which sets the height.
      let iconFrame = CGRect(x: Self.listIconCenter - iconSize.width / 2, y: (titleSize.height - iconSize.height) / 2, width: iconSize.width, height: iconSize.height)
      return (CGSize(width: titleX + titleSize.width, height: titleSize.height), iconFrame, CGRect(origin: CGPoint(x: titleX, y: 0), size: titleSize))
    }
    // On the title's first baseline.
    let key = VerticalAlignment.firstTextBaseline.key
    let iconBase = icon.explicitGuide(key, size: iconSize) ?? iconSize.height
    let titleBase = title.explicitGuide(key, size: titleSize) ?? titleSize.height
    let iconTop = max(0, titleBase - iconBase), titleTop = max(0, iconBase - titleBase)
    let height = max(iconTop + iconSize.height, titleTop + titleSize.height)
    return (CGSize(width: titleX + titleSize.width, height: height),
            CGRect(origin: CGPoint(x: 0, y: iconTop), size: iconSize),
            CGRect(origin: CGPoint(x: titleX, y: titleTop), size: titleSize))
  }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { layout(proposal).size }

  override func explicitGuide(_ key: AlignmentKey, size: CGSize) -> CGFloat? {
    let l = layout(ProposedViewSize(size))
    if let title, let g = title.explicitGuide(key, size: l.title.size) { return l.title.minY + g }
    return nil
  }

  override var firstTextLeading: CGFloat? {
    guard title != nil else { return nil }
    return layout(.unspecified).title.minX
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let container: ContainerView = renderer.view(for: path) { ContainerView() }
    container.frame = frame
    applyAccessibility(to: container)
    parent.pommeAdopt(container)
    let l = layout(ProposedViewSize(frame.size))
    icon?.place(in: l.icon, of: container, renderer)
    title?.place(in: l.title, of: container, renderer)
  }
}
