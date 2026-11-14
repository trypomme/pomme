// Divider: a hairline across its container's axis: vertical in a horizontal
// stack or a list row, horizontal otherwise; in the opaque separator color
// (iOS 26, measured on the simulator: 1 px, (198, 198, 200) on white).

import UIKit

// pomme: in public struct Divider : SwiftUICore.View
extension Divider {
  // pomme: replaces public struct Divider : SwiftUICore.View :: public init()
  public init() {
    _pommeStorage = ()
  }
}

extension Divider: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    [DividerNode(path: path)]
  }
}

final class DividerNode: Node {
  /// The axis of the stack it's in (set by the stack's node; a list row's is horizontal).
  var axis: Axis?
  var thickness: CGFloat { 1 / UIScreen.main.scale }

  /// As long as offered across its container, a hairline thick.
  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    axis == .horizontal
      ? CGSize(width: thickness, height: proposal.height ?? 0)
      : CGSize(width: proposal.width ?? 0, height: thickness)
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let line = renderer.view(for: path) { UIView() }
    line.backgroundColor = .opaqueSeparator
    line.frame = frame
    parent.pommeAdopt(line)
  }
}
