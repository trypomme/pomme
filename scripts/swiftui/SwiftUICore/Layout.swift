// Layout: alignment guides, proposed sizes, and the Layout protocol, which
// SwiftUI's stacks and apps' own layouts use. A layout sees its subviews as
// LayoutSubview proxies; pomme's view graph (in the SwiftUI module) provides
// what's behind them.

import CoreGraphics
import Foundation

// ---- Alignment guides ----

/// The alignment IDs that guides are keyed by: a key is its ID's index here.
final class AlignmentIDs: @unchecked Sendable {
  private let lock = NSLock()
  private var ids: [any AlignmentID.Type] = []

  func key(for id: any AlignmentID.Type) -> AlignmentKey {
    lock.lock()
    defer { lock.unlock() }
    if let i = ids.firstIndex(where: { $0 == id }) {
      return AlignmentKey(index: UInt(i))
    }
    ids.append(id)
    return AlignmentKey(index: UInt(ids.count - 1))
  }

  func id(of key: AlignmentKey) -> any AlignmentID.Type {
    lock.lock()
    defer { lock.unlock() }
    return ids[key.pommeIndex]
  }
}

let alignmentIDs = AlignmentIDs()

// pomme: in public struct AlignmentKey : Swift.Hashable, Swift.Comparable
extension AlignmentKey {
  // pomme: replaces public struct AlignmentKey : Swift.Hashable, Swift.Comparable :: private let bits: Swift.UInt
  private let bits: UInt

  package init(index: UInt) {
    bits = index
  }

  package var pommeIndex: Int { Int(bits) }

  // pomme: replaces public struct AlignmentKey : Swift.Hashable, Swift.Comparable :: public static func < (lhs: SwiftUICore.AlignmentKey, rhs: SwiftUICore.AlignmentKey) -> Swift.Bool
  public static func < (lhs: AlignmentKey, rhs: AlignmentKey) -> Bool { lhs.bits < rhs.bits }
  // pomme: replaces public struct AlignmentKey : Swift.Hashable, Swift.Comparable :: public static func == (a: SwiftUICore.AlignmentKey, b: SwiftUICore.AlignmentKey) -> Swift.Bool
  public static func == (a: AlignmentKey, b: AlignmentKey) -> Bool { a.bits == b.bits }
  // pomme: replaces public struct AlignmentKey : Swift.Hashable, Swift.Comparable :: public func hash(into hasher: inout Swift.Hasher)
  public func hash(into hasher: inout Hasher) { hasher.combine(bits) }
  // pomme: replaces public struct AlignmentKey : Swift.Hashable, Swift.Comparable :: public var hashValue: Swift.Int
  public var hashValue: Int { bits.hashValue }
}

// pomme: in extension SwiftUICore.AlignmentID
extension AlignmentID {
  // pomme: replaces extension SwiftUICore.AlignmentID :: public static func _combineExplicit(childValue: CoreFoundation.CGFloat, _ n: Swift.Int, into parentValue: inout CoreFoundation.CGFloat?)
  /// Several children's explicit values for a guide: the first one's, by default.
  public static func _combineExplicit(childValue: CGFloat, _ n: Int, into parentValue: inout CGFloat?) {
    if parentValue == nil {
      parentValue = childValue
    }
  }
}

// pomme: in public struct HorizontalAlignment : Swift.Equatable
extension HorizontalAlignment {
  // pomme: replaces public struct HorizontalAlignment : Swift.Equatable :: public init(_ id: any SwiftUICore.AlignmentID.Type)
  public init(_ id: any AlignmentID.Type) {
    key = alignmentIDs.key(for: id)
  }

  /// The alignment a key stands for.
  package init(pommeKey: AlignmentKey) {
    key = pommeKey
  }
  // pomme: replaces public struct HorizontalAlignment : Swift.Equatable :: public let key: SwiftUICore.AlignmentKey
  @_documentation(visibility: private) public let key: AlignmentKey
  // pomme: replaces public struct HorizontalAlignment : Swift.Equatable :: public static func == (a: SwiftUICore.HorizontalAlignment, b: SwiftUICore.HorizontalAlignment) -> Swift.Bool
  public static func == (a: HorizontalAlignment, b: HorizontalAlignment) -> Bool { a.key == b.key }
}

// pomme: in public struct VerticalAlignment : Swift.Equatable
extension VerticalAlignment {
  // pomme: replaces public struct VerticalAlignment : Swift.Equatable :: public init(_ id: any SwiftUICore.AlignmentID.Type)
  public init(_ id: any AlignmentID.Type) {
    key = alignmentIDs.key(for: id)
  }

  /// The alignment a key stands for.
  package init(pommeKey: AlignmentKey) {
    key = pommeKey
  }
  // pomme: replaces public struct VerticalAlignment : Swift.Equatable :: public let key: SwiftUICore.AlignmentKey
  @_documentation(visibility: private) public let key: AlignmentKey
  // pomme: replaces public struct VerticalAlignment : Swift.Equatable :: public static func == (a: SwiftUICore.VerticalAlignment, b: SwiftUICore.VerticalAlignment) -> Swift.Bool
  public static func == (a: VerticalAlignment, b: VerticalAlignment) -> Bool { a.key == b.key }
}

extension HorizontalAlignment {
  /// The guide's position in a view of these dimensions (its explicit value, else its default).
  package func pommeValue(in d: ViewDimensions) -> CGFloat { d[self] }
}

extension VerticalAlignment {
  package func pommeValue(in d: ViewDimensions) -> CGFloat { d[self] }
}

// The standard guides.
enum PommeLeadingID: AlignmentID { static func defaultValue(in d: ViewDimensions) -> CGFloat { 0 } }
enum PommeHorizontalCenterID: AlignmentID { static func defaultValue(in d: ViewDimensions) -> CGFloat { d.width / 2 } }
enum PommeTrailingID: AlignmentID { static func defaultValue(in d: ViewDimensions) -> CGFloat { d.width } }
enum PommeTopID: AlignmentID { static func defaultValue(in d: ViewDimensions) -> CGFloat { 0 } }
enum PommeVerticalCenterID: AlignmentID { static func defaultValue(in d: ViewDimensions) -> CGFloat { d.height / 2 } }
enum PommeBottomID: AlignmentID { static func defaultValue(in d: ViewDimensions) -> CGFloat { d.height } }
/// A view without text has its baselines at its bottom (text sets them explicitly).
enum PommeFirstTextBaselineID: AlignmentID { static func defaultValue(in d: ViewDimensions) -> CGFloat { d.height } }
enum PommeLastTextBaselineID: AlignmentID { static func defaultValue(in d: ViewDimensions) -> CGFloat { d.height } }
enum PommeListRowSeparatorLeadingID: AlignmentID { static func defaultValue(in d: ViewDimensions) -> CGFloat { 0 } }
enum PommeListRowSeparatorTrailingID: AlignmentID { static func defaultValue(in d: ViewDimensions) -> CGFloat { d.width } }

