// SwiftUI's bordered button styles on the iOS 26 Simulator: .bordered and
// .borderedProminent at the regular and small control sizes, a prominent one
// whose label fills the width (frame(maxWidth: .infinity).frame(height: 32),
// expo-dev-menu's Continue), and a disabled one. Each prints its frame and its
// label's (METRIC button-name x y w h / METRIC label-name …, in points) — the shape's
// colors are sampled from the screenshot.

import SwiftUI

@main
struct BorderedButtonsProbe: App {
  var body: some Scene {
    WindowGroup {
      VStack(alignment: .leading, spacing: 20) {
        probe("bordered") { Button { } label: { Text("Open").report("bordered") }.buttonStyle(.bordered) }
        probe("prominent") { Button { } label: { Text("Open").report("prominent") }.buttonStyle(.borderedProminent) }
        probe("bordered-small") { Button { } label: { Text("Open").report("bordered-small") }.buttonStyle(.bordered).controlSize(.small) }
        probe("prominent-small") { Button { } label: { Text("Open").report("prominent-small") }.buttonStyle(.borderedProminent).controlSize(.small) }
        probe("prominent-wide") {
          Button { } label: {
            Text("Continue").font(.system(size: 16, weight: .semibold)).frame(maxWidth: .infinity).frame(height: 32).report("prominent-wide")
          }.buttonStyle(.borderedProminent)
        }
        probe("bordered-disabled") { Button { } label: { Text("Open").report("bordered-disabled") }.buttonStyle(.bordered).disabled(true) }
        probe("prominent-disabled") { Button { } label: { Text("Open").report("prominent-disabled") }.buttonStyle(.borderedProminent).disabled(true) }
      }
      .padding()
    }
  }

  func probe<V: View>(_ name: String, @ViewBuilder _ v: () -> V) -> some View {
    v().onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { r in
      fputs("METRIC button-\(name) \(r.minX) \(r.minY) \(r.width) \(r.height)\n", stderr)
    }
  }
}

extension View {
  func report(_ name: String) -> some View {
    onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { r in
      fputs("METRIC label-\(name) \(r.minX) \(r.minY) \(r.width) \(r.height)\n", stderr)
    }
  }
}
