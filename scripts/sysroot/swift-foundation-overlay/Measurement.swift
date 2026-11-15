// Swift's Measurement, as in Apple's Foundation (open-source swift-foundation
// doesn't have it): a value in a unit (a Unit: NSUnit, which pomme implements).
// MeasurementFormatter formats one by way of NSMeasurement.

import PommeObjC

public struct Measurement<UnitType: Unit>: CustomStringConvertible {
    /// The unit component of the measurement.
    public let unit: UnitType

    /// The value component of the measurement.
    public var value: Double

    /// Create a measurement with a value and a unit.
    public init(value: Double, unit: UnitType) {
        self.value = value
        self.unit = unit
    }

    public var description: String {
        "\(value) \(String._unconditionallyBridgeFromObjectiveC(unit.symbol))"
    }
}

extension MeasurementFormatter {
    /// The measurement as text, as `-stringFromMeasurement:` writes it.
    public func string<UnitType>(from measurement: Measurement<UnitType>) -> String {
        let m = NSMeasurement(doubleValue: measurement.value, unit: measurement.unit)
        return String._unconditionallyBridgeFromObjectiveC(string(from: m))
    }
}
