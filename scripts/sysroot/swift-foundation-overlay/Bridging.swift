// pomme: Objective-C bridging for Swift Foundation.
//
// Apple's Foundation makes String, Array, Dictionary, Set and the number types
// `_ObjectiveCBridgeable` in "framework mode", which swift-foundation's
// open-source mode leaves out. These conformances use the same names and
// signatures, so they satisfy the app's imports and the runtime's dynamic
// casts (`as!` between Swift and Objective-C types). The Objective-C side is
// pomme's own Foundation classes, reached through ordinary message sends.

import PommeObjC

// Standard library hooks for Foundation (`@_spi(Foundation)`): exported by
// libswiftCore but not visible through the SDK's Swift interface.
@_silgen_name("$sSS12_cocoaStringSSyXl_tcfC")
internal func _pommeString(cocoa: __owned AnyObject) -> String
@_silgen_name("$sSS23_bridgeToObjectiveCImplyXlyF")
internal func _pommeObjC(_ string: String) -> AnyObject

// MARK: String

extension String: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSString {
        unsafeBitCast(_pommeObjC(self), to: NSString.self)
    }

    public static func _forceBridgeFromObjectiveC(_ x: NSString, result: inout String?) {
        result = _pommeString(cocoa: x)
    }

    public static func _conditionallyBridgeFromObjectiveC(_ x: NSString, result: inout String?) -> Bool {
        result = _pommeString(cocoa: x)
        return true
    }

    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSString?) -> String {
        guard let source else { return "" }
        return _pommeString(cocoa: source)
    }
}

// MARK: Array

extension Array: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSArray {
        unsafeBitCast(_bridgeToObjectiveCImpl(), to: NSArray.self)
    }

    public static func _forceBridgeFromObjectiveC(_ source: NSArray, result: inout [Element]?) {
        result = _unconditionallyBridgeFromObjectiveC(source)
    }

    public static func _conditionallyBridgeFromObjectiveC(_ source: NSArray, result: inout [Element]?) -> Bool {
        var out: [Element] = []
        for object in [AnyObject](_immutableCocoaArray: source) {
            guard let element = object as? Element else {
                result = nil
                return false
            }
            out.append(element)
        }
        result = out
        return true
    }

    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSArray?) -> [Element] {
        guard let source else { return [] }
        if let native = [Element]._bridgeFromObjectiveCAdoptingNativeStorageOf(source) {
            return native
        }
        if _isBridgedVerbatimToObjectiveC(Element.self) {
            return [Element](_immutableCocoaArray: source)
        }
        return [AnyObject](_immutableCocoaArray: source).map { $0 as! Element }
    }
}

// MARK: Dictionary

extension Dictionary: _ObjectiveCBridgeable {
    // Eagerly, into an ordinary NSDictionary (each key and value bridged now)
    // rather than the standard library's lazily bridging subclass, which
    // pomme's NSDictionary doesn't serve reliably.
    public func _bridgeToObjectiveC() -> NSDictionary {
        let keys = self.keys.map { _bridgeAnythingToObjectiveC($0) }
        let values = self.values.map { _bridgeAnythingToObjectiveC($0) }
        return keys.withUnsafeBufferPointer { k in
            values.withUnsafeBufferPointer { v in
                NSDictionary(objects: v.baseAddress, forKeys: k.baseAddress, count: UInt(count))
            }
        }
    }

    public static func _forceBridgeFromObjectiveC(_ source: NSDictionary, result: inout [Key: Value]?) {
        result = _unconditionallyBridgeFromObjectiveC(source)
    }

    public static func _conditionallyBridgeFromObjectiveC(_ source: NSDictionary, result: inout [Key: Value]?) -> Bool {
        var out: [Key: Value] = [:]
        for key in [AnyObject](_immutableCocoaArray: source.allKeys) {
            guard let k = key as? Key, let v = source.object(forKey: key) as? Value else {
                result = nil
                return false
            }
            out[k] = v
        }
        result = out
        return true
    }

    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSDictionary?) -> [Key: Value] {
        guard let source else { return [:] }
        if let native = [Key: Value]._bridgeFromObjectiveCAdoptingNativeStorageOf(source) {
            return native
        }
        if _isBridgedVerbatimToObjectiveC(Key.self) && _isBridgedVerbatimToObjectiveC(Value.self) {
            return [Key: Value](_immutableCocoaDictionary: source)
        }
        var out: [Key: Value] = [:]
        for key in [AnyObject](_immutableCocoaArray: source.allKeys) {
            out[key as! Key] = (source.object(forKey: key) as! Value)
        }
        return out
    }
}

