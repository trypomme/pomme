// SwiftUI tabs, iOS 18's way, for tests/swiftui_tabs.rs: a TabView with a
// selection, three `Tab`s by value, the first a NavigationStack with a title,
// the second from a ForEach in a Group, the third with a badge, the fourth
// the search tab (`role: .search`). Each tab says which is selected.

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
  case home, search, settings, find
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
      Group {
        ForEach([Section.search], id: \.self) { section in
          Tab("Search", systemImage: "magnifyingglass", value: section) {
            Text("Search screen, \(String(describing: selection)) selected")
          }
        }
      }
      Tab("Settings", systemImage: "gear", value: Section.settings) {
        Text("Settings screen")
      }
      .badge(3)
      Tab("Find", systemImage: "magnifyingglass", value: Section.find, role: .search) {
        Text("Find screen, \(String(describing: selection)) selected")
      }
    }
  }
}
