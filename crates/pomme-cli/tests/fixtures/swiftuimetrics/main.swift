// SwiftUI's metrics: standard views in a list, each reporting its frame on
// screen (`METRIC name x y width height`, in points) once laid out. The same
// app runs on the iOS Simulator (scripts/swiftui/metrics-simulator.sh writes
// what it reports to ios26.txt) and in pomme (tests/swiftui_metrics.rs
// compares), so pomme's SwiftUI is held to iOS's layout.

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
struct MetricsApp: App {
  var body: some Scene {
    WindowGroup {
      MetricsView()
    }
  }
}

struct MetricsView: View {
  @State private var on = true
  @State private var value = 0.5

  var body: some View {
    List {
      Section {
        Text("One line").metric("text1")
        Text("Two lines: this text is long enough that it wraps onto a second line").metric("text2")
        Text("Three lines: this text is long enough that it wraps onto a second line and then onto a third one too").metric("text3")
        Toggle("Toggle", isOn: $on).metric("toggle")
        Slider(value: $value).metric("slider")
        Button("Button") {}.metric("button")
        HStack {
          Text("Leading").metric("hstack.leading")
          Spacer()
          Text("Trailing").metric("hstack.trailing")
        }
        .metric("hstack")
      } header: {
        Text("Header").metric("header")
      }
      Section {
        Text("Second section").metric("section2.text")
      } header: {
        Text("Second").metric("section2.header")
      }
      Section {
        Text("No header").metric("section3.text")
      }
    }
  }
}
