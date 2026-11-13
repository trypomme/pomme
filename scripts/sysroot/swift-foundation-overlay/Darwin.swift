// pomme: the parts of Apple's Foundation overlay that swift-foundation only
// builds in "framework mode" or leaves to the Objective-C side: CharacterSet,
// URLRequest, Notification, URLError and NSError bridging, the
// NSData-flavoured Data APIs, URL resource values. Same names and signatures
// as Apple's, so apps' imports resolve to these.

import PommeObjC

// MARK: CharacterSet (pure Swift: Unicode properties and scalar ranges)

public struct CharacterSet: Hashable, Sendable {
    indirect enum Node: Hashable, Sendable {
        case ranges([ClosedRange<UInt32>])
        case alphanumerics, letters, decimalDigits, whitespaces, newlines, punctuation, symbols, uppercase, lowercase
        case union(Node, Node)
        case intersection(Node, Node)
        case inverted(Node)

        func contains(_ s: Unicode.Scalar) -> Bool {
            let p = s.properties
            switch self {
            case .ranges(let rs): return rs.contains { $0.contains(s.value) }
            case .alphanumerics:
                switch p.generalCategory {
                case .uppercaseLetter, .lowercaseLetter, .titlecaseLetter, .modifierLetter, .otherLetter,
                     .nonspacingMark, .spacingMark, .enclosingMark, .decimalNumber, .letterNumber, .otherNumber: return true
                default: return false
                }
            case .letters:
                switch p.generalCategory {
                case .uppercaseLetter, .lowercaseLetter, .titlecaseLetter, .modifierLetter, .otherLetter,
                     .nonspacingMark, .spacingMark, .enclosingMark: return true
                default: return false
                }
            case .decimalDigits: return p.generalCategory == .decimalNumber
            case .whitespaces: return s == "\t" || p.generalCategory == .spaceSeparator
            case .newlines: return (0x0A...0x0D).contains(s.value) || s.value == 0x85 || s.value == 0x2028 || s.value == 0x2029
            case .punctuation:
                switch p.generalCategory {
                case .connectorPunctuation, .dashPunctuation, .openPunctuation, .closePunctuation,
                     .initialPunctuation, .finalPunctuation, .otherPunctuation: return true
                default: return false
                }
            case .symbols:
                switch p.generalCategory {
                case .mathSymbol, .currencySymbol, .modifierSymbol, .otherSymbol: return true
                default: return false
                }
            case .uppercase: return p.generalCategory == .uppercaseLetter || p.generalCategory == .titlecaseLetter
            case .lowercase: return p.generalCategory == .lowercaseLetter
            case .union(let a, let b): return a.contains(s) || b.contains(s)
            case .intersection(let a, let b): return a.contains(s) && b.contains(s)
            case .inverted(let a): return !a.contains(s)
            }
        }
    }

    var node: Node

    init(node: Node) { self.node = node }

    public init() { node = .ranges([]) }

    public init(charactersIn string: __shared String) {
        node = .ranges(string.unicodeScalars.map { $0.value...$0.value })
    }

    public init(charactersIn range: Range<Unicode.Scalar>) {
        node = range.isEmpty ? .ranges([]) : .ranges([range.lowerBound.value...(range.upperBound.value - 1)])
    }

    public init(charactersIn range: ClosedRange<Unicode.Scalar>) {
        node = .ranges([range.lowerBound.value...range.upperBound.value])
    }

    public static var alphanumerics: CharacterSet { CharacterSet(node: .alphanumerics) }
    public static var letters: CharacterSet { CharacterSet(node: .letters) }
    public static var decimalDigits: CharacterSet { CharacterSet(node: .decimalDigits) }
    public static var whitespaces: CharacterSet { CharacterSet(node: .whitespaces) }
    public static var newlines: CharacterSet { CharacterSet(node: .newlines) }
    public static var whitespacesAndNewlines: CharacterSet { CharacterSet(node: .union(.whitespaces, .newlines)) }
    public static var punctuationCharacters: CharacterSet { CharacterSet(node: .punctuation) }
    public static var symbols: CharacterSet { CharacterSet(node: .symbols) }
    public static var uppercaseLetters: CharacterSet { CharacterSet(node: .uppercase) }
    public static var lowercaseLetters: CharacterSet { CharacterSet(node: .lowercase) }