// pomme: in extension SwiftUICore.HorizontalAlignment
extension HorizontalAlignment {
  // pomme: replaces extension SwiftUICore.HorizontalAlignment :: public static let leading: SwiftUICore.HorizontalAlignment
  public static let leading = HorizontalAlignment(PommeLeadingID.self)
  // pomme: replaces extension SwiftUICore.HorizontalAlignment :: public static let center: SwiftUICore.HorizontalAlignment
  public static let center = HorizontalAlignment(PommeHorizontalCenterID.self)
  // pomme: replaces extension SwiftUICore.HorizontalAlignment :: public static let trailing: SwiftUICore.HorizontalAlignment
  public static let trailing = HorizontalAlignment(PommeTrailingID.self)
  // pomme: replaces extension SwiftUICore.HorizontalAlignment :: public static let listRowSeparatorLeading: SwiftUICore.HorizontalAlignment
  @available(iOS 16.0, macOS 13.0, *)
  public static let listRowSeparatorLeading = HorizontalAlignment(PommeListRowSeparatorLeadingID.self)
  // pomme: replaces extension SwiftUICore.HorizontalAlignment :: public static let listRowSeparatorTrailing: SwiftUICore.HorizontalAlignment
  @available(iOS 16.0, macOS 13.0, *)
  public static let listRowSeparatorTrailing = HorizontalAlignment(PommeListRowSeparatorTrailingID.self)
}

// pomme: in extension SwiftUICore.VerticalAlignment
extension VerticalAlignment {
  // pomme: replaces extension SwiftUICore.VerticalAlignment :: public static let top: SwiftUICore.VerticalAlignment
  public static let top = VerticalAlignment(PommeTopID.self)
  // pomme: replaces extension SwiftUICore.VerticalAlignment :: public static let center: SwiftUICore.VerticalAlignment
  public static let center = VerticalAlignment(PommeVerticalCenterID.self)
  // pomme: replaces extension SwiftUICore.VerticalAlignment :: public static let bottom: SwiftUICore.VerticalAlignment
  public static let bottom = VerticalAlignment(PommeBottomID.self)
  // pomme: replaces extension SwiftUICore.VerticalAlignment :: public static let firstTextBaseline: SwiftUICore.VerticalAlignment
  public static let firstTextBaseline = VerticalAlignment(PommeFirstTextBaselineID.self)
  // pomme: replaces extension SwiftUICore.VerticalAlignment :: public static let lastTextBaseline: SwiftUICore.VerticalAlignment
  public static let lastTextBaseline = VerticalAlignment(PommeLastTextBaselineID.self)
}

// pomme: in public struct Alignment : Swift.Equatable
extension Alignment {
  // pomme: replaces public struct Alignment : Swift.Equatable :: public static func == (a: SwiftUICore.Alignment, b: SwiftUICore.Alignment) -> Swift.Bool
  public static func == (a: Alignment, b: Alignment) -> Bool { a.horizontal == b.horizontal && a.vertical == b.vertical }
}

// pomme: in extension SwiftUICore.Alignment
extension Alignment {
  // pomme: replaces extension SwiftUICore.Alignment :: public static let center: SwiftUICore.Alignment
  public static let center = Alignment(horizontal: .center, vertical: .center)
  // pomme: replaces extension SwiftUICore.Alignment :: public static let leading: SwiftUICore.Alignment
  public static let leading = Alignment(horizontal: .leading, vertical: .center)
  // pomme: replaces extension SwiftUICore.Alignment :: public static let trailing: SwiftUICore.Alignment
  public static let trailing = Alignment(horizontal: .trailing, vertical: .center)
  // pomme: replaces extension SwiftUICore.Alignment :: public static let top: SwiftUICore.Alignment
  public static let top = Alignment(horizontal: .center, vertical: .top)
  // pomme: replaces extension SwiftUICore.Alignment :: public static let bottom: SwiftUICore.Alignment
  public static let bottom = Alignment(horizontal: .center, vertical: .bottom)
  // pomme: replaces extension SwiftUICore.Alignment :: public static let topLeading: SwiftUICore.Alignment
  public static let topLeading = Alignment(horizontal: .leading, vertical: .top)
  // pomme: replaces extension SwiftUICore.Alignment :: public static let topTrailing: SwiftUICore.Alignment
  public static let topTrailing = Alignment(horizontal: .trailing, vertical: .top)
  // pomme: replaces extension SwiftUICore.Alignment :: public static let bottomLeading: SwiftUICore.Alignment
  public static let bottomLeading = Alignment(horizontal: .leading, vertical: .bottom)
  // pomme: replaces extension SwiftUICore.Alignment :: public static let bottomTrailing: SwiftUICore.Alignment
  public static let bottomTrailing = Alignment(horizontal: .trailing, vertical: .bottom)
}

