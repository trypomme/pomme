// Localized string keys made by interpolation: `Text("Count: \(count)")`.
// As SwiftUI does, the key is the format with a specifier for each value
// ("Count: %lld"), looked up in the app's localizations, then formatted with
// the values: an app's Localizable.strings can translate "Count: %lld".

import CoreGraphics
import Foundation

// pomme: in public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation > internal struct FormatArgument : Swift.Equatable
extension LocalizedStringKey.FormatArgument {
  /// The value, as `String(format:)` takes it.
  let pommeValue: any CVarArg
  /// What it is, to compare keys by.
  let pommeDescription: String

  // pomme: replaces public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation > internal struct FormatArgument : Swift.Equatable :: internal static func == (lhs: SwiftUICore.LocalizedStringKey.FormatArgument, rhs: SwiftUICore.LocalizedStringKey.FormatArgument) -> Swift.Bool
  @usableFromInline
  internal static func == (lhs: LocalizedStringKey.FormatArgument, rhs: LocalizedStringKey.FormatArgument) -> Bool {
    lhs.pommeDescription == rhs.pommeDescription
  }
}

extension LocalizedStringKey.FormatArgument {
  init(pommeValue: any CVarArg, description: String) {
    self.pommeValue = pommeValue
    self.pommeDescription = description
  }

  /// An object argument (`%@`): a string.
  init(pommeString s: String) {
    self.init(pommeValue: s as NSString, description: s)
  }
}

// pomme: in public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation > public struct StringInterpolation : Swift.StringInterpolationProtocol
extension LocalizedStringKey.StringInterpolation {
  /// The format so far, and its arguments.
  var pommeKey: String
  var pommeArguments: [LocalizedStringKey.FormatArgument]

  // pomme: replaces public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation > public struct StringInterpolation : Swift.StringInterpolationProtocol :: public init(literalCapacity: Swift.Int, interpolationCount: Swift.Int)
  @_semantics("swiftui.localized.interpolation_init")
  public init(literalCapacity: Int, interpolationCount: Int) {
    pommeKey = ""
    pommeKey.reserveCapacity(literalCapacity + 3 * interpolationCount)
    pommeArguments = []
    _pommeStorage = ()
  }

  // pomme: replaces public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation > public struct StringInterpolation : Swift.StringInterpolationProtocol :: public mutating func appendLiteral(_ literal: Swift.String)
  @_semantics("swiftui.localized.appendLiteral")
  public mutating func appendLiteral(_ literal: String) {
    // A literal percent sign stays one once formatted.
    pommeKey += literal.replacingOccurrences(of: "%", with: "%%")
  }

  // pomme: replaces public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation > public struct StringInterpolation : Swift.StringInterpolationProtocol :: public mutating func appendInterpolation(_ string: Swift.String)
  @_semantics("swiftui.localized.appendInterpolation_@_specifier")
  public mutating func appendInterpolation(_ string: String) {
    pommeAppend("%@", .init(pommeString: string))
  }

  // pomme: replaces public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation > public struct StringInterpolation : Swift.StringInterpolationProtocol :: public mutating func appendInterpolation<T>(_ value: T, specifier: Swift.String) where T : SwiftUICore._FormatSpecifiable
  @_semantics("swiftui.localized.appendInterpolation_param_specifier")
  public mutating func appendInterpolation<T>(_ value: T, specifier: String) where T: _FormatSpecifiable {
    pommeAppend(specifier, .init(pommeValue: value._arg, description: "\(value)"))
  }

  // pomme: replaces public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation > public struct StringInterpolation : Swift.StringInterpolationProtocol :: public mutating func appendInterpolation<Subject>(_ subject: Subject, formatter: Foundation.Formatter? = nil) where Subject : Foundation.ReferenceConvertible
  @_semantics("swiftui.localized.appendInterpolation_@_specifier")
  public mutating func appendInterpolation<Subject>(_ subject: Subject, formatter: Formatter? = nil) where Subject: ReferenceConvertible {
    // Formatted as the Objective-C object it bridges to (an NSDate, NSURL …).
    let object: AnyObject = subject._bridgeToObjectiveC()
    let s = formatter?.string(for: object) ?? String(describing: object)
    pommeAppend("%@", .init(pommeString: s))
  }

