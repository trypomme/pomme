// A text field's selection (`TextField(_:text:selection:)`): a range of the
// text, an insertion point being an empty one. pomme's text fields report
// theirs through the binding and take the binding's.

// pomme: owns public struct TextSelection : Swift.Equatable, Swift.Hashable
@available(iOS 18.0, macOS 15.0, visionOS 2.0, *)
@available(tvOS, unavailable)
@available(watchOS, unavailable)
public struct TextSelection: Equatable, Hashable {
  // pomme: replaces public struct TextSelection : Swift.Equatable, Swift.Hashable :: public enum Indices : Swift.Equatable, Swift.Hashable
  public enum Indices: Equatable, Hashable {
    case selection(Range<String.Index>)
    @available(iOS 18.0, macOS 15.0, visionOS 2.0, *)
    case multiSelection(RangeSet<String.Index>)

    public static func == (a: Indices, b: Indices) -> Bool {
      switch (a, b) {
      case (.selection(let x), .selection(let y)): return x == y
      case (.multiSelection(let x), .multiSelection(let y)): return x == y
      default: return false
      }
    }

    public func hash(into hasher: inout Hasher) {
      switch self {
      case .selection(let r):
        hasher.combine(0)
        hasher.combine(r)
      case .multiSelection(let r):
        hasher.combine(1)
        for range in r.ranges { hasher.combine(range) }
      }
    }

    public var hashValue: Int {
      var h = Hasher()
      hash(into: &h)
      return h.finalize()
    }
  }

  // pomme: replaces public struct TextSelection : Swift.Equatable, Swift.Hashable :: public var indices: SwiftUI.TextSelection.Indices
  public var indices: Indices

  // pomme: replaces public struct TextSelection : Swift.Equatable, Swift.Hashable :: public var affinity: SwiftUI.TextSelectionAffinity
  public var affinity: TextSelectionAffinity { .automatic }

  // pomme: replaces public struct TextSelection : Swift.Equatable, Swift.Hashable :: public init(range: Swift.Range<Swift.String.Index>)
  public init(range: Range<String.Index>) {
    indices = .selection(range)
  }

  // pomme: replaces public struct TextSelection : Swift.Equatable, Swift.Hashable :: public init(ranges: Swift.RangeSet<Swift.String.Index>)
  @available(iOS 18.0, macOS 15.0, visionOS 2.0, *)
  public init(ranges: RangeSet<String.Index>) {
    indices = .multiSelection(ranges)
  }

  // pomme: replaces public struct TextSelection : Swift.Equatable, Swift.Hashable :: public init(insertionPoint: Swift.String.Index)
  public init(insertionPoint: String.Index) {
    indices = .selection(insertionPoint..<insertionPoint)
  }

  // pomme: replaces public struct TextSelection : Swift.Equatable, Swift.Hashable :: public var isInsertion: Swift.Bool
  public var isInsertion: Bool {
    switch indices {
    case .selection(let r): return r.isEmpty
    case .multiSelection(let r): return r.ranges.allSatisfy(\.isEmpty)
    }
  }

  // pomme: replaces public struct TextSelection : Swift.Equatable, Swift.Hashable :: public static func == (a: SwiftUI.TextSelection, b: SwiftUI.TextSelection) -> Swift.Bool
  public static func == (a: TextSelection, b: TextSelection) -> Bool { a.indices == b.indices }

  // pomme: replaces public struct TextSelection : Swift.Equatable, Swift.Hashable :: public func hash(into hasher: inout Swift.Hasher)
  public func hash(into hasher: inout Hasher) { hasher.combine(indices) }

  // pomme: replaces public struct TextSelection : Swift.Equatable, Swift.Hashable :: public var hashValue: Swift.Int
  public var hashValue: Int { indices.hashValue }
}
