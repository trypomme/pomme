// SwiftUI's metrics under a navigation bar with a large title: a titled
// navigation stack's list, its sections without headers, each view reporting
// its frame on screen (`METRIC name x y width height`, in points; for
// list.safeArea, its safe-area insets: leading top trailing bottom). Recorded on
// the iOS Simulator (scripts/swiftui/metrics-simulator.sh swiftuinavmetrics →
// ios26.txt); tests/swiftui_metrics.rs holds pomme to it.

import Darwin
import SwiftUI

extension View {
  func metric(_ name: String) -> some View {
    onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { r in
      // (stderr: unbuffered, so each line is out at once, terminal or not.)
      fputs("METRIC \(name) \(r.minX) \(r.minY) \(r.width) \(r.height)\n", stderr)
    }
  }
}

@main
struct NavMetricsApp: App {
  var body: some Scene {
    WindowGroup {
      NavigationStack {
        List {
          Section {
            Text("First").metric("text.first")
            Text("Row").metric("text.row")
          }
          Section {
            Text("Second").metric("text.second")
          }
        }
        .metric("list")
        // The safe area the list gets: its top is the navigation bar's bottom.
        .onGeometryChange(for: EdgeInsets.self) { $0.safeAreaInsets } action: { e in
          fputs("METRIC list.safeArea \(e.leading) \(e.top) \(e.trailing) \(e.bottom)\n", stderr)
        }
        .navigationTitle("Title")
      }
    }
  }
}
