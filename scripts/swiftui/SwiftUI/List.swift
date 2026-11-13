// List: rows in an inset grouped section that scrolls, as iOS shows lists by default.

import UIKit

// pomme: owns public struct List<SelectionValue, Content> : SwiftUICore.View where SelectionValue : Swift.Hashable, Content : SwiftUICore.View
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
@MainActor @preconcurrency
public struct List<SelectionValue, Content>: View where SelectionValue: Hashable, Content: View {
  var pommeContent: Content
}

extension List where SelectionValue == Never {
  // pomme: replaces extension SwiftUI.List where SelectionValue == Swift.Never :: public init(@SwiftUICore.ViewBuilder content: () -> Content)
  @MainActor @preconcurrency
  public init(@ViewBuilder content: () -> Content) {
    pommeContent = content()
  }
}

extension List: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let rows = graph.resolve(pommeContent, at: path.appending("content"), preferences)
    return [ListNode(path: path, rows: rows)]
  }
}

/// A list: its rows, one under another, in a rounded section on a gray page.
final class ListNode: Node {
  let rows: [Node]

  // iOS's inset grouped metrics.
  static let sectionMargin: CGFloat = 16
  static let sectionTop: CGFloat = 20
  static let cornerRadius: CGFloat = 26
  static let rowInset: CGFloat = 20
  static let rowPadding: CGFloat = 11
  static let minRowHeight: CGFloat = 52

  init(path: ViewPath, rows: [Node]) {
    self.rows = rows
    super.init(path: path)
  }

  override var fillsScreen: Bool { true }

  override func sizeThatFits(_ proposal: CGSize, _ renderer: Renderer) -> CGSize {
    proposal
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let scroll: UIScrollView = renderer.view(for: path.appending("scroll")) {
      let s = UIScrollView()
      s.backgroundColor = .systemGroupedBackground
      s.alwaysBounceVertical = true
      return s
    }
    scroll.frame = frame
    parent.pommeAdopt(scroll)

    let section = renderer.view(for: path.appending("section")) {
      let v = UIView()
      v.backgroundColor = .secondarySystemGroupedBackground
      v.layer.cornerRadius = Self.cornerRadius
      v.clipsToBounds = true
      return v
    }
    scroll.pommeAdopt(section)

    // Clear of the bars and the home indicator (the safe area), and the screen's sides.
    let safe = parent.safeAreaInsets
    let x = Self.sectionMargin + safe.left
    let width = frame.width - x - Self.sectionMargin - safe.right
    let contentWidth = width - 2 * Self.rowInset
    let scale = parent.window?.screen.scale ?? UIScreen.main.scale
    var y: CGFloat = 0
    for (i, row) in rows.enumerated() {
      if i > 0 {
        let separator = renderer.view(for: row.path.appending("separator")) {
          let v = UIView()
          v.backgroundColor = .separator
          return v
        }
        separator.frame = CGRect(x: Self.rowInset, y: y, width: width - Self.rowInset, height: 1 / scale)
        section.pommeAdopt(separator)
      }
      let size = row.sizeThatFits(CGSize(width: contentWidth, height: .greatestFiniteMagnitude), renderer)
      let height = max(Self.minRowHeight, size.height + 2 * Self.rowPadding)
      row.place(in: CGRect(x: Self.rowInset, y: y + (height - size.height) / 2, width: contentWidth, height: size.height), of: section, renderer)
      y += height
    }
    let top = safe.top + Self.sectionTop
    section.frame = CGRect(x: x, y: top, width: width, height: y)
    section.isHidden = rows.isEmpty
    scroll.contentSize = CGSize(width: frame.width, height: top + y + Self.sectionTop + safe.bottom)
  }
}