// pomme: in public struct UnitPoint : Swift.Hashable
extension UnitPoint {
  // pomme: replaces public struct UnitPoint : Swift.Hashable :: public static let zero: SwiftUICore.UnitPoint
  public static let zero = UnitPoint(x: 0, y: 0)
  // pomme: replaces public struct UnitPoint : Swift.Hashable :: public static let center: SwiftUICore.UnitPoint
  public static let center = UnitPoint(x: 0.5, y: 0.5)
  // pomme: replaces public struct UnitPoint : Swift.Hashable :: public static let leading: SwiftUICore.UnitPoint
  public static let leading = UnitPoint(x: 0, y: 0.5)
  // pomme: replaces public struct UnitPoint : Swift.Hashable :: public static let trailing: SwiftUICore.UnitPoint
  public static let trailing = UnitPoint(x: 1, y: 0.5)
  // pomme: replaces public struct UnitPoint : Swift.Hashable :: public static let top: SwiftUICore.UnitPoint
  public static let top = UnitPoint(x: 0.5, y: 0)
  // pomme: replaces public struct UnitPoint : Swift.Hashable :: public static let bottom: SwiftUICore.UnitPoint
  public static let bottom = UnitPoint(x: 0.5, y: 1)
  // pomme: replaces public struct UnitPoint : Swift.Hashable :: public static let topLeading: SwiftUICore.UnitPoint
  public static let topLeading = UnitPoint(x: 0, y: 0)
  // pomme: replaces public struct UnitPoint : Swift.Hashable :: public static let topTrailing: SwiftUICore.UnitPoint
  public static let topTrailing = UnitPoint(x: 1, y: 0)
  // pomme: replaces public struct UnitPoint : Swift.Hashable :: public static let bottomLeading: SwiftUICore.UnitPoint
  public static let bottomLeading = UnitPoint(x: 0, y: 1)
  // pomme: replaces public struct UnitPoint : Swift.Hashable :: public static let bottomTrailing: SwiftUICore.UnitPoint
  public static let bottomTrailing = UnitPoint(x: 1, y: 1)
  // pomme: replaces public struct UnitPoint : Swift.Hashable :: public static func == (a: SwiftUICore.UnitPoint, b: SwiftUICore.UnitPoint) -> Swift.Bool
  public static func == (a: UnitPoint, b: UnitPoint) -> Bool { a.x == b.x && a.y == b.y }
}

// pomme: in public enum Edge : Swift.Int8, Swift.CaseIterable > public struct Set : Swift.OptionSet
extension Edge.Set {
  // pomme: replaces public enum Edge : Swift.Int8, Swift.CaseIterable > public struct Set : Swift.OptionSet :: public init(_ e: SwiftUICore.Edge)
  public init(_ e: Edge) {
    self.init(rawValue: 1 << e.rawValue)
  }
}

// ---- Dimensions and proposals ----

// pomme: in public struct ViewDimensions
extension ViewDimensions {
  /// The view's size, and its explicit guides (`alignmentGuide`, text baselines).
  package let pommeSize: CGSize
  package let pommeExplicit: (AlignmentKey) -> CGFloat?

  package init(size: CGSize, explicit: @escaping (AlignmentKey) -> CGFloat? = { _ in nil }) {
    _pommeStorage = ()
    pommeSize = size
    pommeExplicit = explicit
  }

  // pomme: replaces public struct ViewDimensions :: public var width: CoreFoundation.CGFloat
  public var width: CGFloat { pommeSize.width }
  // pomme: replaces public struct ViewDimensions :: public var height: CoreFoundation.CGFloat
  public var height: CGFloat { pommeSize.height }
  // pomme: replaces public struct ViewDimensions :: public subscript(guide: SwiftUICore.HorizontalAlignment) -> CoreFoundation.CGFloat
  public subscript(guide: HorizontalAlignment) -> CGFloat {
    pommeExplicit(guide.key) ?? alignmentIDs.id(of: guide.key).defaultValue(in: self)
  }
  // pomme: replaces public struct ViewDimensions :: public subscript(guide: SwiftUICore.VerticalAlignment) -> CoreFoundation.CGFloat
  public subscript(guide: VerticalAlignment) -> CGFloat {
    pommeExplicit(guide.key) ?? alignmentIDs.id(of: guide.key).defaultValue(in: self)
  }
  // pomme: replaces public struct ViewDimensions :: public subscript(explicit guide: SwiftUICore.HorizontalAlignment) -> CoreFoundation.CGFloat?
  public subscript(explicit guide: HorizontalAlignment) -> CGFloat? { pommeExplicit(guide.key) }
  // pomme: replaces public struct ViewDimensions :: public subscript(explicit guide: SwiftUICore.VerticalAlignment) -> CoreFoundation.CGFloat?
  public subscript(explicit guide: VerticalAlignment) -> CGFloat? { pommeExplicit(guide.key) }
}

// pomme: in public struct ProposedViewSize : Swift.Equatable
extension ProposedViewSize {
  // pomme: replaces public struct ProposedViewSize : Swift.Equatable :: public static let zero: SwiftUICore.ProposedViewSize
  public static let zero = ProposedViewSize(width: 0, height: 0)
  // pomme: replaces public struct ProposedViewSize : Swift.Equatable :: public static let unspecified: SwiftUICore.ProposedViewSize
  public static let unspecified = ProposedViewSize(width: nil, height: nil)
  // pomme: replaces public struct ProposedViewSize : Swift.Equatable :: public static let infinity: SwiftUICore.ProposedViewSize
  public static let infinity = ProposedViewSize(width: .infinity, height: .infinity)
  // pomme: replaces public struct ProposedViewSize : Swift.Equatable :: public static func == (a: SwiftUICore.ProposedViewSize, b: SwiftUICore.ProposedViewSize) -> Swift.Bool
  public static func == (a: ProposedViewSize, b: ProposedViewSize) -> Bool { a.width == b.width && a.height == b.height }
}

// ---- Spacing ----

// pomme: in public struct ViewSpacing : Swift.Sendable
extension ViewSpacing {
  /// The space a view wants on each side (top, leading, bottom, trailing).
  package var pommeEdges: [CGFloat]

  // pomme: replaces public struct ViewSpacing : Swift.Sendable :: public static let zero: SwiftUICore.ViewSpacing
  public static let zero = ViewSpacing(pommeEdges: [0, 0, 0, 0])

  package init(pommeEdges: [CGFloat]) {
    _pommeStorage = ()
    self.pommeEdges = pommeEdges
  }

  // pomme: replaces public struct ViewSpacing : Swift.Sendable :: public init()
  /// The system's spacing around a view: 8 points each side.
  public init() {
    self.init(pommeEdges: [8, 8, 8, 8])
  }

  // pomme: replaces public struct ViewSpacing : Swift.Sendable :: public mutating func formUnion(_ other: SwiftUICore.ViewSpacing, edges: SwiftUICore.Edge.Set = .all)
  public mutating func formUnion(_ other: ViewSpacing, edges: Edge.Set = .all) {
    for e in Edge.allCases where edges.contains(Edge.Set(e)) {
      pommeEdges[Int(e.rawValue)] = max(pommeEdges[Int(e.rawValue)], other.pommeEdges[Int(e.rawValue)])
    }
  }

  // pomme: replaces public struct ViewSpacing : Swift.Sendable :: public func union(_ other: SwiftUICore.ViewSpacing, edges: SwiftUICore.Edge.Set = .all) -> SwiftUICore.ViewSpacing
  public func union(_ other: ViewSpacing, edges: Edge.Set = .all) -> ViewSpacing {
    var s = self
    s.formUnion(other, edges: edges)
    return s
  }

