// Sheet detents in Swift (`.custom(identifier:resolver:)`): written from the
// SDK's interface, on the Objective-C method pomme implements. A resolver's
// nil (inactive) goes to UIKit as NaN, which pomme's sheets take as inactive.

@available(iOS 16.0, *)
@available(tvOS, unavailable)
@available(watchOS, unavailable)
@available(visionOS, unavailable)
extension UISheetPresentationController.Detent {
  @available(iOS 16.0, *)
  @MainActor @preconcurrency public static func custom(identifier: UISheetPresentationController.Detent.Identifier? = nil, resolver: @escaping (_ context: any UISheetPresentationControllerDetentResolutionContext) -> CGFloat?) -> UISheetPresentationController.Detent {
    __customDetent(withIdentifier: identifier) { context in resolver(context) ?? .nan }
  }
}
