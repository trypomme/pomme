// Focus: a FocusState follows which view has the keyboard's focus. Like a
// State it has a location for as long as the view is in the graph; `.focused`
// ties it to a text field, which takes the focus when it's set and sets it as
// the user moves the focus.

import UIKit

// pomme: owns public struct FocusState<Value> : SwiftUICore.DynamicProperty where Value : Swift.Hashable
@frozen @propertyWrapper
public struct FocusState<Value>: DynamicProperty where Value: Hashable {
  // pomme: replaces public struct FocusState<Value> : SwiftUICore.DynamicProperty where Value : Swift.Hashable :: public struct Binding
  @frozen @propertyWrapper
  public struct Binding {
    private var _binding: SwiftUICore.Binding<Value>

    init(binding: SwiftUICore.Binding<Value>) {
      _binding = binding
    }

    public var wrappedValue: Value {
      get { _binding.wrappedValue }
      nonmutating set { _binding.wrappedValue = newValue }
    }

    public var projectedValue: FocusState<Value>.Binding { self }

    /// The binding to the value.
    var pommeBinding: SwiftUICore.Binding<Value> { _binding }
  }

  // pomme: replaces public struct FocusState<Value> : SwiftUICore.DynamicProperty where Value : Swift.Hashable :: internal var value: Value
  internal var value: Value
  // pomme: replaces public struct FocusState<Value> : SwiftUICore.DynamicProperty where Value : Swift.Hashable :: internal var location: SwiftUICore.AnyLocation<Value>?
  internal var location: AnyLocation<Value>?
  // pomme: replaces public struct FocusState<Value> : SwiftUICore.DynamicProperty where Value : Swift.Hashable :: internal var resetValue: Value
  /// What it is when nothing it names has the focus (false, nil).
  internal var resetValue: Value

  // pomme: replaces public struct FocusState<Value> : SwiftUICore.DynamicProperty where Value : Swift.Hashable :: public var wrappedValue: Value
  public var wrappedValue: Value {
    get { location?.value ?? value }
    nonmutating set { location?.value = newValue }
  }

  // pomme: replaces public struct FocusState<Value> : SwiftUICore.DynamicProperty where Value : Swift.Hashable :: public var projectedValue: SwiftUI.FocusState<Value>.Binding
  public var projectedValue: FocusState<Value>.Binding {
    Binding(binding: SwiftUICore.Binding(location: location ?? ConstantLocation(value)))
  }

  // pomme: replaces public struct FocusState<Value> : SwiftUICore.DynamicProperty where Value : Swift.Hashable :: public static func _makeProperty<V>(in buffer: inout SwiftUICore._DynamicPropertyBuffer, container: SwiftUICore._GraphValue<V>, fieldOffset: Swift.Int, inputs: inout SwiftUICore._GraphInputs)
  public static func _makeProperty<V>(in buffer: inout _DynamicPropertyBuffer, container: _GraphValue<V>, fieldOffset: Int, inputs: inout _GraphInputs) {}

  // pomme: replaces public struct FocusState<Value> : SwiftUICore.DynamicProperty where Value : Swift.Hashable :: public init() where Value == Swift.Bool
  public init() where Value == Bool {
    value = false
    location = nil
    resetValue = false
  }

  // pomme: replaces public struct FocusState<Value> : SwiftUICore.DynamicProperty where Value : Swift.Hashable :: public init<T>() where Value == T?, T : Swift.Hashable
  public init<T>() where Value == T?, T: Hashable {
    value = nil
    location = nil
    resetValue = nil
  }
}

extension FocusState: Sendable {}
extension FocusState.Binding: Sendable {}

extension FocusState: PommeDynamicProperty {
  package mutating func pommeAttach(to storage: ViewStorage, slot: Int, environment: EnvironmentValues) {
    if let stored = storage.locations[slot] as? StoredLocation<Value> {
      location = stored
    } else {
      let stored = StoredLocation(value, storage: storage)
      storage.locations[slot] = stored
      location = stored
    }
  }
}

/// `.focused(…)`: the views it modifies take the focus as the binding says,
/// and set it as theirs comes and goes.
struct FocusModifier: ViewModifier, NodeModifier {
  typealias Body = Never
  let get: () -> Bool
  let set: (Bool) -> Void

  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    for node in nodes {
      node.focus = (get, set)
    }
  }
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func focused(_ condition: SwiftUI.FocusState<Swift.Bool>.Binding) -> some SwiftUICore.View
  nonisolated public func focused(_ condition: FocusState<Bool>.Binding) -> some View {
    let binding = condition.pommeBinding
    return modifier(FocusModifier(get: { binding.wrappedValue }, set: { binding.wrappedValue = $0 }))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func focused<Value>(_ binding: SwiftUI.FocusState<Value>.Binding, equals value: Value) -> some SwiftUICore.View where Value : Swift.Hashable
  /// Focused while the binding is `value`; taking the focus sets it to
  /// `value`. (Losing it leaves the binding as it is: the binding can't name
  /// the state's reset value.)
  nonisolated public func focused<Value>(_ binding: FocusState<Value>.Binding, equals value: Value) -> some View where Value: Hashable {
    let b = binding.pommeBinding
    return modifier(FocusModifier(get: { b.wrappedValue == value }, set: { if $0 { b.wrappedValue = value } }))
  }
}