  // pomme: replaces public struct ViewSpacing : Swift.Sendable :: public func distance(to next: SwiftUICore.ViewSpacing, along axis: SwiftUICore.Axis) -> CoreFoundation.CGFloat
  /// The space between this view and the next one along `axis`: the larger of what each wants.
  public func distance(to next: ViewSpacing, along axis: Axis) -> CGFloat {
    axis == .horizontal
      ? max(pommeEdges[Int(Edge.trailing.rawValue)], next.pommeEdges[Int(Edge.leading.rawValue)])
      : max(pommeEdges[Int(Edge.bottom.rawValue)], next.pommeEdges[Int(Edge.top.rawValue)])
  }
}

// ---- Subviews ----

/// What's behind a layout's subview: a view in pomme's graph.
package protocol LayoutProxy: AnyObject {
  func pommeSizeThatFits(_ proposal: ProposedViewSize) -> CGSize
  func pommeDimensions(in proposal: ProposedViewSize) -> ViewDimensions
  func pommePlace(at position: CGPoint, anchor: UnitPoint, proposal: ProposedViewSize)
  var pommePriority: Double { get }
  var pommeSpacing: ViewSpacing { get }
  /// A value set with `.layoutValue(key:value:)` (or a trait), by its key type.
  func pommeLayoutValue(_ key: ObjectIdentifier) -> Any?
}

// pomme: in public struct LayoutSubview : Swift.Equatable
extension LayoutSubview {
  package let pommeProxy: any LayoutProxy

  package init(_ proxy: any LayoutProxy) {
    _pommeStorage = ()
    pommeProxy = proxy
  }

  // pomme: replaces public struct LayoutSubview : Swift.Equatable :: public func _trait<K>(key: K.Type) -> K.Value where K : SwiftUICore._ViewTraitKey
  public func _trait<K>(key: K.Type) -> K.Value where K: _ViewTraitKey {
    pommeProxy.pommeLayoutValue(ObjectIdentifier(key)) as? K.Value ?? K.defaultValue
  }
  // pomme: replaces public struct LayoutSubview : Swift.Equatable :: public subscript<K>(key: K.Type) -> K.Value where K : SwiftUICore.LayoutValueKey
  public subscript<K>(key: K.Type) -> K.Value where K: LayoutValueKey {
    pommeProxy.pommeLayoutValue(ObjectIdentifier(key)) as? K.Value ?? K.defaultValue
  }
  // pomme: replaces public struct LayoutSubview : Swift.Equatable :: public var priority: Swift.Double
  public var priority: Double { pommeProxy.pommePriority }
  // pomme: replaces public struct LayoutSubview : Swift.Equatable :: public func sizeThatFits(_ proposal: SwiftUICore.ProposedViewSize) -> CoreFoundation.CGSize
  public func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { pommeProxy.pommeSizeThatFits(proposal) }
  // pomme: replaces public struct LayoutSubview : Swift.Equatable :: public func dimensions(in proposal: SwiftUICore.ProposedViewSize) -> SwiftUICore.ViewDimensions
  public func dimensions(in proposal: ProposedViewSize) -> ViewDimensions { pommeProxy.pommeDimensions(in: proposal) }
  // pomme: replaces public struct LayoutSubview : Swift.Equatable :: public var spacing: SwiftUICore.ViewSpacing
  public var spacing: ViewSpacing { pommeProxy.pommeSpacing }
  // pomme: replaces public struct LayoutSubview : Swift.Equatable :: public func place(at position: CoreFoundation.CGPoint, anchor: SwiftUICore.UnitPoint = .topLeading, proposal: SwiftUICore.ProposedViewSize)
  public func place(at position: CGPoint, anchor: UnitPoint = .topLeading, proposal: ProposedViewSize) {
    pommeProxy.pommePlace(at: position, anchor: anchor, proposal: proposal)
  }
  // pomme: replaces public struct LayoutSubview : Swift.Equatable :: public static func == (a: SwiftUICore.LayoutSubview, b: SwiftUICore.LayoutSubview) -> Swift.Bool
  public static func == (a: LayoutSubview, b: LayoutSubview) -> Bool { a.pommeProxy === b.pommeProxy }
}

// pomme: in public struct LayoutSubviews : Swift.Equatable, Swift.RandomAccessCollection, Swift.Sendable
extension LayoutSubviews {
  /// All the layout's subviews; this collection is the ones in `pommeRange`.
  package let pommeAll: [LayoutSubview]
  package let pommeRange: Range<Int>

  package init(_ all: [LayoutSubview], range: Range<Int>? = nil, layoutDirection: LayoutDirection = .leftToRight) {
    _pommeStorage = ()
    pommeAll = all
    pommeRange = range ?? all.indices
    self.layoutDirection = layoutDirection
  }

  // pomme: replaces public struct LayoutSubviews : Swift.Equatable, Swift.RandomAccessCollection, Swift.Sendable :: public var startIndex: Swift.Int
  public var startIndex: Int { pommeRange.lowerBound }
  // pomme: replaces public struct LayoutSubviews : Swift.Equatable, Swift.RandomAccessCollection, Swift.Sendable :: public var endIndex: Swift.Int
  public var endIndex: Int { pommeRange.upperBound }
  // pomme: replaces public struct LayoutSubviews : Swift.Equatable, Swift.RandomAccessCollection, Swift.Sendable :: public subscript(index: Swift.Int) -> SwiftUICore.LayoutSubviews.Element
  public subscript(index: Int) -> LayoutSubview { pommeAll[index] }
  // pomme: replaces public struct LayoutSubviews : Swift.Equatable, Swift.RandomAccessCollection, Swift.Sendable :: public subscript(bounds: Swift.Range<Swift.Int>) -> SwiftUICore.LayoutSubviews
  public subscript(bounds: Range<Int>) -> LayoutSubviews {
    LayoutSubviews(pommeAll, range: bounds, layoutDirection: layoutDirection)
  }
  // pomme: replaces public struct LayoutSubviews : Swift.Equatable, Swift.RandomAccessCollection, Swift.Sendable :: public subscript<S>(indices: S) -> SwiftUICore.LayoutSubviews where S : Swift.Sequence, S.Element == Swift.Int
  public subscript<S>(indices: S) -> LayoutSubviews where S: Sequence, S.Element == Int {
    LayoutSubviews(indices.map { pommeAll[$0] }, layoutDirection: layoutDirection)
  }
  // pomme: replaces public struct LayoutSubviews : Swift.Equatable, Swift.RandomAccessCollection, Swift.Sendable :: public static func == (lhs: SwiftUICore.LayoutSubviews, rhs: SwiftUICore.LayoutSubviews) -> Swift.Bool
  public static func == (lhs: LayoutSubviews, rhs: LayoutSubviews) -> Bool {
    lhs.pommeRange.count == rhs.pommeRange.count && zip(lhs, rhs).allSatisfy { $0 == $1 }
  }
}

