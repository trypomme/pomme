// UIBezierPath drawn into an image (UIGraphicsImageRenderer, at scale 1): a
// thick horizontal line with round caps (not square: its corner is clear), a filled oval, and a rounded
// rectangle's outline; the pixels where each should and shouldn't have ink
// (`PIXEL name alpha`), and the paths' bounds (`BOUNDS name x y w h`).

import Darwin
import UIKit

@main
struct BezierPath {
  static func main() {
    let format = UIGraphicsImageRendererFormat()
    format.scale = 1
    let image = UIGraphicsImageRenderer(size: CGSize(width: 100, height: 100), format: format).image { _ in
      UIColor.black.setStroke()
      let line = UIBezierPath()
      line.move(to: CGPoint(x: 20, y: 20))
      line.addLine(to: CGPoint(x: 80, y: 20))
      line.lineWidth = 10
      line.lineCapStyle = .round
      line.stroke()
      UIColor.black.setFill()
      UIBezierPath(ovalIn: CGRect(x: 30, y: 40, width: 40, height: 20)).fill()
      let box = UIBezierPath(roundedRect: CGRect(x: 10, y: 70, width: 80, height: 20), cornerRadius: 6)
      box.lineWidth = 2
      box.stroke()
      func show(_ r: CGRect) -> String { "\(r.minX) \(r.minY) \(r.width) \(r.height)" }
      fputs("BOUNDS oval \(show(UIBezierPath(ovalIn: CGRect(x: 30, y: 40, width: 40, height: 20)).bounds))\n", stderr)
      fputs("BOUNDS line \(show(line.bounds)) at \(line.currentPoint.x) \(line.currentPoint.y)\n", stderr)
    }
    guard let cg = image.cgImage, let data = cg.dataProvider?.data, let bytes = CFDataGetBytePtr(data) else { return }
    func alpha(_ x: Int, _ y: Int) -> UInt8 { bytes[y * cg.bytesPerRow + x * 4 + 3] }
    for (name, x, y) in [("cap", 17, 20), ("beyond-cap", 13, 20), ("cap-corner", 15, 15), ("line", 50, 20), ("above-line", 50, 13),
                         ("oval", 50, 50), ("oval-corner", 32, 42), ("box-edge", 50, 70), ("box-inside", 50, 80),
                         ("box-corner", 10, 70)] {
      fputs("PIXEL \(name) \(alpha(x, y))\n", stderr)
    }
  }
}
