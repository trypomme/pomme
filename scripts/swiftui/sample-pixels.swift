// Sample a screenshot's pixels at points: what something drawn by the
// system (Liquid Glass, say) looks like, recorded as text for tests.
//   xcrun swift scripts/swiftui/sample-pixels.swift shot.png 3 < points
// Each input line is `name x y` in points; each output line
// `PIXEL name x y r g b` (0–255, the pixel at the point's top-left).
import CoreGraphics
import Foundation
import ImageIO

let args = CommandLine.arguments
guard args.count == 3, let scale = Double(args[2]),
  let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: args[1]) as CFURL, nil),
  let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
else {
  fputs("usage: sample-pixels.swift shot.png scale < points\n", stderr)
  exit(1)
}
let (w, h) = (image.width, image.height)
var data = [UInt8](repeating: 0, count: w * h * 4)
let context = CGContext(data: &data, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
context.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
while let line = readLine() {
  let parts = line.split(separator: " ")
  guard parts.count == 3, let x = Double(parts[1]), let y = Double(parts[2]) else { continue }
  let (px, py) = (Int(x * scale), Int(y * scale))
  guard px >= 0, py >= 0, px < w, py < h else { continue }
  let i = (py * w + px) * 4
  print("PIXEL \(parts[0]) \(parts[1]) \(parts[2]) \(data[i]) \(data[i + 1]) \(data[i + 2])")
}