// ---- The Layout protocol's defaults ----

// pomme: in public struct LayoutProperties : Swift.Sendable
extension LayoutProperties {
  // pomme: replaces public struct LayoutProperties : Swift.Sendable :: public init()
  public init() {
    _pommeStorage = ()
    stackOrientation = nil
  }
}

// pomme: in extension SwiftUICore.Layout
extension Layout {
  // pomme: replaces extension SwiftUICore.Layout :: public static var layoutProperties: SwiftUICore.LayoutProperties
  public static var layoutProperties: LayoutProperties { LayoutProperties() }

  // pomme: replaces extension SwiftUICore.Layout :: public func updateCache(_ cache: inout Self.Cache, subviews: Self.Subviews)
  /// The subviews changed: a new cache.
  public func updateCache(_ cache: inout Cache, subviews: Subviews) {
    cache = makeCache(subviews: subviews)
  }

  // pomme: replaces extension SwiftUICore.Layout :: public func explicitAlignment(of guide: SwiftUICore.HorizontalAlignment, in bounds: CoreFoundation.CGRect, proposal: SwiftUICore.ProposedViewSize, subviews: Self.Subviews, cache: inout Self.Cache) -> CoreFoundation.CGFloat?
  public func explicitAlignment(of guide: HorizontalAlignment, in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) -> CGFloat? {
    nil
  }

  // pomme: replaces extension SwiftUICore.Layout :: public func explicitAlignment(of guide: SwiftUICore.VerticalAlignment, in bounds: CoreFoundation.CGRect, proposal: SwiftUICore.ProposedViewSize, subviews: Self.Subviews, cache: inout Self.Cache) -> CoreFoundation.CGFloat?
  public func explicitAlignment(of guide: VerticalAlignment, in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) -> CGFloat? {
    nil
  }

  // pomme: replaces extension SwiftUICore.Layout :: public func spacing(subviews: Self.Subviews, cache: inout Self.Cache) -> SwiftUICore.ViewSpacing
  /// The spacing the subviews want, together.
  public func spacing(subviews: Subviews, cache: inout Cache) -> ViewSpacing {
    subviews.reduce(ViewSpacing.zero) { $0.union($1.spacing) }
  }
}

// pomme: in extension SwiftUICore.Layout where Self.Cache == ()
extension Layout where Cache == () {
  // pomme: replaces extension SwiftUICore.Layout where Self.Cache == () :: public func makeCache(subviews: Self.Subviews) -> Self.Cache
  public func makeCache(subviews: Subviews) -> Cache { () }
}

// ---- AnyLayout ----

// pomme: in internal class AnyLayoutBox : @unchecked Swift.Sendable
extension AnyLayoutBox {
  package let pommeLayout: any Layout

  package init(_ layout: any Layout) {
    pommeLayout = layout
  }
}

// pomme: in public struct AnyLayout : SwiftUICore.Layout, Swift.Sendable > public struct Cache : Swift.Sendable
extension AnyLayout.Cache {
  /// The erased layout's own cache.
  var pommeCache: Any?

  init(pommeCache: Any?) {
    _pommeStorage = ()
    self.pommeCache = pommeCache
  }
}

// pomme: in public struct AnyLayout : SwiftUICore.Layout, Swift.Sendable
extension AnyLayout {
  // pomme: replaces public struct AnyLayout : SwiftUICore.Layout, Swift.Sendable :: public init<L>(_ layout: L) where L : SwiftUICore.Layout
  public init<L>(_ layout: L) where L: Layout {
    if let any = layout as? AnyLayout {
      self = any
    } else {
      storage = AnyLayoutBox(layout)
    }
  }

  /// Run `body` with the erased layout and its cache (made the first time).
  private func with<R>(_ cache: inout Cache, subviews: Subviews, _ body: (any Layout, inout Any) -> R) -> R {
    var c: Any = cache.pommeCache ?? {
      func make<L: Layout>(_ l: L) -> Any { l.makeCache(subviews: subviews) }
      return make(storage.pommeLayout)
    }()
    defer { cache.pommeCache = c }
    return body(storage.pommeLayout, &c)
  }

  // pomme: replaces public struct AnyLayout : SwiftUICore.Layout, Swift.Sendable :: public func makeCache(subviews: SwiftUICore.AnyLayout.Subviews) -> SwiftUICore.AnyLayout.Cache
  public func makeCache(subviews: Subviews) -> Cache {
    func make<L: Layout>(_ l: L) -> Any { l.makeCache(subviews: subviews) }
    return Cache(pommeCache: make(storage.pommeLayout))
  }

  // pomme: replaces public struct AnyLayout : SwiftUICore.Layout, Swift.Sendable :: public func updateCache(_ cache: inout SwiftUICore.AnyLayout.Cache, subviews: SwiftUICore.AnyLayout.Subviews)
  public func updateCache(_ cache: inout Cache, subviews: Subviews) {
    cache = makeCache(subviews: subviews)
  }

  // pomme: replaces public struct AnyLayout : SwiftUICore.Layout, Swift.Sendable :: public func spacing(subviews: SwiftUICore.AnyLayout.Subviews, cache: inout SwiftUICore.AnyLayout.Cache) -> SwiftUICore.ViewSpacing
  public func spacing(subviews: Subviews, cache: inout Cache) -> ViewSpacing {
    with(&cache, subviews: subviews) { layout, c in
      func run<L: Layout>(_ l: L) -> ViewSpacing {
        var lc = c as! L.Cache
        defer { c = lc }
        return l.spacing(subviews: subviews, cache: &lc)
      }
      return run(layout)
    }
  }