  // pomme: replaces public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation > public struct StringInterpolation : Swift.StringInterpolationProtocol :: public mutating func appendInterpolation<Subject>(_ subject: Subject, formatter: Foundation.Formatter? = nil) where Subject : ObjectiveC.NSObject
  @_semantics("swiftui.localized.appendInterpolation_@_specifier")
  public mutating func appendInterpolation<Subject>(_ subject: Subject, formatter: Formatter? = nil) where Subject: NSObject {
    let s = formatter?.string(for: subject) ?? subject.description
    pommeAppend("%@", .init(pommeString: s))
  }

  // pomme: replaces public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation > public struct StringInterpolation : Swift.StringInterpolationProtocol :: public mutating func appendInterpolation<F>(_ input: F.FormatInput, format: F) where F : Foundation.FormatStyle, F.FormatInput : Swift.Equatable, F.FormatOutput == Swift.String
  @available(iOS 15.0, macOS 12.0, tvOS 15.0, watchOS 8.0, *)
  @_semantics("swiftui.localized.appendInterpolation_@_specifier")
  public mutating func appendInterpolation<F>(_ input: F.FormatInput, format: F) where F: FormatStyle, F.FormatInput: Equatable, F.FormatOutput == String {
    pommeAppend("%@", .init(pommeString: format.format(input)))
  }

  // pomme: replaces public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation > public struct StringInterpolation : Swift.StringInterpolationProtocol :: public mutating func appendInterpolation(_ text: SwiftUICore.Text)
  @available(iOS 14.0, macOS 11.0, tvOS 14.0, watchOS 7.0, *)
  @_semantics("swiftui.localized.appendInterpolation_@_specifier")
  public mutating func appendInterpolation(_ text: Text) {
    pommeAppend("%@", .init(pommeString: text.pommeString))
  }

  // pomme: replaces public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation > public struct StringInterpolation : Swift.StringInterpolationProtocol :: public mutating func appendInterpolation(_ attributedString: Foundation.AttributedString)
  @available(macOS 12.0, iOS 15.0, tvOS 15.0, watchOS 8.0, *)
  @_semantics("swiftui.localized.appendInterpolation_@_specifier")
  public mutating func appendInterpolation(_ attributedString: AttributedString) {
    pommeAppend("%@", .init(pommeString: String(attributedString.characters)))
  }
}

extension LocalizedStringKey.StringInterpolation {
  mutating func pommeAppend(_ specifier: String, _ argument: LocalizedStringKey.FormatArgument) {
    pommeKey += specifier
    pommeArguments.append(argument)
  }
}

// pomme: in public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation
extension LocalizedStringKey {
  // pomme: replaces public struct LocalizedStringKey : Swift.Equatable, Swift.ExpressibleByStringInterpolation :: public init(stringInterpolation: SwiftUICore.LocalizedStringKey.StringInterpolation)
  @_semantics("swiftui.localized_string_key.init_interpolation")
  public init(stringInterpolation: LocalizedStringKey.StringInterpolation) {
    key = stringInterpolation.pommeKey
    hasFormatting = true
    arguments = stringInterpolation.pommeArguments
  }

  /// The key's arguments, formatted into `format` (the key's localization).
  package func pommeFormatted(_ format: String) -> String {
    String(format: format, arguments: arguments.map { $0.pommeValue })
  }
}

// The values interpolation formats: each as `String(format:)` takes it, and its specifier.

// pomme: in extension Swift.Int : SwiftUICore._FormatSpecifiable
extension Int {
  // pomme: replaces extension Swift.Int : SwiftUICore._FormatSpecifiable :: public var _arg: Swift.Int64
  public var _arg: Int64 { Int64(self) }
  // pomme: replaces extension Swift.Int : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%lld" }
}

// pomme: in extension Swift.Int8 : SwiftUICore._FormatSpecifiable
extension Int8 {
  // pomme: replaces extension Swift.Int8 : SwiftUICore._FormatSpecifiable :: public var _arg: Swift.Int32
  public var _arg: Int32 { Int32(self) }
  // pomme: replaces extension Swift.Int8 : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%d" }
}

