// The app's translations in SwiftUI, for tests/localized.rs: Text with a
// key, a plural through string interpolation (key `items %lld`, from the
// .stringsdict) and String(localized:), from resources/<language>.lproj.
// Each value is also printed as `VALUE name text` (recorded on the Simulator
// with scripts/swiftui/metrics-simulator.sh, VARIANT=<locale>).

import SwiftUI

@main
struct LocalizedApp: App {
  init() {
    for (name, value) in [
      ("greeting", String(localized: "greeting")),
      ("items.1", String(localized: "items \(1)")),
      ("items.3", String(localized: "items \(3)")),
      ("farewell", String(localized: "farewell", table: "Other")),
      ("missing", String(localized: "missing.key", defaultValue: "Default")),
      ("nslocalized", NSLocalizedString("greeting", comment: "")),
      ("format", String(format: NSLocalizedString("items %lld", comment: ""), 0)),
    ] {
      print("VALUE \(name) \(value)")
    }
  }

  var body: some Scene {
    WindowGroup {
      VStack {
        Text("greeting").accessibilityIdentifier("greeting")
        Text("items \(3)").accessibilityIdentifier("items")
        Text(String(localized: "farewell", table: "Other")).accessibilityIdentifier("farewell")
      }
    }
  }
}