  // pomme: replaces public struct AnyLayout : SwiftUICore.Layout, Swift.Sendable :: public func sizeThatFits(proposal: SwiftUICore.ProposedViewSize, subviews: SwiftUICore.AnyLayout.Subviews, cache: inout SwiftUICore.AnyLayout.Cache) -> CoreFoundation.CGSize
  public func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) -> CGSize {
    with(&cache, subviews: subviews) { layout, c in
      func run<L: Layout>(_ l: L) -> CGSize {
        var lc = c as! L.Cache
        defer { c = lc }
        return l.sizeThatFits(proposal: proposal, subviews: subviews, cache: &lc)
      }
      return run(layout)
    }
  }

  // pomme: replaces public struct AnyLayout : SwiftUICore.Layout, Swift.Sendable :: public func placeSubviews(in bounds: CoreFoundation.CGRect, proposal: SwiftUICore.ProposedViewSize, subviews: SwiftUICore.AnyLayout.Subviews, cache: inout SwiftUICore.AnyLayout.Cache)
  public func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) {
    with(&cache, subviews: subviews) { layout, c in
      func run<L: Layout>(_ l: L) {
        var lc = c as! L.Cache
        defer { c = lc }
        l.placeSubviews(in: bounds, proposal: proposal, subviews: subviews, cache: &lc)
      }
      run(layout)
    }
  }

  // pomme: replaces public struct AnyLayout : SwiftUICore.Layout, Swift.Sendable :: public func explicitAlignment(of guide: SwiftUICore.HorizontalAlignment, in bounds: CoreFoundation.CGRect, proposal: SwiftUICore.ProposedViewSize, subviews: SwiftUICore.AnyLayout.Subviews, cache: inout SwiftUICore.AnyLayout.Cache) -> CoreFoundation.CGFloat?
  public func explicitAlignment(of guide: HorizontalAlignment, in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) -> CGFloat? {
    with(&cache, subviews: subviews) { layout, c in
      func run<L: Layout>(_ l: L) -> CGFloat? {
        var lc = c as! L.Cache
        defer { c = lc }
        return l.explicitAlignment(of: guide, in: bounds, proposal: proposal, subviews: subviews, cache: &lc)
      }
      return run(layout)
    }
  }

  // pomme: replaces public struct AnyLayout : SwiftUICore.Layout, Swift.Sendable :: public func explicitAlignment(of guide: SwiftUICore.VerticalAlignment, in bounds: CoreFoundation.CGRect, proposal: SwiftUICore.ProposedViewSize, subviews: SwiftUICore.AnyLayout.Subviews, cache: inout SwiftUICore.AnyLayout.Cache) -> CoreFoundation.CGFloat?
  public func explicitAlignment(of guide: VerticalAlignment, in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) -> CGFloat? {
    with(&cache, subviews: subviews) { layout, c in
      func run<L: Layout>(_ l: L) -> CGFloat? {
        var lc = c as! L.Cache
        defer { c = lc }
        return l.explicitAlignment(of: guide, in: bounds, proposal: proposal, subviews: subviews, cache: &lc)
      }
      return run(layout)
    }
  }
}

// ---- Stacks ----

/// Lay views out one after another along an axis, the way SwiftUI's stacks do.
package struct StackLayoutEngine {
  let axis: Axis
  let spacing: CGFloat?
  /// The cross-axis guide the views line up on.
  let guide: (ViewDimensions) -> CGFloat

  private func major(_ s: CGSize) -> CGFloat { axis == .vertical ? s.height : s.width }
  private func minor(_ s: CGSize) -> CGFloat { axis == .vertical ? s.width : s.height }
  private func size(major: CGFloat, minor: CGFloat) -> CGSize {
    axis == .vertical ? CGSize(width: minor, height: major) : CGSize(width: major, height: minor)
  }
  private func proposal(major: CGFloat?, minor: CGFloat?) -> ProposedViewSize {
    axis == .vertical ? ProposedViewSize(width: minor, height: major) : ProposedViewSize(width: major, height: minor)
  }

  /// The space before each view (none before the first).
  private func gaps(_ subviews: LayoutSubviews) -> [CGFloat] {
    subviews.indices.map { i in
      guard i > subviews.startIndex else { return 0 }
      return spacing ?? subviews[i - 1].spacing.distance(to: subviews[i].spacing, along: axis)
    }
  }

  /// Each view's proposal along the axis: the space is shared out, the least
  /// flexible views (of the highest priority first) choosing their size first.
  private func allocate(_ proposal: ProposedViewSize, _ subviews: LayoutSubviews, gaps: [CGFloat]) -> [ProposedViewSize] {
    let minorProposal = axis == .vertical ? proposal.width : proposal.height
    guard let available = axis == .vertical ? proposal.height : proposal.width else {
      return subviews.map { _ in self.proposal(major: nil, minor: minorProposal) }
    }
    let n = subviews.count
    var remaining = available - gaps.reduce(0, +)
    var result = [ProposedViewSize](repeating: .unspecified, count: n)
    let flexibility: [CGFloat] = subviews.map { v in
      major(v.sizeThatFits(self.proposal(major: .infinity, minor: minorProposal)))
        - major(v.sizeThatFits(self.proposal(major: 0, minor: minorProposal)))
    }
    let byPriority = Dictionary(grouping: Array(0..<n)) { subviews[subviews.startIndex + $0].priority }
    for priority in byPriority.keys.sorted(by: >) {
      // Higher priorities first: lower ones get only what's left after their minimums.
      let lower = byPriority.filter { $0.key < priority }.flatMap { $0.value }
      let reserved = lower.reduce(CGFloat(0)) { sum, i in
        sum + major(subviews[subviews.startIndex + i].sizeThatFits(self.proposal(major: 0, minor: minorProposal)))
      }
      var group = byPriority[priority]!.sorted { flexibility[$0] < flexibility[$1] }
      var budget = max(remaining - reserved, 0)
      while !group.isEmpty {
        let i = group.removeFirst()
        let share = budget / CGFloat(group.count + 1)
        let p = self.proposal(major: share, minor: minorProposal)
        let used = major(subviews[subviews.startIndex + i].sizeThatFits(p))
        result[i] = p
        budget -= used
        remaining -= used
      }
    }
    return result
  }

  package func sizeThatFits(proposal: ProposedViewSize, subviews: LayoutSubviews) -> CGSize {
    guard !subviews.isEmpty else { return .zero }
    let gaps = gaps(subviews)
    let proposals = allocate(proposal, subviews, gaps: gaps)
    var length: CGFloat = gaps.reduce(0, +)
    var before: CGFloat = 0, after: CGFloat = 0
    for (v, p) in zip(subviews, proposals) {
      let d = v.dimensions(in: p)
      length += major(CGSize(width: d.width, height: d.height))
      let g = guide(d)
      before = max(before, g)
      after = max(after, minor(CGSize(width: d.width, height: d.height)) - g)
    }
    return size(major: length, minor: before + after)
  }

  package func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: LayoutSubviews) {
    guard !subviews.isEmpty else { return }
    let gaps = gaps(subviews)
    let proposals = allocate(proposal, subviews, gaps: gaps)
    let dims = zip(subviews, proposals).map { $0.dimensions(in: $1) }
    let before = dims.map(guide).max() ?? 0
    var position = axis == .vertical ? bounds.minY : bounds.minX
    for (i, v) in subviews.enumerated() {
      position += gaps[i]
      let d = dims[i]
      let cross = (axis == .vertical ? bounds.minX : bounds.minY) + before - guide(d)
      let origin = axis == .vertical ? CGPoint(x: cross, y: position) : CGPoint(x: position, y: cross)
      v.place(at: origin, anchor: .topLeading, proposal: proposals[i])
      position += major(CGSize(width: d.width, height: d.height))
    }
  }
}

