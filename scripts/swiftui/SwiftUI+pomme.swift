// pomme's additions to the generated SwiftUI module: what Apple's binary has
// but its interface doesn't show (defaults that let primitive types conform),
// and placeholder types for opaque results pomme doesn't implement yet
// (never returned: the stub stops first).

import CoreTransferable
import Foundation
import SwiftUICore

// ---- Defaults the interface doesn't show: primitive content (Body == Never) ----

extension Scene where Body == Never {
  public var body: Never { _pommeUnimplemented("the body of a primitive scene") }
}

extension Commands where Body == Never {
  public var body: Never { _pommeUnimplemented("the body of primitive commands") }
}

extension ToolbarContent where Body == Never {
  public var body: Never { _pommeUnimplemented("the body of primitive toolbar content") }
}

extension WidgetConfiguration where Body == Never {
  public var body: Never { _pommeUnimplemented("the body of a primitive widget configuration") }
}

extension ControlWidgetConfiguration where Body == Never {
  public var body: Never { _pommeUnimplemented("the body of a primitive control configuration") }
}

extension ControlWidgetTemplate where Body == Never {
  public var body: Never { _pommeUnimplemented("the body of a primitive control template") }
}

extension TableRowContent where TableRowBody == Never {
  public var tableRowBody: Never { _pommeUnimplemented("the body of primitive table rows") }
}

extension TableColumnContent where TableColumnBody == Never {
  public var tableColumnBody: Never { _pommeUnimplemented("the body of primitive table columns") }
}

extension TabContent {
  public var body: Body { return _pommeUnimplementedValue("the body of primitive tab content") }
}

// ---- Placeholders for opaque results pomme doesn't implement yet ----

internal struct _PommeOpaqueTabContent<TabValue: Hashable>: TabContent {
  var body: Self { return self }  // `return`: not through the result builder
}

internal struct _PommeOpaque_TableRowContentModifier: _TableRowContentModifier {
  var body: Self { self }
  nonisolated static func _makeRows(modifier: _GraphValue<Self>, inputs: _TableRowInputs, body: @escaping (_Graph, _TableRowInputs) -> _TableRowOutputs) -> _TableRowOutputs {
    _pommeUnimplemented("a table row modifier")
  }
  nonisolated static func _tableRowCount(inputs: _TableRowInputs, body: @escaping (_TableRowInputs) -> Int?) -> Int? {
    _pommeUnimplemented("a table row modifier")
  }
}

internal struct _PommeOpaqueMatchedTransitionSourceConfiguration: MatchedTransitionSourceConfiguration {
  func _update(configuration: inout _ResolvedMatchedTransitionSourceConfiguration) {
    _pommeUnimplemented("a matched transition source")
  }
}

internal struct _PommeOpaqueTableColumnContent<TableRowValue: Identifiable, TableColumnSortComparator: SortComparator>: TableColumnContent {
  nonisolated var tableColumnBody: Never { _pommeUnimplemented("table column content") }
  nonisolated static func _makeContent(content: _GraphValue<Self>, inputs: _TableColumnInputs) -> _TableColumnOutputs {
    _pommeUnimplemented("table column content")
  }
  nonisolated static func _tableColumnCount(inputs: _TableColumnInputs) -> Int? { _pommeUnimplemented("table column content") }
}

internal struct _PommeOpaqueTableRowContent<TableRowValue: Identifiable>: TableRowContent {
  var tableRowBody: Never { _pommeUnimplemented("table row content") }
  nonisolated static func _makeRows(content: _GraphValue<Self>, inputs: _TableRowInputs) -> _TableRowOutputs { _pommeUnimplemented("table row content") }
  nonisolated static func _tableRowCount(inputs: _TableRowInputs) -> Int? { _pommeUnimplemented("table row content") }
  nonisolated static func _containsOutlineSymbol(inputs: _TableRowInputs) -> Bool { _pommeUnimplemented("table row content") }
}

internal struct _PommeOpaqueTransferRepresentation<Item: Transferable>: TransferRepresentation {
  var body: Self { self }
}

internal struct _PommeOpaqueDynamicViewContent: DynamicViewContent {
  var data: [Int] { _pommeUnimplemented("dynamic view content") }
  var body: Never { _pommeUnimplemented("dynamic view content") }
}

internal struct _PommeOpaqueWidget: Widget {
  init() {}
  var body: Never { _pommeUnimplemented("a widget") }
}

internal struct _PommeOpaquePresentationSizing: PresentationSizing {
  func proposedSize(for root: PresentationSizingRoot, context: PresentationSizingContext) -> ProposedViewSize {
    _pommeUnimplemented("a presentation sizing")
  }
}

internal struct _PommeOpaqueSliderTickContent<Value: BinaryFloatingPoint>: SliderTickContent {
  var body: [SliderTick<Value>] { _pommeUnimplemented("slider tick content") }
}