// MARK: Set

extension Set: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSSet {
        unsafeBitCast(_bridgeToObjectiveCImpl(), to: NSSet.self)
    }

    public static func _forceBridgeFromObjectiveC(_ source: NSSet, result: inout Set<Element>?) {
        result = _unconditionallyBridgeFromObjectiveC(source)
    }

    public static func _conditionallyBridgeFromObjectiveC(_ source: NSSet, result: inout Set<Element>?) -> Bool {
        var out = Set<Element>()
        for object in [AnyObject](_immutableCocoaArray: source.allObjects) {
            guard let element = object as? Element else {
                result = nil
                return false
            }
            out.insert(element)
        }
        result = out
        return true
    }

    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSSet?) -> Set<Element> {
        guard let source else { return [] }
        if let native = Set<Element>._bridgeFromObjectiveCAdoptingNativeStorageOf(source) {
            return native
        }
        if _isBridgedVerbatimToObjectiveC(Element.self) {
            return Set<Element>(_immutableCocoaSet: source)
        }
        return Set([AnyObject](_immutableCocoaArray: source.allObjects).map { $0 as! Element })
    }
}

// MARK: Numbers

extension Bool: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSNumber { NSNumber(bool: self) }
    public static func _forceBridgeFromObjectiveC(_ x: NSNumber, result: inout Bool?) { result = x.boolValue }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSNumber, result: inout Bool?) -> Bool {
        result = x.boolValue
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSNumber?) -> Bool { source?.boolValue ?? false }
}

extension Int: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSNumber { NSNumber(longLong: Int64(self)) }
    public static func _forceBridgeFromObjectiveC(_ x: NSNumber, result: inout Int?) { result = Int(x.longLongValue) }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSNumber, result: inout Int?) -> Bool {
        result = Int(x.longLongValue)
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSNumber?) -> Int { Int(source?.longLongValue ?? 0) }
}

extension UInt: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSNumber { NSNumber(unsignedLongLong: UInt64(self)) }
    public static func _forceBridgeFromObjectiveC(_ x: NSNumber, result: inout UInt?) { result = UInt(x.unsignedLongLongValue) }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSNumber, result: inout UInt?) -> Bool {
        result = UInt(x.unsignedLongLongValue)
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSNumber?) -> UInt { UInt(source?.unsignedLongLongValue ?? 0) }
}

extension Int64: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSNumber { NSNumber(longLong: self) }
    public static func _forceBridgeFromObjectiveC(_ x: NSNumber, result: inout Int64?) { result = x.longLongValue }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSNumber, result: inout Int64?) -> Bool {
        result = x.longLongValue
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSNumber?) -> Int64 { source?.longLongValue ?? 0 }
}

extension Int32: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSNumber { NSNumber(longLong: Int64(self)) }
    public static func _forceBridgeFromObjectiveC(_ x: NSNumber, result: inout Int32?) { result = Int32(truncatingIfNeeded: x.longLongValue) }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSNumber, result: inout Int32?) -> Bool {
        result = Int32(truncatingIfNeeded: x.longLongValue)
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSNumber?) -> Int32 { Int32(truncatingIfNeeded: source?.longLongValue ?? 0) }
}

extension Double: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSNumber { NSNumber(double: self) }
    public static func _forceBridgeFromObjectiveC(_ x: NSNumber, result: inout Double?) { result = x.doubleValue }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSNumber, result: inout Double?) -> Bool {
        result = x.doubleValue
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSNumber?) -> Double { source?.doubleValue ?? 0 }
}

extension Float: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSNumber { NSNumber(float: self) }
    public static func _forceBridgeFromObjectiveC(_ x: NSNumber, result: inout Float?) { result = x.floatValue }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSNumber, result: inout Float?) -> Bool {
        result = x.floatValue
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSNumber?) -> Float { source?.floatValue ?? 0 }
}

// MARK: Literals

// `NSDictionary(dictionaryLiteral:)` and `NSArray(arrayLiteral:)`: Apple's
// overlay makes them ExpressibleBy…Literal; apps call the initializers by
// these names. (The metatype argument isn't needed: the class is fixed.)
@_silgen_name("$sSo12NSDictionaryC10FoundationE17dictionaryLiteralAByp_yptd_tcfC")
public func _pommeNSDictionaryLiteral(_ elements: [(Any, Any)]) -> NSDictionary {
    let keys = elements.map { _bridgeAnythingToObjectiveC($0.0) }
    let values = elements.map { _bridgeAnythingToObjectiveC($0.1) }
    return keys.withUnsafeBufferPointer { k in
        values.withUnsafeBufferPointer { v in
            NSDictionary(objects: v.baseAddress, forKeys: k.baseAddress, count: UInt(elements.count))
        }
    }
}

