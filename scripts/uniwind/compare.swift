// Put a Simulator screenshot and a pomme screenshot side by side, with a
// third panel that marks where they differ (red: a channel differs by more
// than the tolerance), and print how many pixels differ.
//   xcrun swift scripts/uniwind/compare.swift ios.png pomme.png out.png [tolerance] [skip-top-px]
// skip-top-px: rows at the top not counted (the status bar: pomme draws the
// phone's notch or island in screenshots, the Simulator doesn't).
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

func load(_ path: String) -> CGImage {
  guard let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
    let img = CGImageSourceCreateImageAtIndex(src, 0, nil)
  else { fatalError("can't read \(path)") }
  return img
}

func pixels(_ img: CGImage, _ w: Int, _ h: Int) -> [UInt8] {
  var data = [UInt8](repeating: 0, count: w * h * 4)
  let ctx = CGContext(data: &data, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
    space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
  ctx.draw(img, in: CGRect(x: 0, y: 0, width: w, height: h))
  return data
}

let args = CommandLine.arguments
guard args.count >= 4 else {
  fputs("usage: compare.swift ios.png pomme.png out.png [tolerance]\n", stderr)
  exit(1)
}
let tolerance = args.count > 4 ? Int(args[4])! : 16
let skipTop = args.count > 5 ? Int(args[5])! : 0
let (a, b) = (load(args[1]), load(args[2]))
let (w, h) = (min(a.width, b.width), min(a.height, b.height))
let (pa, pb) = (pixels(a, w, h), pixels(b, w, h))
var diff = [UInt8](repeating: 255, count: w * h * 4)
var bad = 0
for i in stride(from: 0, to: w * h * 4, by: 4) {
  let off = i / 4 / w >= skipTop && (0..<3).contains { abs(Int(pa[i + $0]) - Int(pb[i + $0])) > tolerance }
  let gray = UInt8((Int(pa[i]) + Int(pa[i + 1]) + Int(pa[i + 2])) / 3 / 2 + 128)
  if off {
    bad += 1
    diff[i] = 230; diff[i + 1] = 30; diff[i + 2] = 30
  } else {
    diff[i] = gray; diff[i + 1] = gray; diff[i + 2] = gray
  }
}
let space = CGColorSpace(name: CGColorSpace.sRGB)!
let out = CGContext(data: nil, width: w * 3, height: h, bitsPerComponent: 8, bytesPerRow: 0, space: space,
  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
out.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
out.fill(CGRect(x: 0, y: 0, width: w * 3, height: h))
out.draw(a, in: CGRect(x: 0, y: h - a.height, width: a.width, height: a.height))
out.draw(b, in: CGRect(x: w, y: h - b.height, width: b.width, height: b.height))
let diffImage = CGContext(data: &diff, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: space,
  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!.makeImage()!
out.draw(diffImage, in: CGRect(x: 2 * w, y: 0, width: w, height: h))
let dest = CGImageDestinationCreateWithURL(URL(fileURLWithPath: args[3]) as CFURL, UTType.png.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(dest, out.makeImage()!, nil)
CGImageDestinationFinalize(dest)
print(String(format: "%@: %d of %d pixels differ by more than %d (%.2f%%)", args[3], bad, w * h, tolerance, 100 * Double(bad) / Double(w * h)))