    // RFC 3986 sets, as Foundation defines them.
    private static func ascii(_ extra: String) -> CharacterSet {
        CharacterSet(node: .union(.ranges([0x30...0x39, 0x41...0x5A, 0x61...0x7A]), .ranges(extra.unicodeScalars.map { $0.value...$0.value })))
    }
    public static var urlQueryAllowed: CharacterSet { ascii("-._~!$&'()*+,;=:@/?") }
    public static var urlPathAllowed: CharacterSet { ascii("-._~!$&'()*+,;=:@/") }
    public static var urlHostAllowed: CharacterSet { ascii("-._~!$&'()*+,;=:[]") }
    public static var urlFragmentAllowed: CharacterSet { ascii("-._~!$&'()*+,;=:@/?") }
    public static var urlUserAllowed: CharacterSet { ascii("-._~!$&'()*+,;=") }
    public static var urlPasswordAllowed: CharacterSet { ascii("-._~!$&'()*+,;=") }

    public func contains(_ member: Unicode.Scalar) -> Bool { node.contains(member) }
    public func union(_ other: CharacterSet) -> CharacterSet { CharacterSet(node: .union(node, other.node)) }
    public func intersection(_ other: CharacterSet) -> CharacterSet { CharacterSet(node: .intersection(node, other.node)) }
    public func subtracting(_ other: CharacterSet) -> CharacterSet { CharacterSet(node: .intersection(node, .inverted(other.node))) }
    public var inverted: CharacterSet { CharacterSet(node: .inverted(node)) }
    public mutating func formUnion(_ other: CharacterSet) { node = .union(node, other.node) }
    public mutating func formIntersection(_ other: CharacterSet) { node = .intersection(node, other.node) }
    public mutating func insert(charactersIn string: String) { formUnion(CharacterSet(charactersIn: string)) }
    public mutating func remove(charactersIn string: String) { self = subtracting(CharacterSet(charactersIn: string)) }
    public mutating func invert() { node = .inverted(node) }
}

extension StringProtocol {
    public func trimmingCharacters(in set: CharacterSet) -> String {
        let scalars = Array(String(self).unicodeScalars)
        var lo = 0, hi = scalars.count
        while lo < hi, set.contains(scalars[lo]) { lo += 1 }
        while hi > lo, set.contains(scalars[hi - 1]) { hi -= 1 }
        var out = String.UnicodeScalarView()
        out.append(contentsOf: scalars[lo..<hi])
        return String(out)
    }

    public func components(separatedBy separator: CharacterSet) -> [String] {
        var parts: [String] = []
        var current = String.UnicodeScalarView()
        for s in String(self).unicodeScalars {
            if separator.contains(s) {
                parts.append(String(current))
                current = String.UnicodeScalarView()
            } else {
                current.append(s)
            }
        }
        parts.append(String(current))
        return parts
    }

    public func addingPercentEncoding(withAllowedCharacters allowed: CharacterSet) -> String? {
        var out = ""
        for s in String(self).unicodeScalars {
            if allowed.contains(s) {
                out.unicodeScalars.append(s)
            } else {
                for b in String(s).utf8 {
                    out += "%" + String(b >> 4, radix: 16, uppercase: true) + String(b & 0xF, radix: 16, uppercase: true)
                }
            }
        }
        return out
    }
}

// MARK: String searching with NSStringCompareOptions (no regular expressions)

extension String {
    public init(contentsOf url: __shared URL) throws {
        try self.init(contentsOf: url, encoding: .utf8)
    }

    /// printf-style formatting, done by pomme's `-[NSString initWithFormat:arguments:]`.
    public init(format: __shared String, _ arguments: CVarArg...) {
        self = withVaList(arguments) {
            String._unconditionallyBridgeFromObjectiveC(NSString(format: format._bridgeToObjectiveC(), arguments: $0))
        }
    }
}

/// Where `needle` occurs in `haystack[within]`, honouring case-insensitive,
/// anchored and backwards search.
func _pommeFind(_ needle: String, in haystack: String, options: NSStringCompareOptions, within: Range<String.Index>?) -> Range<String.Index>? {
    let range = within ?? haystack.startIndex..<haystack.endIndex
    if needle.isEmpty { return nil }
    let fold = options.contains(.caseInsensitiveSearch)
    let n = Array(fold ? needle.lowercased() : needle)
    var starts: [String.Index] = []
    var i = range.lowerBound
    while i < range.upperBound {
        starts.append(i)
        i = haystack.index(after: i)
    }
    if options.contains(.backwardsSearch) { starts.reverse() }
    for (k, start) in starts.enumerated() {
        if options.contains(.anchoredSearch) && k > 0 { break }
        var j = start
        var matched = true
        for c in n {
            guard j < range.upperBound else { matched = false; break }
            let h = haystack[j]
            if (fold ? Character(h.lowercased()) : h) != c { matched = false; break }
            j = haystack.index(after: j)
        }
        if matched { return start..<j }
    }
    return nil
}

