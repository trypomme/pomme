// SwiftUI's materials on the iOS 26 Simulator: each (ultra thin, thin,
// regular, thick, ultra thick, bar) as a rectangle over white (left) and
// over black (right), a row each, 80 pt tall from y = 100. Sample the
// screenshot at the middle of each half (x = 97.5 and 292.5 on the iPhone
// 16e, y = 140 + 80 × row) for the color it gives over a plain background.

import SwiftUI

@main
struct MaterialsProbe: App {
  var body: some Scene {
    WindowGroup {
      VStack(spacing: 0) {
        Color.clear.frame(height: 100)
        row(.ultraThinMaterial)
        row(.thinMaterial)
        row(.regularMaterial)
        row(.thickMaterial)
        row(.ultraThickMaterial)
        row(.bar)
        Spacer()
      }
      .ignoresSafeArea()
    }
  }

  func row(_ m: Material) -> some View {
    HStack(spacing: 0) {
      ZStack { Color.white; Rectangle().fill(m) }
      ZStack { Color.black; Rectangle().fill(m) }
    }
    .frame(height: 80)
  }
}

// Recorded with scripts/swiftui/metrics-simulator.sh materials (SHOT, light
// and dark appearance) and sample-pixels.swift: ios26-light.txt,
// ios26-dark.txt (thick and ultra thick come out the same).
