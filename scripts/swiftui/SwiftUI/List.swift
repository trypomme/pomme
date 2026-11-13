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

/// A list: its sections (its rows not in a section make their own), one under
/// another, each a rounded group of rows on a gray page, with its header above
/// and its footer below.
final class ListNode: Node {
  let sections: [SectionNode]

  // iOS 26's inset grouped metrics (measured on its simulator, iPhone 16e).
  static let sectionMargin: CGFloat = 16
  static let sectionTop: CGFloat = 20
  static let sectionSpacing: CGFloat = 20
  static let cornerRadius: CGFloat = 26
  /// The rows' content, headers and separators, from the group's edges.
  static let rowInset: CGFloat = 16
  static let rowPadding: CGFloat = 11
  static let minRowHeight: CGFloat = 52
  /// A header's baseline: below the safe area's top (the first one) or the
  /// previous group, and above its own group.
  static let firstHeaderBaseline: CGFloat = 26
  static let headerBaselineAfterGroup: CGFloat = 43.67
  static let headerBaselineAboveGroup: CGFloat = 14.33
  static let headerGap: CGFloat = 7
  static let separatorColor = UIColor(red: 60 / 255, green: 60 / 255, blue: 67 / 255, alpha: 0.12)

  init(path: ViewPath, rows: [Node]) {
    var sections: [SectionNode] = []
    var loose: [Node] = []
    func flush() {
      if !loose.isEmpty {
        sections.append(SectionNode(path: loose[0].path.appending("section"), header: [], rows: loose, footer: []))
        loose = []
      }
    }
    for row in rows {
      if let section = row as? SectionNode {
        flush()
        sections.append(section)
      } else {
        loose.append(row)
      }
    }
    flush()
    self.sections = sections
    super.init(path: path)
  }

  override var fillsScreen: Bool { true }

  /// All it's offered (with nothing offered, a list's ideal size is 10 × 10, as SwiftUI's).
  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    proposal.replacingUnspecifiedDimensions()
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let scroll: UIScrollView = renderer.view(for: path.appending("scroll")) {
      let s = UIScrollView()
      s.backgroundColor = .systemGroupedBackground
      s.alwaysBounceVertical = true
      return s
    }
    // The background goes under the bars and the home indicator where the list
    // meets them; its rows keep clear of them.
    let (extended, safe) = fillsScreen && parent === renderer.root
      ? (frame, parent.safeAreaInsets)
      : renderer.extendingUnderSafeArea(frame, in: parent)
    scroll.frame = extended
    parent.pommeAdopt(scroll)

    let x = Self.sectionMargin + safe.left
    let width = extended.width - x - Self.sectionMargin - safe.right
    let contentWidth = width - 2 * Self.rowInset
    var y = safe.top
    var previousBottom: CGFloat?

    for section in sections {
      // The header, in the system's style, lined up with the rows' content,
      // placed by its baseline.
      if let header = section.header.first {
        let text = header as? TextNode
        text?.style = .sectionHeader
        let baseline = previousBottom.map { $0 + Self.headerBaselineAfterGroup } ?? safe.top + Self.firstHeaderBaseline
        let ascender = text?.style.font.ascender ?? 0
        let size = header.sizeThatFits(ProposedViewSize(width: contentWidth, height: nil))
        header.place(in: CGRect(x: x + Self.rowInset, y: baseline - ascender, width: contentWidth, height: size.height), of: scroll, renderer)
        y = baseline + Self.headerBaselineAboveGroup
      } else {
        y = previousBottom.map { $0 + Self.sectionSpacing } ?? safe.top + Self.sectionTop
      }

      let group = renderer.view(for: section.path.appending("group")) {
        let v = UIView()
        v.backgroundColor = .secondarySystemGroupedBackground
        v.layer.cornerRadius = Self.cornerRadius
        v.clipsToBounds = true
        return v
      }
      scroll.pommeAdopt(group)
      var rowY: CGFloat = 0
      for (i, row) in section.rows.enumerated() {
        if i > 0 {
          let separator = renderer.view(for: row.path.appending("separator")) {
            let v = UIView()
            v.backgroundColor = Self.separatorColor
            return v
          }
          // (A full point, at the bottom of the row above: iOS 26's, measured.)
          separator.frame = CGRect(x: Self.rowInset, y: rowY - 1, width: width - 2 * Self.rowInset, height: 1)
          group.pommeAdopt(separator)
        }
        let size = row.sizeThatFits(ProposedViewSize(width: contentWidth, height: nil))
        let height = max(Self.minRowHeight, size.height + 2 * Self.rowPadding)
        row.place(in: CGRect(x: Self.rowInset, y: rowY + (height - size.height) / 2, width: contentWidth, height: size.height), of: group, renderer)
        rowY += height
      }
      group.frame = CGRect(x: x, y: y, width: width, height: rowY)
      group.isHidden = section.rows.isEmpty
      y += rowY
      previousBottom = y

      for footer in section.footer {
        (footer as? TextNode)?.style = .sectionFooter
        y += Self.headerGap
        let size = footer.sizeThatFits(ProposedViewSize(width: contentWidth, height: nil))
        footer.place(in: CGRect(x: x + Self.rowInset, y: y, width: contentWidth, height: size.height), of: scroll, renderer)
        y += size.height
      }
    }
    scroll.contentSize = CGSize(width: extended.width, height: y + Self.sectionTop + safe.bottom)
  }
}
