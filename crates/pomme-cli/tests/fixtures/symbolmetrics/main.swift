// SF Symbols' metrics, as UIKit and SwiftUI lay them out: each symbol pomme
// maps (crates/pomme-hle/src/symbols/map.txt) at the default configuration
// (`SYMBOL name width height baseline`, the image's size and its baseline
// from the bottom, in points), and at the small and large scales (`SMALL`,
// `LARGE`); one symbol across point sizes, weights,
// scales and text styles (`CONFIG …`); and Image and Label in a list
// (`METRIC name x y width height`). Recorded on the simulator
// (scripts/swiftui/metrics-simulator.sh symbolmetrics → ios26.txt), which
// pomme's symbol sizes come from; tests/symbols.rs holds pomme to it.

import Darwin
import SwiftUI
import UIKit

let names = ["airplane", "antenna.radiowaves.left.and.right", "archivebox", "archivebox.fill", "arrow.clockwise", "arrow.counterclockwise", "arrow.down", "arrow.left", "arrow.right", "arrow.up", "arrow.up.arrow.down", "arrow.triangle.2.circlepath", "arrow.uturn.backward", "bag", "bag.fill", "barcode", "battery.100", "bell", "bell.fill", "bell.slash", "bicycle", "bolt", "bolt.fill", "book", "book.fill", "bookmark", "bookmark.fill", "bubble.left", "bubble.left.fill", "calendar", "camera", "camera.fill", "car", "car.fill", "cart", "cart.fill", "checkmark", "checkmark.circle", "checkmark.circle.fill", "checkmark.square", "checkmark.square.fill", "chevron.down", "chevron.left", "chevron.right", "chevron.up", "chevron.up.chevron.down", "circle", "circle.fill", "clock", "clock.fill", "cloud", "cloud.fill", "creditcard", "creditcard.fill", "desktopcomputer", "doc", "doc.fill", "doc.on.doc", "doc.text", "ellipsis", "ellipsis.circle", "envelope", "envelope.fill", "exclamationmark.circle", "exclamationmark.circle.fill", "exclamationmark.triangle", "exclamationmark.triangle.fill", "eye", "eye.fill", "eye.slash", "film", "flag", "flag.fill", "flame", "flame.fill", "folder", "folder.fill", "gamecontroller", "gearshape", "gearshape.fill", "gear", "gift", "globe", "graduationcap", "hammer", "hand.thumbsup", "hand.thumbsup.fill", "headphones", "heart", "heart.fill", "house", "house.fill", "info.circle", "info.circle.fill", "iphone", "keyboard", "leaf", "leaf.fill", "line.3.horizontal", "line.3.horizontal.decrease", "link", "list.bullet", "location", "location.fill", "lock", "lock.fill", "lock.open", "magnifyingglass", "map", "map.fill", "mappin", "mappin.circle", "message", "message.fill", "mic", "mic.fill", "minus", "minus.circle", "moon", "moon.fill", "music.note", "network", "newspaper", "paintbrush", "paperclip", "paperplane", "paperplane.fill", "pause", "pause.fill", "pencil", "person", "person.fill", "person.2", "person.2.fill", "person.circle", "person.circle.fill", "person.crop.circle", "person.crop.circle.fill", "phone", "phone.fill", "photo", "photo.fill", "play", "play.fill", "plus", "plus.circle", "plus.circle.fill", "plus.square", "power", "printer", "qrcode", "questionmark.circle", "questionmark.circle.fill", "rectangle.portrait.and.arrow.right", "scissors", "shield", "shield.fill", "slider.horizontal.3", "sparkles", "speaker.wave.2", "speaker.wave.2.fill", "square", "square.and.arrow.down", "square.and.arrow.up", "square.and.pencil", "square.grid.2x2", "star", "star.fill", "stop.fill", "sun.max", "sun.max.fill", "tag", "tag.fill", "trash", "trash.fill", "tray", "trophy", "trophy.fill", "tv", "video", "video.fill", "wand.and.stars", "wifi", "wrench", "xmark", "xmark.circle", "xmark.circle.fill", ]