// pomme: in extension Swift.Int16 : SwiftUICore._FormatSpecifiable
extension Int16 {
  // pomme: replaces extension Swift.Int16 : SwiftUICore._FormatSpecifiable :: public var _arg: Swift.Int32
  public var _arg: Int32 { Int32(self) }
  // pomme: replaces extension Swift.Int16 : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%d" }
}

// pomme: in extension Swift.Int32 : SwiftUICore._FormatSpecifiable
extension Int32 {
  // pomme: replaces extension Swift.Int32 : SwiftUICore._FormatSpecifiable :: public var _arg: Swift.Int32
  public var _arg: Int32 { Int32(self) }
  // pomme: replaces extension Swift.Int32 : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%d" }
}

// pomme: in extension Swift.Int64 : SwiftUICore._FormatSpecifiable
extension Int64 {
  // pomme: replaces extension Swift.Int64 : SwiftUICore._FormatSpecifiable :: public var _arg: Swift.Int64
  public var _arg: Int64 { Int64(self) }
  // pomme: replaces extension Swift.Int64 : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%lld" }
}

// pomme: in extension Swift.UInt : SwiftUICore._FormatSpecifiable
extension UInt {
  // pomme: replaces extension Swift.UInt : SwiftUICore._FormatSpecifiable :: public var _arg: Swift.UInt64
  public var _arg: UInt64 { UInt64(self) }
  // pomme: replaces extension Swift.UInt : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%llu" }
}

// pomme: in extension Swift.UInt8 : SwiftUICore._FormatSpecifiable
extension UInt8 {
  // pomme: replaces extension Swift.UInt8 : SwiftUICore._FormatSpecifiable :: public var _arg: Swift.UInt32
  public var _arg: UInt32 { UInt32(self) }
  // pomme: replaces extension Swift.UInt8 : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%u" }
}

// pomme: in extension Swift.UInt16 : SwiftUICore._FormatSpecifiable
extension UInt16 {
  // pomme: replaces extension Swift.UInt16 : SwiftUICore._FormatSpecifiable :: public var _arg: Swift.UInt32
  public var _arg: UInt32 { UInt32(self) }
  // pomme: replaces extension Swift.UInt16 : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%u" }
}

// pomme: in extension Swift.UInt32 : SwiftUICore._FormatSpecifiable
extension UInt32 {
  // pomme: replaces extension Swift.UInt32 : SwiftUICore._FormatSpecifiable :: public var _arg: Swift.UInt32
  public var _arg: UInt32 { UInt32(self) }
  // pomme: replaces extension Swift.UInt32 : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%u" }
}

// pomme: in extension Swift.UInt64 : SwiftUICore._FormatSpecifiable
extension UInt64 {
  // pomme: replaces extension Swift.UInt64 : SwiftUICore._FormatSpecifiable :: public var _arg: Swift.UInt64
  public var _arg: UInt64 { UInt64(self) }
  // pomme: replaces extension Swift.UInt64 : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%llu" }
}

// pomme: in extension Swift.Float : SwiftUICore._FormatSpecifiable
extension Float {
  // pomme: replaces extension Swift.Float : SwiftUICore._FormatSpecifiable :: public var _arg: Swift.Float
  public var _arg: Float { Float(self) }
  // pomme: replaces extension Swift.Float : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%f" }
}

// pomme: in extension Swift.Double : SwiftUICore._FormatSpecifiable
extension Double {
  // pomme: replaces extension Swift.Double : SwiftUICore._FormatSpecifiable :: public var _arg: Swift.Double
  public var _arg: Double { Double(self) }
  // pomme: replaces extension Swift.Double : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%lf" }
}

// pomme: in extension CoreFoundation.CGFloat : SwiftUICore._FormatSpecifiable
extension CGFloat {
  // pomme: replaces extension CoreFoundation.CGFloat : SwiftUICore._FormatSpecifiable :: public var _arg: CoreFoundation.CGFloat
  public var _arg: CGFloat { self }
  // pomme: replaces extension CoreFoundation.CGFloat : SwiftUICore._FormatSpecifiable :: public var _specifier: Swift.String
  public var _specifier: String { "%lf" }
}