/// Stack the views on each other, lined up on an alignment's guides.
package struct ZStackLayoutEngine {
  let alignment: Alignment

  package func sizeThatFits(proposal: ProposedViewSize, subviews: LayoutSubviews) -> CGSize {
    guard !subviews.isEmpty else { return .zero }
    var left: CGFloat = 0, right: CGFloat = 0, up: CGFloat = 0, down: CGFloat = 0
    for v in subviews {
      let d = v.dimensions(in: proposal)
      let h = d[alignment.horizontal], vv = d[alignment.vertical]
      left = max(left, h)
      right = max(right, d.width - h)
      up = max(up, vv)
      down = max(down, d.height - vv)
    }
    return CGSize(width: left + right, height: up + down)
  }

  /// Each view's guides on the stack's own guides, in the bounds it was given.
  package func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: LayoutSubviews) {
    let container = ViewDimensions(size: bounds.size)
    let x = bounds.minX + container[alignment.horizontal]
    let y = bounds.minY + container[alignment.vertical]
    for v in subviews {
      let d = v.dimensions(in: proposal)
      v.place(at: CGPoint(x: x - d[alignment.horizontal], y: y - d[alignment.vertical]), anchor: .topLeading, proposal: proposal)
    }
  }
}

// pomme: in extension SwiftUICore._ZStackLayout : SwiftUICore.Layout
extension _ZStackLayout {
  // pomme: replaces extension SwiftUICore._ZStackLayout : SwiftUICore.Layout :: public func sizeThatFits(proposal: SwiftUICore.ProposedViewSize, subviews: SwiftUICore._ZStackLayout.Subviews, cache: inout Swift.Void) -> CoreFoundation.CGSize
  public func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    ZStackLayoutEngine(alignment: alignment).sizeThatFits(proposal: proposal, subviews: subviews)
  }
  // pomme: replaces extension SwiftUICore._ZStackLayout : SwiftUICore.Layout :: public func placeSubviews(in bounds: CoreFoundation.CGRect, proposal: SwiftUICore.ProposedViewSize, subviews: SwiftUICore._ZStackLayout.Subviews, cache: inout Swift.Void)
  public func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    ZStackLayoutEngine(alignment: alignment).placeSubviews(in: bounds, proposal: proposal, subviews: subviews)
  }
  // pomme: replaces extension SwiftUICore._ZStackLayout : SwiftUICore.Layout :: public func spacing(subviews: SwiftUICore._ZStackLayout.Subviews, cache: inout Swift.Void) -> SwiftUICore.ViewSpacing
  public func spacing(subviews: Subviews, cache: inout ()) -> ViewSpacing {
    subviews.reduce(ViewSpacing.zero) { $0.union($1.spacing) }
  }
  // pomme: replaces extension SwiftUICore._ZStackLayout : SwiftUICore.Layout :: public static var layoutProperties: SwiftUICore.LayoutProperties
  public static var layoutProperties: LayoutProperties { LayoutProperties() }
}

/// The stack layouts' axis, spacing and guide.
package protocol PommeStackLayout {
  var pommeEngine: StackLayoutEngine { get }
}

extension _VStackLayout: PommeStackLayout {
  package var pommeEngine: StackLayoutEngine { StackLayoutEngine(axis: .vertical, spacing: spacing, guide: { [alignment] in $0[alignment] }) }
}

extension VStackLayout: PommeStackLayout {
  package var pommeEngine: StackLayoutEngine { StackLayoutEngine(axis: .vertical, spacing: spacing, guide: { [alignment] in $0[alignment] }) }
}

extension _HStackLayout: PommeStackLayout {
  package var pommeEngine: StackLayoutEngine { StackLayoutEngine(axis: .horizontal, spacing: spacing, guide: { [alignment] in $0[alignment] }) }
}

extension HStackLayout: PommeStackLayout {
  package var pommeEngine: StackLayoutEngine { StackLayoutEngine(axis: .horizontal, spacing: spacing, guide: { [alignment] in $0[alignment] }) }
}

// pomme: in public struct _StackLayoutCache
extension _StackLayoutCache {
  package init() {
    _pommeStorage = ()
  }
}

// ---- What layout views hold ----

/// A layout tree's root that holds a layout (`Layout.callAsFunction`'s).
package protocol PommeLayoutRoot {
  var pommeLayout: any Layout { get }
}

extension _LayoutRoot: PommeLayoutRoot {
  package var pommeLayout: any Layout { layout }
}

extension VStack {
  package var pommeTree: _VariadicView.Tree<_VStackLayout, Content> { _tree }
}

extension HStack {
  package var pommeTree: _VariadicView.Tree<_HStackLayout, Content> { _tree }
}

extension ZStack {
  package var pommeTree: _VariadicView.Tree<_ZStackLayout, Content> { _tree }
}

// ---- Layout modifiers ----

/// `fixedSize`: its content at its ideal size along the fixed axes, centered in
/// what it's given.
package struct FixedSizeLayout: Layout {
  let horizontal: Bool
  let vertical: Bool

  private func fixed(_ p: ProposedViewSize) -> ProposedViewSize {
    ProposedViewSize(width: horizontal ? nil : p.width, height: vertical ? nil : p.height)
  }

  package func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    subviews.first?.sizeThatFits(fixed(proposal)) ?? .zero
  }

  package func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    for v in subviews {
      v.place(at: CGPoint(x: bounds.midX, y: bounds.midY), anchor: .center, proposal: fixed(proposal))
    }
  }
}

