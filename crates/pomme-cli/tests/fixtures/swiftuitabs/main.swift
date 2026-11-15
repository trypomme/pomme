// SwiftUI tabs, iOS 18's way, for tests/swiftui_tabs.rs: a TabView with a
// selection, three `Tab`s by value, the first a NavigationStack with a title.
// Each tab says which is selected.

import SwiftUI

@main
struct TabsApp: App {
  var body: some Scene {
    WindowGroup {
      // (The fixtures build for iOS 17; Tab is iOS 18's.)
      if #available(iOS 18.0, *) {
        TabsView()
      }
    }
  }
}

enum Section: Hashable {
  case home, search, settings
}

@available(iOS 18.0, *)
struct TabsView: View {
  @State private var selection = Section.home

  var body: some View {
    TabView(selection: $selection) {
      Tab("Home", systemImage: "house.fill", value: Section.home) {
        NavigationStack {
          Text("Home screen, \(String(describing: selection)) selected")
            .navigationTitle("Home")
        }
      }
      Tab("Search", systemImage: "magnifyingglass", value: Section.search) {
        Text("Search screen, \(String(describing: selection)) selected")
      }
      Tab("Settings", systemImage: "gear", value: Section.settings) {
        Text("Settings screen")
      }
    }
  }
}
