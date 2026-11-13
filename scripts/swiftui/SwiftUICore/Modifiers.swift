// Applying modifiers to views.

// pomme: in extension SwiftUICore.View
extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func modifier<T>(_ modifier: T) -> SwiftUICore.ModifiedContent<Self, T>
  nonisolated public func modifier<T>(_ modifier: T) -> ModifiedContent<Self, T> {
    ModifiedContent(content: self, modifier: modifier)
  }
}

// ---- AnyView ----

// pomme: in internal class AnyViewStorageBase
extension AnyViewStorageBase {
  /// The view, whatever its type.
  package let pommeView: any View

  package init(_ view: any View) {
    pommeView = view
  }
}

// pomme: in public struct AnyView : SwiftUICore.View
extension AnyView {
  // pomme: replaces public struct AnyView : SwiftUICore.View :: public init<V>(_ view: V) where V : SwiftUICore.View
  public init<V>(_ view: V) where V: View {
    if let any = view as? AnyView {
      self = any
    } else {
      storage = AnyViewStorageBase(view)
    }
  }

  // pomme: replaces public struct AnyView : SwiftUICore.View :: public init?(_fromValue value: Any)
  public init?(_fromValue value: Any) {
    guard let view = value as? any View else { return nil }
    self.init(view)
  }
}

extension AnyView {
  /// The view it erases.
  package var pommeView: any View { storage.pommeView }
}

// ---- if/else in a view builder ----

// pomme: in extension SwiftUICore._ConditionalContent : SwiftUICore.View where TrueContent : SwiftUICore.View, FalseContent : SwiftUICore.View
extension _ConditionalContent where TrueContent: View, FalseContent: View {
  // pomme: replaces extension SwiftUICore._ConditionalContent : SwiftUICore.View where TrueContent : SwiftUICore.View, FalseContent : SwiftUICore.View :: internal init(storage: SwiftUICore._ConditionalContent<TrueContent, FalseContent>.Storage)
  @usableFromInline
  internal init(storage: Storage) {
    self.storage = storage
  }
}
