import AppKit
import Foundation
import ScreenCaptureKit

private enum ScreenCaptureBridgeError: Error {
    case invalidDisplayID(Int64)
    case unexpectedNilImage
    case unexpectedTaskResult
}

private final class BlockingTaskBox<T>: @unchecked Sendable {
    var result: Result<T, Error>?
}

@_cdecl("get_display_count")
public func get_display_count() -> Int32 {
    guard #available(macOS 14.0, *) else {
        return -1
    }

    let result = blockingTask {
        let content = try await SCShareableContent.current
        return Int32(content.displays.count)
    }

    switch result {
    case .success(let count):
        return count
    case .failure:
        return -1
    }
}

@_cdecl("copy_display_ids")
public func copy_display_ids(_ buffer: UnsafeMutablePointer<UInt32>?, _ maxCount: Int32) -> Int32 {
    guard #available(macOS 14.0, *) else {
        return -1
    }

    guard maxCount >= 0 else {
        return -1
    }

    let result = blockingTask {
        let content = try await SCShareableContent.current
        return content.displays.map(\.displayID)
    }

    switch result {
    case .success(let displayIDs):
        guard displayIDs.count <= Int(maxCount), let buffer else {
            return -1
        }

        for (index, displayID) in displayIDs.enumerated() {
            buffer[index] = displayID
        }
        return Int32(displayIDs.count)
    case .failure:
        return -1
    }
}

@_cdecl("capture_screenshot")
public func capture_screenshot(
    _ displayID: Int64,
    _ outputPath: UnsafePointer<CChar>?,
    _ quality: UInt8
) -> Bool {
    guard let outputPath else {
        return false
    }

    guard #available(macOS 14.0, *) else {
        return false
    }

    let outputURL = URL(
        fileURLWithFileSystemRepresentation: outputPath,
        isDirectory: false,
        relativeTo: nil
    )
    let quality = min(Double(quality), 100.0) / 100.0

    let result = blockingTask {
        let content = try await SCShareableContent.current
        let display = try resolveDisplay(displayID, in: content)
        let filter = SCContentFilter(display: display, excludingWindows: [])
        let configuration = SCStreamConfiguration()
        configuration.width = Int(filter.contentRect.width * CGFloat(filter.pointPixelScale))
        configuration.height = Int(filter.contentRect.height * CGFloat(filter.pointPixelScale))
        configuration.showsCursor = true

        let image = try await SCScreenshotManager.captureImage(
            contentFilter: filter,
            configuration: configuration
        )
        try writeJPEG(image: image, to: outputURL, quality: quality)
    }

    if case .failure = result {
        return false
    }

    return true
}

@available(macOS 14.0, *)
private func resolveDisplay(_ displayID: Int64, in content: SCShareableContent) throws -> SCDisplay {
    guard let display = content.displays.first(where: { Int64($0.displayID) == displayID }) else {
        throw ScreenCaptureBridgeError.invalidDisplayID(displayID)
    }

    return display
}

private func writeJPEG(image: CGImage, to url: URL, quality: Double) throws {
    let bitmap = NSBitmapImageRep(cgImage: image)
    let properties: [NSBitmapImageRep.PropertyKey: Any] = [
        .compressionFactor: quality,
    ]

    guard let data = bitmap.representation(using: .jpeg, properties: properties) else {
        throw ScreenCaptureBridgeError.unexpectedNilImage
    }

    try data.write(to: url, options: .atomic)
}

private func blockingTask<T>(_ operation: @escaping @Sendable () async throws -> T) -> Result<T, Error> {
    let semaphore = DispatchSemaphore(value: 0)
    let box = BlockingTaskBox<T>()

    Task.detached {
        defer { semaphore.signal() }
        do {
            box.result = .success(try await operation())
        } catch {
            box.result = .failure(error)
        }
    }

    semaphore.wait()
    return box.result ?? .failure(ScreenCaptureBridgeError.unexpectedTaskResult)
}
