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

// pomme: in extension SwiftUICore.Text
extension Text {
  // pomme: replaces extension SwiftUICore.Text :: public init(_ key: SwiftUICore.LocalizedStringKey, tableName: Swift.String? = nil, bundle: Foundation.Bundle? = nil, comment: Swift.StaticString? = nil)
  /// The key's string in the app's (or the bundle's) localizations: the key
  /// itself without one.
  @_semantics("swiftui.init_with_localization")
  public init(_ key: LocalizedStringKey, tableName: String? = nil, bundle: Bundle? = nil, comment: StaticString? = nil) {
    self.init(verbatim: key.pommeLocalized(in: bundle, table: tableName))
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
  package func pommeLocalized(in bundle: Bundle? = nil, table: String? = nil) -> String {
    if hasFormatting { _pommeUnimplemented("LocalizedStringKey with interpolated arguments") }
    return (bundle ?? .main).localizedString(forKey: key, value: key, table: table)
  }
}