// `NSDictionary(dictionary:)`, the overlay's convenience initializer taking an
// NSDictionary: a copy of its entries (pomme's dictionaries are all mutable
// underneath, so NSMutableDictionary's use of it gets a mutable one too).
@_silgen_name("$sSo12NSDictionaryC10FoundationE10dictionaryA2Bh_tcfC")
public func _pommeNSDictionaryWithDictionary(_ other: NSDictionary) -> NSDictionary {
    NSDictionary(dictionary: other)
}

@_silgen_name("$sSo8NSStringC10FoundationE13stringLiteralABs12StaticStringV_tcfC")
public func _pommeNSStringLiteral(_ literal: StaticString) -> NSString {
    unsafeBitCast(_pommeObjC(literal.description), to: NSString.self)
}

@_silgen_name("$sSo7NSArrayC10FoundationE12arrayLiteralAByp_td_tcfC")
public func _pommeNSArrayLiteral(_ elements: [Any]) -> NSArray {
    let objects = elements.map { _bridgeAnythingToObjectiveC($0) }
    return objects.withUnsafeBufferPointer { NSArray(objects: $0.baseAddress, count: UInt(elements.count)) }
}

// The same initializer as newer compilers mangle its `Any...` (Swift 5's
// spelled the variadic element as a one-element tuple, `(Any)...`).
@_silgen_name("$sSo7NSArrayC10FoundationE12arrayLiteralABypd_tcfC")
public func _pommeNSArrayLiteralVariadic(_ elements: [Any]) -> NSArray {
    _pommeNSArrayLiteral(elements)
}

// MARK: AnyHashable

// An NSString in an AnyHashable is its String, so `[AnyHashable: Any]`
// dictionaries bridged from Objective-C (a notification's userInfo) can be
// looked up with Swift string keys.
extension NSString: _HasCustomAnyHashableRepresentation {
    public func _toCustomAnyHashable() -> AnyHashable? {
        AnyHashable(_pommeString(cocoa: self))
    }
}

// An AnyHashable bridges as the value it wraps (`[AnyHashable: Any]` is what
// `NSDictionary(dictionary:)` takes, so its keys come through here).
extension AnyHashable: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSObject {
        unsafeBitCast(_bridgeAnythingToObjectiveC(base), to: NSObject.self)
    }
    public static func _forceBridgeFromObjectiveC(_ x: NSObject, result: inout AnyHashable?) { result = AnyHashable(x) }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSObject, result: inout AnyHashable?) -> Bool {
        result = AnyHashable(x)
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSObject?) -> AnyHashable { AnyHashable(source!) }
}

// CGFloat bridges to NSNumber too (keyboard controller puts frames in an
// `[String: Any]` for React Native). On Apple platforms this conformance
// lives in Foundation as well.
extension CGFloat: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSNumber { NSNumber(double: Double(self)) }
    public static func _forceBridgeFromObjectiveC(_ x: NSNumber, result: inout CGFloat?) { result = CGFloat(x.doubleValue) }
    public static func _conditionallyBridgeFromObjectiveC(_ x: NSNumber, result: inout CGFloat?) -> Bool {
        result = CGFloat(x.doubleValue)
        return true
    }
    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSNumber?) -> CGFloat { CGFloat(source?.doubleValue ?? 0) }
}

// MARK: URL

extension URL: _ObjectiveCBridgeable {
    public func _bridgeToObjectiveC() -> NSURL {
        NSURL(string: absoluteString._bridgeToObjectiveC())!
    }

    public static func _forceBridgeFromObjectiveC(_ source: NSURL, result: inout URL?) {
        result = URL(string: String._unconditionallyBridgeFromObjectiveC(source.absoluteString))
    }

    public static func _conditionallyBridgeFromObjectiveC(_ source: NSURL, result: inout URL?) -> Bool {
        result = URL(string: String._unconditionallyBridgeFromObjectiveC(source.absoluteString))
        return result != nil
    }

    public static func _unconditionallyBridgeFromObjectiveC(_ source: NSURL?) -> URL {
        var result: URL?
        _forceBridgeFromObjectiveC(source!, result: &result)
        return result!
    }
}
