// Tags: a value a view is known by (`.tag(_:)`): a picker's options, a tab
// view's tabs. `.tag` is inlined into apps: on iOS 26 it writes a
// `_TagTraitWritingModifier`, which pomme's SwiftUI reads from the views it
// modifies.

extension View {
  // pomme: replaces extension SwiftUICore.View :: internal func tag<V>(_ tag: V) -> some SwiftUICore.View where V : Swift.Hashable
  @usableFromInline
  @MainActor @preconcurrency internal func tag<V>(_ tag: V) -> some View where V: Hashable {
    modifier(_TagTraitWritingModifier(tag: tag, includeOptional: true))
  }
}
