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
    public func _bridgeToObjectiveC() -> NSDictionary {
        unsafeBitCast(_bridgeToObjectiveCImpl(), to: NSDictionary.self)
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
