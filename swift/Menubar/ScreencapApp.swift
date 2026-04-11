import AppKit
import SwiftUI

@main
struct ScreencapApp: App {
    var body: some Scene {
        MenuBarExtra("Screencap", systemImage: "camera") {
            Text("Screencap")
                .foregroundStyle(.secondary)

            Divider()

            Button("Quit") {
                NSApplication.shared.terminate(nil)
            }
            .keyboardShortcut("q")
        }
    }
}
