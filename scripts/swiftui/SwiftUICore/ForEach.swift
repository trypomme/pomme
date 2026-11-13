// ForEach: views made from a collection's elements, each identified by its ID.

// pomme: in public struct ForEach<Data, ID, Content> where Data : Swift.RandomAccessCollection, ID : Swift.Hashable
extension ForEach {
  /// An element's identity.
  package var pommeID: (Data.Element) -> ID
}

// pomme: in extension SwiftUICore.ForEach where Content : SwiftUICore.View
extension ForEach where Content: View {
  // pomme: replaces extension SwiftUICore.ForEach where Content : SwiftUICore.View :: public init(_ data: Data, id: Swift.KeyPath<Data.Element, ID>, @SwiftUICore.ViewBuilder content: @escaping (Data.Element) -> Content)
  public init(_ data: Data, id: KeyPath<Data.Element, ID>, @ViewBuilder content: @escaping (Data.Element) -> Content) {
    _pommeStorage = ()
    self.data = data
    self.content = content
    pommeID = { $0[keyPath: id] }
  }

  /// Each element's ID and view, in order.
  package var pommeElements: [(id: ID, view: Content)] {
    data.map { (pommeID($0), content($0)) }
  }
}
