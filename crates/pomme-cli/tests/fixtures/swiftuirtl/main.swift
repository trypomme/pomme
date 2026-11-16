// SwiftUI in a right-to-left layout, for tests/rtl.rs: an HStack of three
// boxes (red 50, green 80, blue 30 wide), a yellow bar with its text at the
// leading edge, a box padded from the leading edge, and the chevron.forward
// symbol. The layout direction is printed (`VALUE layoutDirection`); the
// stills (scripts/swiftui/metrics-simulator.sh, VARIANT=<language>, SHOT)
// show where everything went.

import SwiftUI

@main
struct RTLApp: App {
  var body: some Scene {
    WindowGroup {
      RTLView()
    }
  }
}

struct RTLView: View {
  @Environment(\.layoutDirection) var direction

  var body: some View {
    VStack(alignment: .leading, spacing: 20) {
      HStack(spacing: 10) {
        Color.red.frame(width: 50, height: 40)
        Color.green.frame(width: 80, height: 40)
        Color.blue.frame(width: 30, height: 40)
      }
      Text("Hello")
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.yellow)
      Color.purple.frame(width: 60, height: 30).padding(.leading, 40)
      Image(systemName: "chevron.forward").font(.system(size: 40))
    }
    .padding(20)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    .onAppear { print("VALUE layoutDirection \(direction == .rightToLeft ? "rightToLeft" : "leftToRight")") }
  }
}
