// String(localized:) and String.LocalizationValue, as in Apple's Foundation
// (open-source swift-foundation doesn't have them), with Apple's signatures,
// so apps bind to them by the same symbols. A value is a format key
// (`items %lld`) and its arguments; the string is the key's translation in
// the bundle (NSBundle: pomme's, with .strings and .stringsdict plurals),
// formatted with the arguments by NSString. (The translation stays the
// NSString NSBundle gave, which pomme's formatter knows a plural format by.)

import PommeObjC

@available(macOS 12, iOS 15, tvOS 15, watchOS 8, *)
@preconcurrency public protocol _FormatSpecifiable: Equatable, Sendable {
    associatedtype _Arg: CVarArg, Sendable
    var _arg: _Arg { get }
}

extension Int: _FormatSpecifiable { public var _arg: Int64 { Int64(self) } }
extension Int8: _FormatSpecifiable { public var _arg: Int32 { Int32(self) } }
extension Int16: _FormatSpecifiable { public var _arg: Int32 { Int32(self) } }
extension Int32: _FormatSpecifiable { public var _arg: Int32 { self } }
extension Int64: _FormatSpecifiable { public var _arg: Int64 { self } }
extension UInt: _FormatSpecifiable { public var _arg: UInt64 { UInt64(self) } }
extension UInt8: _FormatSpecifiable { public var _arg: UInt32 { UInt32(self) } }
extension UInt16: _FormatSpecifiable { public var _arg: UInt32 { UInt32(self) } }
extension UInt32: _FormatSpecifiable { public var _arg: UInt32 { self } }
extension UInt64: _FormatSpecifiable { public var _arg: UInt64 { self } }
extension Float: _FormatSpecifiable { public var _arg: Float { self } }
extension Double: _FormatSpecifiable { public var _arg: Double { self } }

extension String {
    public struct LocalizationValue: Equatable, ExpressibleByStringInterpolation {
        /// The format key (`items %lld`), and its arguments.
        var _key: String
        var _arguments: [_Argument]

        /// One argument: as `String(format:)` takes it, and what it is (to compare by).
        struct _Argument: Equatable {
            let value: any CVarArg
            let description: String
            static func == (a: _Argument, b: _Argument) -> Bool { a.description == b.description }
        }

        public init(_ value: String) {
            _key = value
            _arguments = []
        }

        @_semantics("localization_key.init_literal")
        public init(stringLiteral value: String) {
            _key = value
            _arguments = []
        }

        @_semantics("localization_key.init_interpolation")
        public init(stringInterpolation: StringInterpolation) {
            _key = stringInterpolation._key
            _arguments = stringInterpolation._arguments
        }

        public struct StringInterpolation: StringInterpolationProtocol {
            var _key: String
            var _arguments: [_Argument]

            @_semantics("localization.interpolation_init")
            public init(literalCapacity: Int, interpolationCount: Int) {
                _key = ""
                _key.reserveCapacity(literalCapacity + 4 * interpolationCount)
                _arguments = []
            }

            @_semantics("localization.interpolation.appendLiteral")
            public mutating func appendLiteral(_ literal: String) {
                // A literal percent sign stays one once formatted.
                _key += literal.replacingOccurrences(of: "%", with: "%%")
            }

            @_semantics("localization.interpolation.appendInterpolation_@_specifier")
            public mutating func appendInterpolation(_ string: String) {
                _key += "%@"
                _arguments.append(_Argument(value: string._bridgeToObjectiveC(), description: string))
            }

            @_semantics("localization.interpolation.appendInterpolation_@_specifier")
            public mutating func appendInterpolation<Subject>(_ subject: Subject) where Subject: NSObject {
                _key += "%@"
                _arguments.append(_Argument(value: subject, description: String(describing: subject)))
            }

            @_semantics("localization.interpolation.appendInterpolation_param_specifier")
            public mutating func appendInterpolation<T>(_ value: T, specifier: String) where T: _FormatSpecifiable {
                _key += specifier
                _arguments.append(_Argument(value: value._arg, description: "\(value)"))
            }

            public typealias StringLiteralType = String
        }

        public static func == (a: LocalizationValue, b: LocalizationValue) -> Bool {
            a._key == b._key && a._arguments == b._arguments
        }

        public typealias ExtendedGraphemeClusterLiteralType = String
        public typealias StringLiteralType = String
        public typealias UnicodeScalarLiteralType = String
    }

    /// The translation of `key` (else `value`) in `bundle`'s `table`, formatted with `arguments`.
    static func _pommeLocalized(_ key: String, value: String, arguments: [LocalizationValue._Argument], table: String?, bundle: NSBundle?) -> String {
        let format = (bundle ?? NSBundle.main).localizedString(forKey: key._bridgeToObjectiveC(), value: value._bridgeToObjectiveC(), table: table?._bridgeToObjectiveC())
        if arguments.isEmpty {
            return String._unconditionallyBridgeFromObjectiveC(format)
        }
        return withVaList(arguments.map(\.value)) {
            String._unconditionallyBridgeFromObjectiveC(NSString(format: format, arguments: $0))
        }
    }

    @_semantics("string.init_localized")
    public init(localized keyAndValue: LocalizationValue, table: String? = nil, bundle: NSBundle? = nil, locale: Locale = .current, comment: StaticString? = nil) {
        self = String._pommeLocalized(keyAndValue._key, value: keyAndValue._key, arguments: keyAndValue._arguments, table: table, bundle: bundle)
    }

    @_semantics("string.init_localized")
    public init(localized key: StaticString, defaultValue: LocalizationValue, table: String? = nil, bundle: NSBundle? = nil, locale: Locale = .current, comment: StaticString? = nil) {
        self = String._pommeLocalized(key.description, value: defaultValue._key, arguments: defaultValue._arguments, table: table, bundle: bundle)
    }
}

extension String.LocalizationValue: @unchecked Sendable {}
extension String.LocalizationValue.StringInterpolation: @unchecked Sendable {}

/// The translation of `key` in `bundle`'s `tableName` (else `value`, else the key).
public func NSLocalizedString(_ key: String, tableName: String? = nil, bundle: NSBundle = NSBundle.main, value: String = "", comment: String) -> String {
    String._unconditionallyBridgeFromObjectiveC(bundle.localizedString(forKey: key._bridgeToObjectiveC(), value: value._bridgeToObjectiveC(), table: tableName?._bridgeToObjectiveC()))
}
