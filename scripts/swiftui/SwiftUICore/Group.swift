// Group: views together, standing for them where it's placed (a list's rows,
// a stack's items) — pomme's SwiftUI resolves it to its content.

// pomme: in extension SwiftUICore.Group : SwiftUICore.View where Content : SwiftUICore.View
extension Group where Content: View {
  // pomme: replaces extension SwiftUICore.Group : SwiftUICore.View where Content : SwiftUICore.View :: nonisolated public init(@SwiftUICore.ViewBuilder content: () -> Content)
  @inlinable nonisolated public init(@ViewBuilder content: () -> Content) {
    self.init(_content: content())
  }
}