extension StringProtocol {
    public func range<T: StringProtocol>(of aString: T, options: NSStringCompareOptions = [], range: Range<String.Index>? = nil, locale: Locale? = nil) -> Range<String.Index>? {
        _pommeFind(String(aString), in: String(self), options: options, within: range)
    }

    public func contains<T: StringProtocol>(_ other: T) -> Bool {
        _pommeFind(String(other), in: String(self), options: [], within: nil) != nil
    }

    public func replacingOccurrences<Target: StringProtocol, Replacement: StringProtocol>(of target: Target, with replacement: Replacement, options: NSStringCompareOptions = [], range searchRange: Range<String.Index>? = nil) -> String {
        var s = String(self)
        let t = String(target), r = String(replacement)
        var from = searchRange?.lowerBound ?? s.startIndex
        var end = searchRange.map { s.distance(from: s.startIndex, to: $0.upperBound) } ?? s.count
        while let found = _pommeFind(t, in: s, options: options.subtracting(.backwardsSearch), within: from..<s.index(s.startIndex, offsetBy: end)) {
            let offset = s.distance(from: s.startIndex, to: found.lowerBound)
            let removed = s.distance(from: found.lowerBound, to: found.upperBound)
            s.replaceSubrange(found, with: r)
            end += r.count - removed
            from = s.index(s.startIndex, offsetBy: offset + r.count)
            if options.contains(.anchoredSearch) { break }
        }
        return s
    }
}

// MARK: NSError bridging

extension NSError: Error {}

/// The runtime looks this conformance up by name (`$sSo10CFErrorRefas5Error10FoundationMc`)
/// to box NSErrors as Swift errors.
extension CFError: @retroactive Error {}

/// What `error as NSError` does (the runtime finds this by name).
public func _convertErrorToNSError(_ error: Error) -> NSError {
    if let ns = error as AnyObject as? NSError {
        return ns
    }
    if let custom = error as? any _BridgedStoredNSError {
        return custom._nsError
    }
    let info = ["NSLocalizedDescription" as NSString: "\(error)"._bridgeToObjectiveC()] as [NSString: NSString]
    return NSError(domain: String(reflecting: type(of: error))._bridgeToObjectiveC(), code: 1, userInfo: info._bridgeToObjectiveC())
}

extension Error {
    /// What NSError says about this error.
    public var localizedDescription: String {
        String._unconditionallyBridgeFromObjectiveC(_convertErrorToNSError(self).localizedDescription)
    }
}

/// What `nsError as Error` does.
public func _convertNSErrorToError(_ error: NSError?) -> Error {
    if let error { return error }
    return NSError(domain: "Foundation._GenericObjCError"._bridgeToObjectiveC(), code: 0, userInfo: nil)
}

/// An error code type that can be matched against a bridged error.
public protocol _ErrorCodeProtocol: Equatable {
    associatedtype _ErrorType: _BridgedStoredNSError where _ErrorType.Code == Self
}

/// A Swift error type that stores an NSError (URLError, CocoaError, ...).
public protocol _BridgedStoredNSError: Error, Hashable {
    associatedtype Code: _ErrorCodeProtocol, RawRepresentable where Code.RawValue: FixedWidthInteger
    var _nsError: NSError { get }
    init(_nsError error: NSError)
}

extension _BridgedStoredNSError {
    public var code: Code {
        Code(rawValue: Code.RawValue(truncatingIfNeeded: _nsError.code))!
    }

    public static func == (a: Self, b: Self) -> Bool { a._nsError.isEqual(b._nsError) }
    public func hash(into hasher: inout Hasher) { hasher.combine(_nsError.hash) }
}

public struct URLError: _BridgedStoredNSError {
    public let _nsError: NSError

    public init(_nsError error: NSError) { _nsError = error }

