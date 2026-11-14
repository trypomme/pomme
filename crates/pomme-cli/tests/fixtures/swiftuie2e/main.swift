// A small SwiftUI app for tests/e2e.rs: a counter, a toggle and a name
// field, each with an accessibility identifier, driven the way XCUITest
// drives an app (pomme_hle::driver).

import SwiftUI

@main
struct E2EApp: App {
  var body: some Scene {
    WindowGroup {
      E2EView()
    }
  }
}

struct E2EView: View {
  @State private var count = 0
  @State private var notify = false
  @State private var name = ""

  var body: some View {
    Form {
      Section {
        Text("Count: \(count)").accessibilityIdentifier("count")
        Button("Add one") { count += 1 }.accessibilityIdentifier("add")
      }
      Section {
        Toggle("Notify me", isOn: $notify).accessibilityIdentifier("notify")
        Text(notify ? "Notifications on" : "Notifications off").accessibilityIdentifier("notifyState")
      }
      Section {
        TextField("Your name", text: $name).accessibilityIdentifier("name")
        Text(name.isEmpty ? "Who are you?" : "Hello, \(name)!").accessibilityIdentifier("greeting")
      }
    }
  }
}
