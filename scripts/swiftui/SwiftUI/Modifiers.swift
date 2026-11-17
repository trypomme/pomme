// Modifiers pomme applies itself: tasks, appearance actions, accessibility.

import UIKit

// ---- .task ----

// pomme: owns public struct _TaskModifier2 : SwiftUICore.ViewModifier
@available(iOS 26.4, macOS 26.4, tvOS 26.4, watchOS 26.4, visionOS 26.4, *)
public struct _TaskModifier2: ViewModifier {
  var pommeWork: TaskWork
}

extension _TaskModifier2 {
  // pomme: replaces public struct _TaskModifier2 : SwiftUICore.ViewModifier :: internal init(name: Swift.String, executorPreference taskExecutor: (any _Concurrency.TaskExecutor)?, priority: _Concurrency.TaskPriority, action: sending @escaping @isolated(any) () async -> Swift.Void)
  @usableFromInline
  nonisolated internal init(name: String, executorPreference taskExecutor: (any TaskExecutor)?, priority: TaskPriority, action: sending @escaping @isolated(any) () async -> Void) {
    pommeWork = TaskWork(priority: priority, action: action)
  }
}

extension _TaskModifier2: NodeModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    graph.addTask(pommeWork, at: path)
  }
}

extension _TaskModifier: NodeModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    graph.addTask(TaskWork(priority: priority, action: action), at: path)
  }
}

// ---- .onAppear, .onDisappear ----

extension _AppearanceActionModifier: NodeModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    graph.addAppearance(AppearanceActions(appear: appear, disappear: disappear), at: path)
  }
}

// ---- Accessibility ----

// pomme: owns public struct AccessibilityAttachmentModifier : SwiftUICore.ViewModifier
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
public struct AccessibilityAttachmentModifier: ViewModifier {
  var pommeIdentifier: String?
  var pommeLabel: (@MainActor () -> String)?
  var pommeHidden: Bool?
}

extension AccessibilityAttachmentModifier: NodeModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    for node in nodes {
      if let id = pommeIdentifier { node.accessibilityIdentifier = id }
      if let label = pommeLabel { node.accessibilityLabel = label() }
      if let hidden = pommeHidden { node.accessibilityHidden = hidden }
    }
  }
}

extension View {
  // pomme: replaces extension SwiftUICore.View :: nonisolated public func accessibilityIdentifier(_ identifier: Swift.String) -> SwiftUICore.ModifiedContent<Self, SwiftUI.AccessibilityAttachmentModifier>
  nonisolated public func accessibilityIdentifier(_ identifier: String) -> ModifiedContent<Self, AccessibilityAttachmentModifier> {
    modifier(AccessibilityAttachmentModifier(pommeIdentifier: identifier))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func accessibilityIdentifier(_ identifier: Swift.String, isEnabled: Swift.Bool) -> SwiftUICore.ModifiedContent<Self, SwiftUI.AccessibilityAttachmentModifier>
  nonisolated public func accessibilityIdentifier(_ identifier: String, isEnabled: Bool) -> ModifiedContent<Self, AccessibilityAttachmentModifier> {
    modifier(AccessibilityAttachmentModifier(pommeIdentifier: isEnabled ? identifier : nil))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func accessibilityLabel(_ labelKey: SwiftUICore.LocalizedStringKey) -> SwiftUICore.ModifiedContent<Self, SwiftUI.AccessibilityAttachmentModifier>
  nonisolated public func accessibilityLabel(_ labelKey: LocalizedStringKey) -> ModifiedContent<Self, AccessibilityAttachmentModifier> {
    modifier(AccessibilityAttachmentModifier(pommeLabel: { labelKey.pommeLocalized() }))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func accessibilityLabel(_ label: SwiftUICore.Text) -> SwiftUICore.ModifiedContent<Self, SwiftUI.AccessibilityAttachmentModifier>
  nonisolated public func accessibilityLabel(_ label: Text) -> ModifiedContent<Self, AccessibilityAttachmentModifier> {
    modifier(AccessibilityAttachmentModifier(pommeLabel: { label.pommeString }))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func accessibilityLabel<S>(_ label: S) -> SwiftUICore.ModifiedContent<Self, SwiftUI.AccessibilityAttachmentModifier> where S : Swift.StringProtocol
  @_disfavoredOverload
  nonisolated public func accessibilityLabel<S>(_ label: S) -> ModifiedContent<Self, AccessibilityAttachmentModifier> where S: StringProtocol {
    let s = String(label)
    return modifier(AccessibilityAttachmentModifier(pommeLabel: { s }))
  }

  // pomme: replaces extension SwiftUICore.View :: nonisolated public func accessibilityHidden(_ hidden: Swift.Bool) -> SwiftUICore.ModifiedContent<Self, SwiftUI.AccessibilityAttachmentModifier>
  nonisolated public func accessibilityHidden(_ hidden: Bool) -> ModifiedContent<Self, AccessibilityAttachmentModifier> {
    modifier(AccessibilityAttachmentModifier(pommeHidden: hidden))
  }
}

extension Group: PrimitiveView where Content: View {
  /// Its content, as if it were there itself.
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    graph.resolve(content, at: path.appending("content"), preferences)
  }
}