    public static var errorDomain: String { "NSURLErrorDomain" }

    public struct Code: RawRepresentable, Hashable, Sendable, _ErrorCodeProtocol {
        public typealias _ErrorType = URLError
        public let rawValue: Int
        public init(rawValue: Int) { self.rawValue = rawValue }

        public static var unknown: Code { Code(rawValue: -1) }
        public static var cancelled: Code { Code(rawValue: -999) }
        public static var badURL: Code { Code(rawValue: -1000) }
        public static var timedOut: Code { Code(rawValue: -1001) }
        public static var unsupportedURL: Code { Code(rawValue: -1002) }
        public static var cannotFindHost: Code { Code(rawValue: -1003) }
        public static var cannotConnectToHost: Code { Code(rawValue: -1004) }
        public static var networkConnectionLost: Code { Code(rawValue: -1005) }
        public static var notConnectedToInternet: Code { Code(rawValue: -1009) }
        public static var badServerResponse: Code { Code(rawValue: -1011) }
    }

    public init(_ code: Code, userInfo: [String: Any] = [:]) {
        _nsError = NSError(domain: "NSURLErrorDomain"._bridgeToObjectiveC(), code: code.rawValue, userInfo: nil)
    }
}

// MARK: Data, Date, Locale ↔ Objective-C

extension Data: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSData {
        withUnsafeBytes { NSData(bytes: $0.baseAddress, length: UInt($0.count)) }
    }
    public static func _forceBridgeFromObjectiveC(_ x: NSData, result: inout Data?) {
        result = _unconditionallyBridgeFromObjectiveC(x)
    }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSData, result: inout Data?) -> Bool {
        result = _unconditionallyBridgeFromObjectiveC(x)
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSData?) -> Data {
        guard let source, source.length > 0, let p = source.bytes else { return Data() }
        return Data(bytes: p, count: Int(source.length))
    }

    public init(contentsOf url: __shared URL, options: NSDataReadingOptions) throws {
        try self.init(contentsOf: url, options: Data.ReadingOptions(rawValue: options.rawValue))
    }

    public func write(to url: URL, options: NSDataWritingOptions) throws {
        try write(to: url, options: Data.WritingOptions(rawValue: options.rawValue))
    }

    public init?(base64Encoded base64String: __shared String, options: NSDataBase64DecodingOptions) {
        self.init(base64Encoded: base64String, options: Data.Base64DecodingOptions(rawValue: options.rawValue))
    }

    public func base64EncodedString(options: NSDataBase64EncodingOptions) -> String {
        base64EncodedString(options: Data.Base64EncodingOptions(rawValue: options.rawValue))
    }
}

extension Date: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSDate { NSDate(timeIntervalSinceReferenceDate: timeIntervalSinceReferenceDate) }
    public static func _forceBridgeFromObjectiveC(_ x: NSDate, result: inout Date?) {
        result = Date(timeIntervalSinceReferenceDate: x.timeIntervalSinceReferenceDate)
    }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSDate, result: inout Date?) -> Bool {
        result = Date(timeIntervalSinceReferenceDate: x.timeIntervalSinceReferenceDate)
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSDate?) -> Date {
        Date(timeIntervalSinceReferenceDate: source?.timeIntervalSinceReferenceDate ?? 0)
    }
}

// DateFormatter with Swift's types, as Apple's overlay gives it.
extension DateFormatter {
    public func string(from date: Date) -> String {
        String._unconditionallyBridgeFromObjectiveC(string(from: date._bridgeToObjectiveC()) as NSString)
    }
    public func date(from string: String) -> Date? {
        let d: NSDate? = date(from: string._bridgeToObjectiveC())
        return d.map { Date._unconditionallyBridgeFromObjectiveC($0) }
    }
}

// Locale, TimeZone and Calendar bridge by identifier, as their Foundation
// classes and Swift values correspond.
extension Locale: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSLocale { NSLocale(localeIdentifier: identifier._bridgeToObjectiveC()) }
    public static func _forceBridgeFromObjectiveC(_ x: NSLocale, result: inout Locale?) {
        result = Locale(identifier: String._unconditionallyBridgeFromObjectiveC(x.localeIdentifier))
    }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSLocale, result: inout Locale?) -> Bool {
        _forceBridgeFromObjectiveC(x, result: &result)
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSLocale?) -> Locale {
        guard let source else { return Locale(identifier: "") }
        return Locale(identifier: String._unconditionallyBridgeFromObjectiveC(source.localeIdentifier))
    }
}

