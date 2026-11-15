// SwiftUI tabs the way before iOS 18, for tests/swiftui_tabs.rs: a TabView of
// views with `.tabItem` labels and `.tag`s, its selection an Int.

import SwiftUI

@main
struct TabItemsApp: App {
  var body: some Scene {
    WindowGroup {
      TabItemsView()
    }
  }
}

struct TabItemsView: View {
  @State private var selection = 1

  var body: some View {
    TabView(selection: $selection) {
      Text("First screen, tab \(selection)")
        .tabItem { Label("First", systemImage: "1.circle") }
        .tag(1)
      Text("Second screen, tab \(selection)")
        .tabItem {
          Image(systemName: "2.circle")
          Text("Second")
        }
        .tag(2)
        .badge("New")
    }
  }
}