func describe(_ i: UIImage?) -> String {
  guard let i else { return "none" }
  return "\(i.size.width) \(i.size.height) \(i.baselineOffsetFromBottom ?? -1)"
}

func report() {
  for n in names {
    fputs("SYMBOL \(n) \(describe(UIImage(systemName: n)))\n", stderr)
    for (tag, scale) in [("SMALL", UIImage.SymbolScale.small), ("LARGE", .large)] {
      fputs("\(tag) \(n) \(describe(UIImage(systemName: n, withConfiguration: UIImage.SymbolConfiguration(scale: scale))))\n", stderr)
    }
  }
  fputs("SYMBOL not.a.symbol \(describe(UIImage(systemName: "not.a.symbol")))\n", stderr)
  for size in [12.0, 17.0, 34.0] {
    fputs("CONFIG size\(Int(size)) \(describe(UIImage(systemName: "star.fill", withConfiguration: UIImage.SymbolConfiguration(pointSize: size))))\n", stderr)
  }
  for (w, weight) in [("ultraLight", UIImage.SymbolWeight.ultraLight), ("bold", .bold), ("black", .black)] {
    fputs("CONFIG \(w) \(describe(UIImage(systemName: "star.fill", withConfiguration: UIImage.SymbolConfiguration(weight: weight))))\n", stderr)
  }
  for (s, scale) in [("small", UIImage.SymbolScale.small), ("large", .large)] {
    fputs("CONFIG \(s) \(describe(UIImage(systemName: "star.fill", withConfiguration: UIImage.SymbolConfiguration(scale: scale))))\n", stderr)
  }
  for (t, style) in [("title", UIFont.TextStyle.title1), ("caption", .caption1)] {
    fputs("CONFIG \(t) \(describe(UIImage(systemName: "star.fill", withConfiguration: UIImage.SymbolConfiguration(textStyle: style))))\n", stderr)
  }
}

extension View {
  func metric(_ name: String) -> some View {
    onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { r in
      fputs("METRIC \(name) \(r.minX) \(r.minY) \(r.width) \(r.height)\n", stderr)
    }
  }
}

@main
struct SymbolMetricsApp: App {
  init() { report() }

  var body: some Scene {
    WindowGroup {
      List {
        Section {
          Image(systemName: "star.fill").metric("image")
          Image(systemName: "heart").font(.largeTitle).metric("image.largeTitle")
          Image(systemName: "heart").imageScale(.large).metric("image.large")
          HStack {
            Image(systemName: "gearshape").metric("hstack.icon")
            Text("Settings").metric("hstack.text")
          }
          Label {
            Text("Favorites").metric("label.title")
          } icon: {
            Image(systemName: "heart").metric("label.icon")
          }
          .metric("label")
          Label {
            Text("Wide").metric("label2.title")
          } icon: {
            Image(systemName: "rectangle.portrait.and.arrow.right").metric("label2.icon")
          }
          Label("Plain", systemImage: "star").metric("label3")
        }
        Section {
          VStack(alignment: .leading) {
            Label {
              Text("Outside").metric("vlabel.title")
            } icon: {
              Image(systemName: "heart").metric("vlabel.icon")
            }
          }
        }
      }
      // (Outside the list's rows: a Label as it is elsewhere.)
      .overlay(alignment: .bottomLeading) {
        VStack(alignment: .leading) {
          Label {
            Text("Loose").metric("loose.title")
          } icon: {
            Image(systemName: "heart").metric("loose.icon")
          }
          .metric("loose")
          Label {
            Text("Loose wide").metric("loose2.title")
          } icon: {
            Image(systemName: "rectangle.portrait.and.arrow.right").metric("loose2.icon")
          }
          Label {
            Text("Title font").metric("loose3.title")
          } icon: {
            Image(systemName: "star").metric("loose3.icon")
          }
          .font(.title)
        }
        .padding(40)
      }
    }
  }
}
