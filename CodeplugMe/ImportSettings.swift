import Security
import SwiftUI

/// The RepeaterBook token, kept in the login Keychain as a generic password.
/// The token is personal and bound to the user's account, so it never goes in
/// UserDefaults.
enum RepeaterBookKeychain {
    private static let service = "CodeplugMe.RepeaterBook"
    private static let account = "token"

    /// The stored token, or nil when none is saved.
    static func token() -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let data = item as? Data else { return nil }
        return String(decoding: data, as: UTF8.self)
    }

    /// Save `token`, or delete the stored one when `token` is empty.
    static func setToken(_ token: String) throws {
        let base: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(base as CFDictionary)
        guard !token.isEmpty else { return }
        var add = base
        add[kSecValueData as String] = Data(token.utf8)
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else {
            let reason = SecCopyErrorMessageString(status, nil) as String? ?? "status \(status)"
            throw RadioCoreError.native("Could not save the RepeaterBook token: \(reason)")
        }
    }
}

/// A warning when `token` isn't an export API token, or nil. The export API
/// takes only app tokens (`rbuapp_` or `app_`); a bare value is usually the
/// private account token, which RepeaterBook rejects there as unknown.
func repeaterBookTokenWarning(_ token: String) -> String? {
    let t = token.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !t.isEmpty, !t.hasPrefix("rbuapp_"), !t.hasPrefix("app_") else { return nil }
    return "This looks like a private account token, which RepeaterBook's export API "
        + "doesn't accept. It needs an app token starting with rbuapp_ (or app_)."
}

/// UserDefaults key for the contact email RepeaterBook requires in the
/// User-Agent of every request made with a token.
let repeaterBookContactKey = "repeaterBookContact"

/// The app's Settings window: RepeaterBook credentials for the repeater import.
struct ImportSettingsView: View {
    @AppStorage(repeaterBookContactKey) private var contact = ""
    @State private var token = ""
    @State private var saveError: String?

    var body: some View {
        Form {
            Section {
                SecureField("Token", text: $token)
                if let warning = repeaterBookTokenWarning(token) {
                    Text(warning)
                        .font(.caption)
                        .foregroundStyle(.orange)
                        .fixedSize(horizontal: false, vertical: true)
                }
                TextField("Contact email", text: $contact)
                Text("RepeaterBook needs a personal token. Create one at repeaterbook.com "
                     + "under API Apps, then paste it here. The token is stored in your Keychain.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                if let saveError {
                    Text(saveError)
                        .font(.caption)
                        .foregroundStyle(.red)
                }
            } header: {
                Text("RepeaterBook")
            }
        }
        .formStyle(.grouped)
        .frame(width: 460)
        .onAppear { token = RepeaterBookKeychain.token() ?? "" }
        // Saved on every change rather than on a button: a Settings window has no
        // Done, and closing it is how people expect to commit.
        .onChange(of: token) { _ in saveToken() }
    }

    /// Write the edited token to the Keychain, reporting a failure inline.
    private func saveToken() {
        do {
            try RepeaterBookKeychain.setToken(token.trimmingCharacters(in: .whitespacesAndNewlines))
            saveError = nil
        } catch {
            saveError = error.localizedDescription
        }
    }
}

/// The same RepeaterBook settings, shown from inside the import sheet so the
/// user never has to find the app's Settings window to set up a source.
struct SourceSettingsSheet: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .trailing, spacing: 0) {
            ImportSettingsView()
            Button("Done") { dismiss() }
                .keyboardShortcut(.defaultAction)
                .padding()
        }
    }
}
