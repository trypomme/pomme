// ScrollView: its content, stacked along the axis it scrolls (as SwiftUI
// stacks a scroll view's views), in a UIScrollView. It takes the space it's
// offered; its content gets the width (or height) it has and its ideal size
// along the axis it scrolls. Like a list, its scroll view goes under the bars
// where it meets them and keeps its content clear of them.

import UIKit

// pomme: owns public struct ScrollView<Content> : SwiftUICore.View where Content : SwiftUICore.View
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
public struct ScrollView<Content>: View where Content: View {
  // pomme: replaces public struct ScrollView<Content> : SwiftUICore.View where Content : SwiftUICore.View :: public var content: Content
  public var content: Content
  // pomme: replaces public struct ScrollView<Content> : SwiftUICore.View where Content : SwiftUICore.View :: public var axes: SwiftUICore.Axis.Set
  public var axes: Axis.Set
  // pomme: replaces public struct ScrollView<Content> : SwiftUICore.View where Content : SwiftUICore.View :: public var showsIndicators: Swift.Bool
  public var showsIndicators: Bool

  // pomme: replaces public struct ScrollView<Content> : SwiftUICore.View where Content : SwiftUICore.View :: public var body: some SwiftUICore.View
  public var body: Never { fatalError("pomme resolves a ScrollView itself") }
  // pomme: replaces public struct ScrollView<Content> : SwiftUICore.View where Content : SwiftUICore.View :: public typealias Body = @_opaqueReturnTypeOf("$s7SwiftUI10ScrollViewV4bodyQrvp", 0) __<Content>
  public typealias Body = Never
}

extension ScrollView {
  // pomme: replaces public struct ScrollView<Content> : SwiftUICore.View where Content : SwiftUICore.View :: public init(_ axes: SwiftUICore.Axis.Set = .vertical, showsIndicators: Swift.Bool = true, @SwiftUICore.ViewBuilder content: () -> Content)
  public init(_ axes: Axis.Set = .vertical, showsIndicators: Bool = true, @ViewBuilder content: () -> Content) {
    self.axes = axes
    self.showsIndicators = showsIndicators
    self.content = content()
  }
}

extension ScrollView: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let children = graph.resolve(content, at: path.appending("content"), preferences)
    let horizontal = axes == .horizontal
    let layout: any Layout = horizontal ? _HStackLayout() : _VStackLayout()
    let stack = LayoutNode(path: path.appending("stack"), layout: layout, children: children)
    let node = ScrollNode(path: path, content: stack, axes: axes)
    node.showsIndicators = showsIndicators
    return [node]
  }
}

final class ScrollNode: Node {
  let content: Node
  let axes: Axis.Set
  var showsIndicators = true

  init(path: ViewPath, content: Node, axes: Axis.Set) {
    self.content = content
    self.axes = axes
    super.init(path: path)
  }

  /// What its content is offered: nothing along the axes it scrolls.
  private func contentProposal(_ size: CGSize) -> ProposedViewSize {
    ProposedViewSize(width: axes.contains(.horizontal) ? nil : size.width,
                     height: axes.contains(.vertical) ? nil : size.height)
  }

  /// All it's offered; with nothing offered, its content's size.
  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    if let w = proposal.width, let h = proposal.height { return CGSize(width: w, height: h) }
    let ideal = content.sizeThatFits(ProposedViewSize(width: axes.contains(.horizontal) ? nil : proposal.width,
                                                      height: axes.contains(.vertical) ? nil : proposal.height))
    return CGSize(width: proposal.width ?? ideal.width, height: proposal.height ?? ideal.height)
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let scroll: UIScrollView = renderer.view(for: path.appending("scroll")) {
      let s = UIScrollView()
      s.alwaysBounceVertical = axes.contains(.vertical)
      s.alwaysBounceHorizontal = axes.contains(.horizontal)
      return s
    }
    scroll.showsVerticalScrollIndicator = showsIndicators
    scroll.showsHorizontalScrollIndicator = showsIndicators
    applyAccessibility(to: scroll)
    let (extended, safe) = renderer.extendingUnderSafeArea(frame, in: parent)
    scroll.contentInsetAdjustmentBehavior = renderer.respectsSafeArea ? .automatic : .never
    scroll.frame = extended
    parent.pommeAdopt(scroll)

    // The content: as wide (or tall) as the scroll view's safe area, its
    // ideal size the way it scrolls, and centred the other way if smaller.
    let inner = frame.size
    let proposal = contentProposal(inner)
    let size = content.sizeThatFits(proposal)
    var origin = CGPoint(x: safe.left, y: 0)
    if !axes.contains(.horizontal) { origin.x += max(0, (inner.width - size.width) / 2) }
    if !axes.contains(.vertical) { origin.y += max(0, (inner.height - size.height) / 2) }
    if axes.contains(.horizontal) { origin.x = 0 }
    content.place(in: CGRect(origin: origin, size: size), of: scroll, renderer)
    scroll.contentSize = CGSize(width: axes.contains(.horizontal) ? size.width : extended.width,
                                height: axes.contains(.vertical) ? size.height : inner.height)
  }

  override var firstTextLeading: CGFloat? { content.firstTextLeading }
}