extension TimeZone: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSTimeZone {
        NSTimeZone(name: identifier._bridgeToObjectiveC()) ?? NSTimeZone(forSecondsFromGMT: secondsFromGMT())
    }
    public static func _forceBridgeFromObjectiveC(_ x: NSTimeZone, result: inout TimeZone?) {
        result = TimeZone(identifier: String._unconditionallyBridgeFromObjectiveC(x.name)) ?? TimeZone(secondsFromGMT: x.secondsFromGMT)
    }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSTimeZone, result: inout TimeZone?) -> Bool {
        _forceBridgeFromObjectiveC(x, result: &result)
        return result != nil
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSTimeZone?) -> TimeZone {
        var result: TimeZone?
        if let source { _forceBridgeFromObjectiveC(source, result: &result) }
        return result ?? TimeZone(secondsFromGMT: 0)!
    }
}

extension Calendar: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSCalendar {
        let c = NSCalendar(identifier: Calendar._objCName(identifier)._bridgeToObjectiveC()) ?? NSCalendar(identifier: "gregorian"._bridgeToObjectiveC())!
        c.timeZone = timeZone._bridgeToObjectiveC()
        c.firstWeekday = UInt(firstWeekday)
        c.minimumDaysInFirstWeek = UInt(minimumDaysInFirstWeek)
        return c
    }
    public static func _forceBridgeFromObjectiveC(_ x: NSCalendar, result: inout Calendar?) {
        var c = Calendar(identifier: _swiftIdentifier(String._unconditionallyBridgeFromObjectiveC(x.calendarIdentifier)))
        if let zone = x.timeZone { c.timeZone = TimeZone._unconditionallyBridgeFromObjectiveC(zone) }
        c.firstWeekday = Int(x.firstWeekday)
        c.minimumDaysInFirstWeek = Int(x.minimumDaysInFirstWeek)
        result = c
    }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSCalendar, result: inout Calendar?) -> Bool {
        _forceBridgeFromObjectiveC(x, result: &result)
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSCalendar?) -> Calendar {
        var result: Calendar?
        if let source { _forceBridgeFromObjectiveC(source, result: &result) }
        return result ?? Calendar(identifier: .gregorian)
    }

    /// NSCalendarIdentifier's strings for Calendar.Identifier.
    static func _objCName(_ id: Calendar.Identifier) -> String {
        switch id {
        case .gregorian: "gregorian"
        case .buddhist: "buddhist"
        case .chinese: "chinese"
        case .coptic: "coptic"
        case .ethiopicAmeteMihret: "ethiopic"
        case .ethiopicAmeteAlem: "ethiopic-amete-alem"
        case .hebrew: "hebrew"
        case .iso8601: "iso8601"
        case .indian: "indian"
        case .islamic: "islamic"
        case .islamicCivil: "islamic-civil"
        case .japanese: "japanese"
        case .persian: "persian"
        case .republicOfChina: "roc"
        case .islamicTabular: "islamic-tbla"
        case .islamicUmmAlQura: "islamic-umalqura"
        default: "gregorian"
        }
    }

    static func _swiftIdentifier(_ name: String) -> Calendar.Identifier {
        let all: [Calendar.Identifier] = [.gregorian, .buddhist, .chinese, .coptic, .ethiopicAmeteMihret, .ethiopicAmeteAlem, .hebrew, .iso8601, .indian, .islamic, .islamicCivil, .japanese, .persian, .republicOfChina, .islamicTabular, .islamicUmmAlQura]
        return all.first { _objCName($0) == name } ?? .gregorian
    }
}

// MARK: NSString-wrapper keys (Hashable by their string value)

extension NSNotificationName: @retroactive Hashable {
    public static func == (a: Self, b: Self) -> Bool {
        String._unconditionallyBridgeFromObjectiveC(a.rawValue) == String._unconditionallyBridgeFromObjectiveC(b.rawValue)
    }
    public func hash(into hasher: inout Hasher) { hasher.combine(String._unconditionallyBridgeFromObjectiveC(rawValue)) }
}

extension NSURLResourceKey: @retroactive Hashable {
    public static func == (a: Self, b: Self) -> Bool {
        String._unconditionallyBridgeFromObjectiveC(a.rawValue) == String._unconditionallyBridgeFromObjectiveC(b.rawValue)
    }
    public func hash(into hasher: inout Hasher) { hasher.combine(String._unconditionallyBridgeFromObjectiveC(rawValue)) }
}

