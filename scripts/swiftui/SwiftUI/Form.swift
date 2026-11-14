// Form and Section: on iOS, a form is an inset grouped list, its sections
// rounded groups of rows with a header above and a footer below.

import UIKit

// pomme: owns public struct Form<Content> : SwiftUICore.View where Content : SwiftUICore.View
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
@MainActor @preconcurrency
public struct Form<Content>: View where Content: View {
  var pommeContent: Content
}

extension Form {
  // pomme: replaces public struct Form<Content> : SwiftUICore.View where Content : SwiftUICore.View :: public init(@SwiftUICore.ViewBuilder content: () -> Content)
  @MainActor @preconcurrency
  public init(@ViewBuilder content: () -> Content) {
    pommeContent = content()
  }
}

extension Form: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let rows = graph.withEnvironment({ $0.pommeInList = true }) { graph.resolve(pommeContent, at: path.appending("content"), preferences) }
    let list = ListNode(path: path, rows: rows)
    list.underNavigationBar = graph.environment.pommeUnderNavigationBar
    return [list]
  }
}

// pomme: owns public struct Section<Parent, Content, Footer>
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
public struct Section<Parent, Content, Footer> {
  var pommeHeader: Parent
  var pommeContent: Content
  var pommeFooter: Footer
}

extension Section where Parent: View, Content: View, Footer: View {
  // pomme: replaces extension SwiftUI.Section where Parent : SwiftUICore.View, Content : SwiftUICore.View, Footer : SwiftUICore.View :: public init(@SwiftUICore.ViewBuilder content: () -> Content, @SwiftUICore.ViewBuilder header: () -> Parent, @SwiftUICore.ViewBuilder footer: () -> Footer)
  public init(@ViewBuilder content: () -> Content, @ViewBuilder header: () -> Parent, @ViewBuilder footer: () -> Footer) {
    pommeContent = content()
    pommeHeader = header()
    pommeFooter = footer()
  }

  // pomme: replaces extension SwiftUI.Section where Parent : SwiftUICore.View, Content : SwiftUICore.View, Footer : SwiftUICore.View :: public init(header: Parent, footer: Footer, @SwiftUICore.ViewBuilder content: () -> Content)
  public init(header: Parent, footer: Footer, @ViewBuilder content: () -> Content) {
    self.init(content: content, header: { header }, footer: { footer })
  }
}

extension Section where Parent == EmptyView, Content: View, Footer: View {
  // pomme: replaces extension SwiftUI.Section where Parent == SwiftUICore.EmptyView, Content : SwiftUICore.View, Footer : SwiftUICore.View :: public init(@SwiftUICore.ViewBuilder content: () -> Content, @SwiftUICore.ViewBuilder footer: () -> Footer)
  public init(@ViewBuilder content: () -> Content, @ViewBuilder footer: () -> Footer) {
    self.init(content: content, header: { EmptyView() }, footer: footer)
  }

  // pomme: replaces extension SwiftUI.Section where Parent == SwiftUICore.EmptyView, Content : SwiftUICore.View, Footer : SwiftUICore.View :: public init(footer: Footer, @SwiftUICore.ViewBuilder content: () -> Content)
  public init(footer: Footer, @ViewBuilder content: () -> Content) {
    self.init(content: content, header: { EmptyView() }, footer: { footer })
  }
}

extension Section where Parent: View, Content: View, Footer == EmptyView {
  // pomme: replaces extension SwiftUI.Section where Parent : SwiftUICore.View, Content : SwiftUICore.View, Footer == SwiftUICore.EmptyView :: public init(@SwiftUICore.ViewBuilder content: () -> Content, @SwiftUICore.ViewBuilder header: () -> Parent)
  public init(@ViewBuilder content: () -> Content, @ViewBuilder header: () -> Parent) {
    self.init(content: content, header: header, footer: { EmptyView() })
  }

  // pomme: replaces extension SwiftUI.Section where Parent : SwiftUICore.View, Content : SwiftUICore.View, Footer == SwiftUICore.EmptyView :: public init(header: Parent, @SwiftUICore.ViewBuilder content: () -> Content)
  public init(header: Parent, @ViewBuilder content: () -> Content) {
    self.init(content: content, header: { header }, footer: { EmptyView() })
  }
}

extension Section where Parent == EmptyView, Content: View, Footer == EmptyView {
  // pomme: replaces extension SwiftUI.Section where Parent == SwiftUICore.EmptyView, Content : SwiftUICore.View, Footer == SwiftUICore.EmptyView :: public init(@SwiftUICore.ViewBuilder content: () -> Content)
  public init(@ViewBuilder content: () -> Content) {
    self.init(content: content, header: { EmptyView() }, footer: { EmptyView() })
  }
}

extension Section where Parent == Text, Content: View, Footer == EmptyView {
  // pomme: replaces extension SwiftUI.Section where Parent == SwiftUICore.Text, Content : SwiftUICore.View, Footer == SwiftUICore.EmptyView :: public init(_ titleKey: SwiftUICore.LocalizedStringKey, @SwiftUICore.ViewBuilder content: () -> Content)
  public init(_ titleKey: LocalizedStringKey, @ViewBuilder content: () -> Content) {
    self.init(content: content, header: { Text(verbatim: titleKey.pommeLocalized()) }, footer: { EmptyView() })
  }

  // pomme: replaces extension SwiftUI.Section where Parent == SwiftUICore.Text, Content : SwiftUICore.View, Footer == SwiftUICore.EmptyView :: public init<S>(_ title: S, @SwiftUICore.ViewBuilder content: () -> Content) where S : Swift.StringProtocol
  @_disfavoredOverload
  public init<S>(_ title: S, @ViewBuilder content: () -> Content) where S: StringProtocol {
    self.init(content: content, header: { Text(title) }, footer: { EmptyView() })
  }
}

extension Section: PrimitiveView where Parent: View, Content: View, Footer: View {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    [SectionNode(
      path: path,
      header: graph.resolve(pommeHeader, at: path.appending("header"), preferences),
      rows: graph.resolve(pommeContent, at: path.appending("content"), preferences),
      footer: graph.resolve(pommeFooter, at: path.appending("footer"), preferences))]
  }
}

/// A list's section: its header, rows and footer (a list lays it out).
final class SectionNode: Node {
  let header: [Node]
  let rows: [Node]
  let footer: [Node]

  init(path: ViewPath, header: [Node], rows: [Node], footer: [Node]) {
    self.header = header
    self.rows = rows
    self.footer = footer
    super.init(path: path)
  }
}
