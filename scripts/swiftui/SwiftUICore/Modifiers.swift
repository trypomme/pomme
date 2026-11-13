// Applying modifiers to views.

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func modifier<T>(_ modifier: T) -> SwiftUICore.ModifiedContent<Self, T>
  nonisolated public func modifier<T>(_ modifier: T) -> ModifiedContent<Self, T> {
    ModifiedContent(content: self, modifier: modifier)
  }
}