// MARK: Notification

public struct Notification: Hashable {
    public var name: NSNotificationName
    public var object: Any?
    public var userInfo: [AnyHashable: Any]?

    public init(name: NSNotificationName, object: Any? = nil, userInfo: [AnyHashable: Any]? = nil) {
        self.name = name
        self.object = object
        self.userInfo = userInfo
    }

    public static func == (a: Notification, b: Notification) -> Bool { a.name == b.name }
    public func hash(into hasher: inout Hasher) { hasher.combine(name) }

    public func _bridgeToObjectiveC() -> NSNotification {
        let obj: AnyObject? = object.map { _bridgeAnythingToObjectiveC($0) }
        return NSNotification(name: name, object: obj, userInfo: userInfo?._bridgeToObjectiveC())
    }

    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSNotification?) -> Notification {
        guard let source else { return Notification(name: NSNotificationName(rawValue: ""._bridgeToObjectiveC())) }
        var info: [AnyHashable: Any]?
        if let d = source.userInfo {
            info = [AnyHashable: Any]()
            for key in [AnyObject](_immutableCocoaArray: d.allKeys) {
                if let k = key as? AnyHashable { info![k] = d.object(forKey: key) }
            }
        }
        return Notification(name: source.name, object: source.object, userInfo: info)
    }
}

// MARK: URLRequest

public struct URLRequest: Hashable, Sendable {
    public typealias CachePolicy = NSURLRequestCachePolicy

    public var url: URL?
    public var cachePolicy: NSURLRequestCachePolicy
    public var timeoutInterval: Double
    public var httpMethod: String?
    public var allHTTPHeaderFields: [String: String]?
    public var httpBody: Data?
    public var httpBodyStream: NSInputStream? { nil }

    public init(url: URL, cachePolicy: NSURLRequestCachePolicy = .useProtocolCachePolicy, timeoutInterval: Double = 60.0) {
        self.url = url
        self.cachePolicy = cachePolicy
        self.timeoutInterval = timeoutInterval
        self.httpMethod = "GET"
    }

    public func value(forHTTPHeaderField field: String) -> String? {
        allHTTPHeaderFields?.first { $0.key.lowercased() == field.lowercased() }?.value
    }

    public mutating func setValue(_ value: String?, forHTTPHeaderField field: String) {
        var h = allHTTPHeaderFields ?? [:]
        if let existing = h.keys.first(where: { $0.lowercased() == field.lowercased() }) {
            h[existing] = nil
        }
        if let value { h[field] = value }
        allHTTPHeaderFields = h
    }

    public mutating func addValue(_ value: String, forHTTPHeaderField field: String) {
        setValue(self.value(forHTTPHeaderField: field).map { "\($0),\(value)" } ?? value, forHTTPHeaderField: field)
    }

    public static func == (a: URLRequest, b: URLRequest) -> Bool {
        a.url == b.url && a.httpMethod == b.httpMethod && a.allHTTPHeaderFields == b.allHTTPHeaderFields && a.httpBody == b.httpBody
    }
    public func hash(into hasher: inout Hasher) { hasher.combine(url) }

    public func _bridgeToObjectiveC() -> NSURLRequest {
        let r = NSMutableURLRequest(url: url!._bridgeToObjectiveC(), cachePolicy: cachePolicy, timeoutInterval: timeoutInterval)
        if let httpMethod { r.setHTTPMethod(httpMethod._bridgeToObjectiveC()) }
        for (k, v) in allHTTPHeaderFields ?? [:] { r.setValue(v._bridgeToObjectiveC(), forHTTPHeaderField: k._bridgeToObjectiveC()) }
        if let httpBody { r.setHTTPBody(httpBody._bridgeToObjectiveC()) }
        return r
    }

    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSURLRequest?) -> URLRequest {
        let source = source!
        var r = URLRequest(url: URL._unconditionallyBridgeFromObjectiveC(source.url), cachePolicy: source.cachePolicy, timeoutInterval: source.timeoutInterval)
        r.httpMethod = source.httpMethod.map { String._unconditionallyBridgeFromObjectiveC($0) }
        if let h = source.allHTTPHeaderFields {
            var out: [String: String] = [:]
            for key in [AnyObject](_immutableCocoaArray: h.allKeys) {
                if let k = key as? String, let v = h.object(forKey: key) as? String { out[k] = v }
            }
            r.allHTTPHeaderFields = out
        }
        r.httpBody = source.httpBody.map { Data._unconditionallyBridgeFromObjectiveC($0) }
        return r
    }
}

