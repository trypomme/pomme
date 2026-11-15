// The device's settings for Swift's `Locale.current` and `TimeZone.current`,
// as Apple's framework reads them. pomme writes the device's language and
// region settings into the global user defaults domain (AppleLocale,
// AppleLanguages, the units and the 24-Hour Time switch), and NSUserDefaults
// looks there after the launch arguments and the app's own domain, as on iOS.
// The time zone is pomme's NSTimeZone.systemTimeZone.
// (swift-foundation-pomme.patch makes Locale's and TimeZone's caches call these.)

import PommeObjC

private func _pommeDefault(_ key: String) -> Any? {
    NSUserDefaults.standard.object(forKey: key._bridgeToObjectiveC())
}

private func _pommeBool(_ key: String) -> Bool? {
    (_pommeDefault(key) as? NSNumber)?.boolValue
}

internal func _pommeLocalePreferences() -> LocalePreferences {
    var prefs = LocalePreferences()
    prefs.locale = _pommeDefault("AppleLocale") as? String
    prefs.languages = _pommeDefault("AppleLanguages") as? [String]
    prefs.metricUnits = _pommeBool("AppleMetricUnits")
    prefs.measurementUnits = .init(_pommeDefault("AppleMeasurementUnits") as? String)
    prefs.temperatureUnit = .init(_pommeDefault("AppleTemperatureUnit") as? String)
    prefs.force24Hour = _pommeBool("AppleICUForce24HourTime")
    prefs.force12Hour = _pommeBool("AppleICUForce12HourTime")
    return prefs
}

internal func _pommePreferredLanguages() -> [String] {
    (_pommeLocalePreferences().languages ?? []).compactMap { Locale.canonicalLanguageIdentifier(from: $0) }
}


/// The device's time zone, as pomme's NSTimeZone has it (the setting's, else GMT).
internal func _pommeSystemTimeZoneIdentifier() -> String? {
    let name = String._unconditionallyBridgeFromObjectiveC(NSTimeZone.system.name)
    return name.isEmpty ? nil : name
}
