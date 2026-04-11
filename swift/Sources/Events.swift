import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

private let nativeEventAppSwitch: UInt32 = 1
private let nativeEventKeyDown: UInt32 = 2
private let nativeEventMouseMove: UInt32 = 3

private var nativeEventListener: NativeEventListener?

@_cdecl("start_native_event_listener")
public func start_native_event_listener(
    _ callback: (@convention(c) (UInt32, Double, Double) -> Void)?
) -> Bool {
    guard let callback else {
        return false
    }

    stop_native_event_listener()

    let listener = NativeEventListener(callback: callback)
    guard listener.start() else {
        return false
    }

    nativeEventListener = listener
    return true
}

@_cdecl("stop_native_event_listener")
public func stop_native_event_listener() {
    nativeEventListener?.stop()
    nativeEventListener = nil
}

private final class NativeEventListener {
    typealias Callback = @convention(c) (UInt32, Double, Double) -> Void

    private let callback: Callback
    private let stateLock = NSLock()
    private let startupCondition = NSCondition()
    private var startupResult: Bool?
    private var runLoop: CFRunLoop?
    private var eventTap: CFMachPort?
    private var appChangeObserver: NSObjectProtocol?

    init(callback: @escaping Callback) {
        self.callback = callback
    }

    func start() -> Bool {
        guard AXIsProcessTrusted() else {
            return false
        }

        let thread = Thread { [self] in
            runEventLoop()
        }
        thread.name = "ScreencapEventTap"
        thread.start()

        startupCondition.lock()
        while startupResult == nil {
            startupCondition.wait()
        }
        let started = startupResult ?? false
        startupCondition.unlock()

        if !started {
            stop()
        }

        return started
    }

    func stop() {
        removeAppChangeObserver()

        stateLock.lock()
        let runLoop = self.runLoop
        let eventTap = self.eventTap
        stateLock.unlock()

        if let eventTap {
            CGEvent.tapEnable(tap: eventTap, enable: false)
        }

        if let runLoop {
            CFRunLoopStop(runLoop)
            CFRunLoopWakeUp(runLoop)
        }

        startupCondition.lock()
        if startupResult == nil {
            startupResult = false
            startupCondition.signal()
        }
        startupCondition.unlock()
    }

    private func runEventLoop() {
        guard let eventTap = makeEventTap() else {
            finishStartup(false)
            return
        }

        guard let eventSource = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, eventTap, 0) else {
            CFMachPortInvalidate(eventTap)
            finishStartup(false)
            return
        }

        let runLoop = CFRunLoopGetCurrent()

        stateLock.lock()
        self.runLoop = runLoop
        self.eventTap = eventTap
        self.appChangeObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification,
            object: nil,
            queue: nil
        ) { [weak self] _ in
            self?.callback(nativeEventAppSwitch, 0, 0)
        }
        stateLock.unlock()

        CFRunLoopAddSource(runLoop, eventSource, CFRunLoopMode.commonModes)
        CGEvent.tapEnable(tap: eventTap, enable: true)
        finishStartup(true)

        CFRunLoopRun()

        removeAppChangeObserver()
        CFRunLoopRemoveSource(runLoop, eventSource, CFRunLoopMode.commonModes)
        CFMachPortInvalidate(eventTap)

        stateLock.lock()
        self.runLoop = nil
        self.eventTap = nil
        stateLock.unlock()
    }

    private func makeEventTap() -> CFMachPort? {
        let eventMask = eventMask(for: .keyDown)
            | eventMask(for: .mouseMoved)
            | eventMask(for: .leftMouseDragged)
            | eventMask(for: .rightMouseDragged)
            | eventMask(for: .otherMouseDragged)
        let userInfo = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())

        let callback: CGEventTapCallBack = { proxy, type, event, userInfo in
            guard let userInfo else {
                return Unmanaged.passUnretained(event)
            }

            let listener = Unmanaged<NativeEventListener>.fromOpaque(userInfo).takeUnretainedValue()
            return listener.handleTapEvent(proxy: proxy, type: type, event: event)
        }

        return CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: .listenOnly,
            eventsOfInterest: eventMask,
            callback: callback,
            userInfo: userInfo
        )
    }

    private func handleTapEvent(
        proxy _: CGEventTapProxy,
        type: CGEventType,
        event: CGEvent
    ) -> Unmanaged<CGEvent>? {
        switch type {
        case .tapDisabledByTimeout, .tapDisabledByUserInput:
            reenableEventTap()
        case .keyDown:
            callback(nativeEventKeyDown, 0, 0)
        case .mouseMoved, .leftMouseDragged, .rightMouseDragged, .otherMouseDragged:
            let location = event.location
            callback(nativeEventMouseMove, location.x, location.y)
        default:
            break
        }

        return Unmanaged.passUnretained(event)
    }

    private func reenableEventTap() {
        stateLock.lock()
        let eventTap = self.eventTap
        stateLock.unlock()

        if let eventTap {
            CGEvent.tapEnable(tap: eventTap, enable: true)
        }
    }

    private func removeAppChangeObserver() {
        stateLock.lock()
        let observer = self.appChangeObserver
        self.appChangeObserver = nil
        stateLock.unlock()

        if let observer {
            NSWorkspace.shared.notificationCenter.removeObserver(observer)
        }
    }

    private func finishStartup(_ started: Bool) {
        startupCondition.lock()
        startupResult = started
        startupCondition.signal()
        startupCondition.unlock()
    }

    private func eventMask(for type: CGEventType) -> CGEventMask {
        CGEventMask(1) << CGEventMask(type.rawValue)
    }
}
