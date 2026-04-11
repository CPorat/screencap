import AppKit
import CoreGraphics
import Darwin
import Foundation


private var appChangeObserver: NSObjectProtocol?

@_cdecl("start_app_change_listener")
public func start_app_change_listener(_ callback: (@convention(c) () -> Void)?) {
    guard let callback else {
        return
    }

    stop_app_change_listener()
    appChangeObserver = NSWorkspace.shared.notificationCenter.addObserver(
        forName: NSWorkspace.didActivateApplicationNotification,
        object: nil,
        queue: nil
    ) { _ in
        callback()
    }
}

@_cdecl("stop_app_change_listener")
public func stop_app_change_listener() {
    guard let observer = appChangeObserver else {
        return
    }

    NSWorkspace.shared.notificationCenter.removeObserver(observer)
    appChangeObserver = nil
}

private enum WindowInfoBridgeError: Error {
    case missingFrontmostApplication
    case stringAllocationFailed
}

private struct ActiveWindowInfo {
    let appName: String
    let windowTitle: String
    let bundleID: String
}

@_cdecl("get_active_window")
public func get_active_window(
    _ outAppName: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?,
    _ outWindowTitle: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?,
    _ outBundleID: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> Bool {
    guard let outAppName, let outWindowTitle, let outBundleID else {
        return false
    }

    outAppName.pointee = nil
    outWindowTitle.pointee = nil
    outBundleID.pointee = nil

    var appName: UnsafeMutablePointer<CChar>?
    var windowTitle: UnsafeMutablePointer<CChar>?
    var bundleID: UnsafeMutablePointer<CChar>?

    do {
        let info = try currentActiveWindowInfo()
        appName = try duplicateCString(info.appName)
        windowTitle = try duplicateCString(info.windowTitle)
        bundleID = try duplicateCString(info.bundleID)

        outAppName.pointee = appName
        outWindowTitle.pointee = windowTitle
        outBundleID.pointee = bundleID
        return true
    } catch {
        if let appName {
            free_bridge_string(appName)
        }
        if let windowTitle {
            free_bridge_string(windowTitle)
        }
        if let bundleID {
            free_bridge_string(bundleID)
        }
        return false
    }
}

@_cdecl("free_bridge_string")
public func free_bridge_string(_ value: UnsafeMutablePointer<CChar>?) {
    guard let value else {
        return
    }

    free(value)
}

private func currentActiveWindowInfo() throws -> ActiveWindowInfo {
    guard let application = NSWorkspace.shared.frontmostApplication else {
        throw WindowInfoBridgeError.missingFrontmostApplication
    }

    return ActiveWindowInfo(
        appName: resolvedAppName(for: application),
        windowTitle: resolvedWindowTitle(for: application.processIdentifier) ?? "",
        bundleID: application.bundleIdentifier ?? ""
    )
}

private func resolvedAppName(for application: NSRunningApplication) -> String {
    if let localizedName = application.localizedName, !localizedName.isEmpty {
        return localizedName
    }

    if let bundleIdentifier = application.bundleIdentifier, !bundleIdentifier.isEmpty {
        return bundleIdentifier
    }

    if let executableURL = application.executableURL {
        let executableName = executableURL.deletingPathExtension().lastPathComponent
        if !executableName.isEmpty {
            return executableName
        }
    }

    return ""
}

private func resolvedWindowTitle(for processIdentifier: pid_t) -> String? {
    guard let windowList = CGWindowListCopyWindowInfo(
        [.optionOnScreenOnly, .excludeDesktopElements],
        kCGNullWindowID
    ) as? [[String: Any]] else {
        return nil
    }

    for window in windowList {
        guard let ownerPID = window[kCGWindowOwnerPID as String] as? pid_t,
              ownerPID == processIdentifier
        else {
            continue
        }

        let layer = window[kCGWindowLayer as String] as? Int ?? 0
        guard layer == 0 else {
            continue
        }

        if let title = window[kCGWindowName as String] as? String, !title.isEmpty {
            return title
        }

        return nil
    }

    return nil
}

private func duplicateCString(_ value: String) throws -> UnsafeMutablePointer<CChar> {
    try value.withCString { pointer in
        guard let duplicated = strdup(pointer) else {
            throw WindowInfoBridgeError.stringAllocationFailed
        }

        return duplicated
    }
}
