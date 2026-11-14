// A Font as UIKit draws it: the system font at a size, a text style's font
// (as at the default text size), or a named font, at its weight.
// (Italic, small capitals and leading don't change what pomme draws yet: its
// system font has no italic face.)

import UIKit

extension Font {
  /// The UIFont this font is.
  var pommeUIFont: UIFont {
    guard let box = pommeBox else { return .preferredFont(forTextStyle: .body) }
    var font: UIFont
    switch box.base {
    case .system(let size, _):
      font = .systemFont(ofSize: size)
    case .textStyle(let style, _):
      font = .preferredFont(forTextStyle: style.pommeUIKit)
    case .custom(let name, let size, _):
      font = UIFont(name: name, size: size) ?? .systemFont(ofSize: size)
    }
    if let weight = box.weight {
      font = font.pommeWeighted(weight)
    }
    return font
  }
}

extension Font.TextStyle {
  var pommeUIKit: UIFont.TextStyle {
    switch self {
    case .largeTitle: return .largeTitle
    case .title: return .title1
    case .title2: return .title2
    case .title3: return .title3
    case .headline: return .headline
    case .subheadline: return .subheadline
    case .body: return .body
    case .callout: return .callout
    case .footnote: return .footnote
    case .caption: return .caption1
    case .caption2: return .caption2
    }
  }
}

extension UIFont {
  /// The same font at a weight: the system font's, or (for a named font) the
  /// member of its family nearest that weight.
  func pommeWeighted(_ weight: Font.Weight) -> UIFont {
    if fontName.hasPrefix(".") {
      return .systemFont(ofSize: pointSize, weight: UIFont.Weight(weight.value))
    }
    let members = UIFont.fontNames(forFamilyName: familyName).compactMap { UIFont(name: $0, size: pointSize) }
    func distance(_ f: UIFont) -> CGFloat {
      let w = (f.fontDescriptor.object(forKey: .traits) as? [UIFontDescriptor.TraitKey: Any])?[.weight] as? CGFloat ?? 0
      return abs(w - weight.value)
    }
    return members.min { distance($0) < distance($1) } ?? self
  }
}