extension _FixedSizeLayout {
  package var pommeFixedSize: FixedSizeLayout { FixedSizeLayout(horizontal: horizontal, vertical: vertical) }
}

// ---- Frames ----

/// Place a view of `size` in `bounds`, its guides on the bounds' guides for `alignment`.
private func aligned(_ size: CGSize, in bounds: CGRect, _ alignment: Alignment, _ d: ViewDimensions) -> CGPoint {
  let container = ViewDimensions(size: bounds.size)
  return CGPoint(
    x: bounds.minX + container[alignment.horizontal] - d[alignment.horizontal],
    y: bounds.minY + container[alignment.vertical] - d[alignment.vertical])
}

// pomme: in public struct _FrameLayout
extension _FrameLayout {
  // pomme: replaces public struct _FrameLayout :: package init(width: CoreFoundation.CGFloat?, height: CoreFoundation.CGFloat?, alignment: SwiftUICore.Alignment)
  @usableFromInline
  package init(width: CGFloat?, height: CGFloat?, alignment: Alignment) {
    self.width = width
    self.height = height
    self.alignment = alignment
  }
}

/// `.frame(width:height:alignment:)`: the given size (the content's along an
/// axis it doesn't fix), the content aligned in it.
package struct FrameLayout: Layout {
  let width: CGFloat?
  let height: CGFloat?
  let alignment: Alignment

  private func childProposal(_ p: ProposedViewSize) -> ProposedViewSize {
    ProposedViewSize(width: width ?? p.width, height: height ?? p.height)
  }

  package func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let child = subviews.first?.sizeThatFits(childProposal(proposal)) ?? .zero
    return CGSize(width: width ?? child.width, height: height ?? child.height)
  }

  package func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    let p = childProposal(ProposedViewSize(bounds.size))
    for v in subviews {
      let d = v.dimensions(in: p)
      v.place(at: aligned(bounds.size, in: bounds, alignment, d), anchor: .topLeading, proposal: p)
    }
  }
}

extension _FrameLayout {
  package var pommeFrame: FrameLayout { FrameLayout(width: width, height: height, alignment: alignment) }
}

// pomme: in public struct _FlexFrameLayout
extension _FlexFrameLayout {
  // pomme: replaces public struct _FlexFrameLayout :: package init(minWidth: CoreFoundation.CGFloat? = nil, idealWidth: CoreFoundation.CGFloat? = nil, maxWidth: CoreFoundation.CGFloat? = nil, minHeight: CoreFoundation.CGFloat? = nil, idealHeight: CoreFoundation.CGFloat? = nil, maxHeight: CoreFoundation.CGFloat? = nil, alignment: SwiftUICore.Alignment)
  @usableFromInline
  package init(minWidth: CGFloat? = nil, idealWidth: CGFloat? = nil, maxWidth: CGFloat? = nil, minHeight: CGFloat? = nil, idealHeight: CGFloat? = nil, maxHeight: CGFloat? = nil, alignment: Alignment) {
    self.minWidth = minWidth
    self.idealWidth = idealWidth
    self.maxWidth = maxWidth
    self.minHeight = minHeight
    self.idealHeight = idealHeight
    self.maxHeight = maxHeight
    self.alignment = alignment
  }
}

/// `.frame(minWidth:idealWidth:maxWidth:…)`: along each axis, what's offered
/// kept within the minimum and maximum (the content's own size standing for a
/// missing bound); with nothing offered, the ideal size, or the content's.
package struct FlexFrameLayout: Layout {
  struct Axis {
    let min: CGFloat?
    let ideal: CGFloat?
    let max: CGFloat?

    /// What the content is offered.
    func proposal(_ p: CGFloat?) -> CGFloat? {
      guard var v = p else { return ideal }
      if let max { v = Swift.min(v, max) }
      if let min { v = Swift.max(v, min) }
      return v
    }

    /// The frame's length, given the content's.
    func length(_ p: CGFloat?, child: CGFloat) -> CGFloat {
      guard min != nil || max != nil else { return child }
      if var v = p {
        v = max.map { Swift.min(v, $0) } ?? Swift.min(v, child)
        v = min.map { Swift.max(v, $0) } ?? Swift.max(v, child)
        return v
      }
      var v = ideal ?? child
      if let max { v = Swift.min(v, max) }
      if let min { v = Swift.max(v, min) }
      return v
    }
  }

  let horizontal: Axis
  let vertical: Axis
  let alignment: Alignment

  private func childProposal(_ p: ProposedViewSize) -> ProposedViewSize {
    ProposedViewSize(width: horizontal.proposal(p.width), height: vertical.proposal(p.height))
  }

  package func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let child = subviews.first?.sizeThatFits(childProposal(proposal)) ?? .zero
    return CGSize(width: horizontal.length(proposal.width, child: child.width), height: vertical.length(proposal.height, child: child.height))
  }

  package func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    let p = childProposal(ProposedViewSize(bounds.size))
    for v in subviews {
      let d = v.dimensions(in: p)
      v.place(at: aligned(bounds.size, in: bounds, alignment, d), anchor: .topLeading, proposal: p)
    }
  }
}

extension _FlexFrameLayout {
  package var pommeFrame: FlexFrameLayout {
    FlexFrameLayout(
      horizontal: .init(min: minWidth, ideal: idealWidth, max: maxWidth),
      vertical: .init(min: minHeight, ideal: idealHeight, max: maxHeight),
      alignment: alignment)
  }
}

// ---- Traits and layout values ----

// pomme: in public struct _LayoutTrait<K> : SwiftUICore._ViewTraitKey where K : SwiftUICore.LayoutValueKey
extension _LayoutTrait {
  // pomme: replaces public struct _LayoutTrait<K> : SwiftUICore._ViewTraitKey where K : SwiftUICore.LayoutValueKey :: public static var defaultValue: K.Value
  public static var defaultValue: K.Value { K.defaultValue }
}

/// A trait that carries a layout value: layouts find it by the value's key.
package protocol PommeLayoutTrait {
  static var pommeLayoutKey: ObjectIdentifier { get }
}

extension _LayoutTrait: PommeLayoutTrait {
  package static var pommeLayoutKey: ObjectIdentifier { ObjectIdentifier(K.self) }
}
