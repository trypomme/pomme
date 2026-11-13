import FirebaseAnalytics
import FirebaseAuth
import FirebaseCore
import FirebaseCrashlytics
import FirebaseFirestore
import SwiftUI

/// One anonymous sign-in. Firestore waits for it: its rules allow signed-in
/// users only, and writes made before a sign-in stay with the signed-out user.
@MainActor
enum SignIn {
    private static var task: Task<User, Error>?

    static func user() async throws -> User {
        if let task { return try await task.value }
        let t = Task { try await Auth.auth().signInAnonymously().user }
        task = t
        return try await t.value
    }
}

enum Checks {
    static func app() async throws -> String {
        let app = FirebaseApp.app()!
        // An app ID is 1:<project number>:<platform>:<hash>.
        let platform = app.options.googleAppID.split(separator: ":")[2]
        return "\(app.name) · \(app.options.projectID ?? "?") · \(platform)"
    }

    static func analytics() async throws -> String {
        Analytics.logEvent("pomme_opened", parameters: ["screen": "firebase"])
        return "logged · instance \(Analytics.appInstanceID() ?? "none")"
    }

    static func crashlytics() async throws -> String {
        let c = Crashlytics.crashlytics()
        c.log("opened the Firebase screen")
        c.setCustomValue("firebase", forKey: "screen")
        return "logged · collection \(c.isCrashlyticsCollectionEnabled())"
    }

    static func auth() async throws -> String {
        let user = try await SignIn.user()
        return "anonymous \(user.isAnonymous) · \(user.uid.count) char uid"
    }

    /// A write and a read of one document: the local cache first, then the server.
    static func firestore() async throws -> String {
        _ = try await SignIn.user()
        let ref = Firestore.firestore().collection("pomme").document("swiftui")
        let at = Int(Date().timeIntervalSince1970 * 1000)
        async let written: Void = ref.setData(["greeting": "hello", "at": at])
        let cached = try await ref.getDocument(source: .cache).data()
        try await written
        let server = try await ref.getDocument(source: .server).data()
        let same = (server?["at"] as? Int) == at
        return "cache \(cached?["greeting"] as? String ?? "nil") · server \(same ? "same" : "different")"
    }
}

struct FirebaseView: View {
    private let lines: [(String, () async throws -> String)] = [
        ("app", Checks.app),
        ("analytics", Checks.analytics),
        ("crashlytics", Checks.crashlytics),
        ("auth", Checks.auth),
        ("firestore", Checks.firestore),
    ]

    var body: some View {
        NavigationStack {
            List(lines, id: \.0) { label, run in
                Line(label: label, run: run)
            }
            .navigationTitle("Firebase")
        }
    }
}

/// "label …" until `run` finishes, then the label and its result (or the error).
struct Line: View {
    let label: String
    let run: () async throws -> String
    @State private var text: String?

    var body: some View {
        Text(text ?? "\(label) …")
            .accessibilityIdentifier(label)
            .task {
                do {
                    text = "\(label) \(try await run())"
                } catch {
                    text = "\(label) error \(error.localizedDescription)"
                }
            }
    }
}