extension NSURLRequestCachePolicy: @retroactive @unchecked Sendable {}
extension NSInputStream: @retroactive @unchecked Sendable {}

// MARK: URL resource values (a fixed, roomy device)

public struct URLResourceValues {
    var values: [NSURLResourceKey: Any]
    public init() { values = [:] }
    public var volumeTotalCapacity: Int? { values[NSURLResourceKey(rawValue: "NSURLVolumeTotalCapacityKey"._bridgeToObjectiveC())] as? Int }
    public var volumeAvailableCapacityForImportantUsage: Int64? {
        values[NSURLResourceKey(rawValue: "NSURLVolumeAvailableCapacityForImportantUsageKey"._bridgeToObjectiveC())] as? Int64
    }
}

extension URL {
    // Apple's Set<NSURLResourceKey> uses the importer's Hashable; ours is retroactive and
    // would mangle differently, so pin the name.
    @_silgen_name("$s10Foundation3URLV14resourceValues7forKeysAA011URLResourceD0VShySo16NSURLResourceKeyaG_tKF")
    public func resourceValues(forKeys keys: Set<NSURLResourceKey>) throws -> URLResourceValues {
        var v = URLResourceValues()
        v.values[NSURLResourceKey(rawValue: "NSURLVolumeTotalCapacityKey"._bridgeToObjectiveC())] = 128_000_000_000
        v.values[NSURLResourceKey(rawValue: "NSURLVolumeAvailableCapacityForImportantUsageKey"._bridgeToObjectiveC())] = Int64(64_000_000_000)
        return v
    }

    public func startAccessingSecurityScopedResource() -> Bool { true }
    public func stopAccessingSecurityScopedResource() {}
}

// MARK: Objective-C collections as Swift sequences

extension NSArray: @retroactive Sequence {
    public func makeIterator() -> NSFastEnumerationIterator { NSFastEnumerationIterator(self) }
}

extension NSSet: @retroactive Sequence {
    public func makeIterator() -> NSFastEnumerationIterator { NSFastEnumerationIterator(self) }
}

extension NSDictionary: @retroactive Sequence {
    // A class, as in Apple's overlay (the mangled names say so).
    final public class Iterator: IteratorProtocol {
        var keys: NSFastEnumerationIterator
        let dictionary: NSDictionary
        init(keys: NSFastEnumerationIterator, dictionary: NSDictionary) {
            self.keys = keys
            self.dictionary = dictionary
        }
        public func next() -> (key: Any, value: Any)? {
            guard let key = keys.next() else { return nil }
            return (key, dictionary.object(forKey: key as AnyObject) as Any)
        }
    }
    public func makeIterator() -> Iterator { Iterator(keys: NSFastEnumerationIterator(self), dictionary: self) }
}

// MARK: KVO and fast enumeration

public class NSKeyValueObservation: NSObject {
    public func invalidate() {}
}

public struct NSFastEnumerationIterator: IteratorProtocol {
    let enumerable: NSFastEnumeration
    var objects: [AnyObject]?
    var index = 0

    public init(_ enumerable: NSFastEnumeration) { self.enumerable = enumerable }

    public mutating func next() -> Any? {
        if objects == nil {
            var all: [AnyObject] = []
            var state = NSFastEnumerationState()
            let buffer = UnsafeMutablePointer<AnyObject?>.allocate(capacity: 16)
            defer { buffer.deallocate() }
            while true {
                let n = withUnsafeMutablePointer(to: &state) { s in
                    buffer.withMemoryRebound(to: Unmanaged<AnyObject>?.self, capacity: 16) { _ in
                        enumerable.countByEnumerating(with: s, objects: AutoreleasingUnsafeMutablePointer(buffer), count: 16)
                    }
                }
                if n == 0 { break }
                for i in 0..<Int(n) {
                    if let item = state.itemsPtr?[i] { all.append(item) }
                }
            }
            objects = all
        }
        guard let objects, index < objects.count else { return nil }
        defer { index += 1 }
        return objects[index]
    }
}
