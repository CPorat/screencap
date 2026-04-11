import AppKit
import Combine
import Foundation
import SwiftUI

private struct StatsResponse: Decodable {
    let status: String
    let uptimeSecs: Int
    let storageBytes: Int
    let capturesToday: Int

    enum CodingKeys: String, CodingKey {
        case status
        case uptimeSecs = "uptime_secs"
        case storageBytes = "storage_bytes"
        case capturesToday = "captures_today"
    }
}

final class DaemonStatus: ObservableObject {
    @Published private(set) var capturesToday: Int?
    @Published private(set) var lastError: String?

    private let statsURL = URL(string: "http://127.0.0.1:7878/api/stats")!
    private var timerCancellable: AnyCancellable?

    init(refreshInterval: TimeInterval = 5) {
        fetchStats()
        timerCancellable = Timer.publish(every: refreshInterval, on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                self?.fetchStats()
            }
    }

    deinit {
        timerCancellable?.cancel()
    }

    var statusText: String {
        if let capturesToday {
            return "Captures Today: \(capturesToday)"
        }
        return "Daemon Not Running"
    }

    private func fetchStats() {
        URLSession.shared.dataTask(with: statsURL) { [weak self] data, response, error in
            guard let self else { return }

            if let error {
                DispatchQueue.main.async {
                    self.capturesToday = nil
                    self.lastError = error.localizedDescription
                }
                return
            }

            guard
                let httpResponse = response as? HTTPURLResponse,
                (200...299).contains(httpResponse.statusCode),
                let data
            else {
                DispatchQueue.main.async {
                    self.capturesToday = nil
                    self.lastError = "Daemon returned an unexpected response"
                }
                return
            }

            do {
                let stats = try JSONDecoder().decode(StatsResponse.self, from: data)
                DispatchQueue.main.async {
                    self.capturesToday = stats.capturesToday
                    self.lastError = nil
                }
            } catch {
                DispatchQueue.main.async {
                    self.capturesToday = nil
                    self.lastError = "Failed to parse daemon stats"
                }
            }
        }.resume()
    }
}

@main
struct ScreencapApp: App {
    @StateObject private var daemonStatus = DaemonStatus()

    var body: some Scene {
        MenuBarExtra("Screencap", systemImage: "camera") {
            Text(daemonStatus.statusText)
                .foregroundStyle(daemonStatus.capturesToday == nil ? .red : .primary)

            if daemonStatus.capturesToday == nil, let lastError = daemonStatus.lastError {
                Text(lastError)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Divider()

            Button("Quit") {
                NSApplication.shared.terminate(nil)
            }
            .keyboardShortcut("q")
        }
    }
}