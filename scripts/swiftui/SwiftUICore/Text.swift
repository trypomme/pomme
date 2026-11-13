// Text and the localized string keys it's made from.

import Foundation

// pomme: in public struct Text : Swift.Equatable, Swift.Sendable
extension Text {
  // pomme: replaces public struct Text : Swift.Equatable, Swift.Sendable :: public init<S>(_ content: S) where S : Swift.StringProtocol
  @_disfavoredOverload
  public init<S>(_ content: S) where S: StringProtocol {
    self.init(verbatim: String(content))
  }
}

extension Text {
  /// The text to show.
  package var pommeString: String {
    switch storage {
    case .verbatim(let s): return s
    case .anyTextStorage: _pommeUnimplemented("Text made from localized, formatted or styled content")
    }
  }
}

// pomme: in public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation
extension LocalizedStringKey {
  // pomme: replaces public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation :: public init(stringLiteral value: Swift.String)
  @_semantics("swiftui.localized_string_key.init_literal")
  public init(stringLiteral value: String) {
    key = value
    hasFormatting = false
    arguments = []
  }
}

extension LocalizedStringKey {
  /// The string for the key in the app's localizations (the key itself when it has none).
  package func pommeLocalized(in bundle: Bundle? = nil) -> String {
    if hasFormatting { _pommeUnimplemented("LocalizedStringKey with interpolated arguments") }
    return (bundle ?? .main).localizedString(forKey: key, value: key, table: nil)
  }
}
