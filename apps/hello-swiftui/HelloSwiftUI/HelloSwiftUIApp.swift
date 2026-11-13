import FirebaseCore
import SwiftUI

// A SwiftUI app using the Firebase Apple SDK directly (Swift Package
// Manager), for pomme's tests: one line per module, as hello-expo's and
// hello-flutter's Firebase screens show them.

final class AppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        FirebaseApp.configure()
        return true
    }
}

@main
struct HelloSwiftUIApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) var delegate

    var body: some Scene {
        WindowGroup {
            FirebaseView()
        }
    }
}
