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

extension _TaskModifier2: PrimitiveModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    graph.addTask(pommeWork, at: path)
  }
}

extension _TaskModifier: PrimitiveModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    graph.addTask(TaskWork(priority: priority, action: action), at: path)
  }
}

// ---- .onAppear, .onDisappear ----

extension _AppearanceActionModifier: PrimitiveModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    graph.addAppearance(AppearanceActions(appear: appear, disappear: disappear), at: path)
  }
}

// ---- Accessibility ----

// pomme: owns public struct AccessibilityAttachmentModifier : SwiftUICore.ViewModifier
@available(iOS 13.0, macOS 10.15, tvOS 13.0, watchOS 6.0, *)
public struct AccessibilityAttachmentModifier: ViewModifier {
  var pommeIdentifier: String?
}

extension AccessibilityAttachmentModifier: PrimitiveModifier {
  func pommeApply(to nodes: [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) {
    if let id = pommeIdentifier {
      for node in nodes {
        node.accessibilityIdentifier = id
      }
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
}
