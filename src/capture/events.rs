use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use tokio::{
    sync::mpsc,
    task::JoinHandle,
};

const NATIVE_EVENT_APP_SWITCH: u32 = 1;
const NATIVE_EVENT_KEY_DOWN: u32 = 2;
const NATIVE_EVENT_MOUSE_MOVE: u32 = 3;

const KEYBOARD_BURST_MIN_KEYS: usize = 6;
const KEYBOARD_BURST_WINDOW: Duration = Duration::from_secs(2);
const MOUSE_RESUME_DISTANCE: f64 = 120.0;

type NativeEventSender = mpsc::Sender<NativeInputEvent>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTrigger {
    AppSwitch,
    KeyboardBurst,
    MouseResume,
}

#[derive(Debug)]
pub enum EventListenerStart {
    Active(EventListenerGuard),
    Disabled { reason: String },
}

#[derive(Debug)]
pub struct EventListenerGuard {
    processing_task: JoinHandle<()>,
}

pub fn start_capture_trigger_listener(
    trigger_sender: mpsc::Sender<CaptureTrigger>,
    idle_threshold: Duration,
) -> EventListenerStart {
    let (native_event_sender, native_event_receiver) = mpsc::channel(256);
    set_native_event_sender(native_event_sender);

    if !native::start_native_event_listener(on_native_event) {
        clear_native_event_sender();
        return EventListenerStart::Disabled {
            reason: "Accessibility permission is unavailable or CGEventTap could not be created"
                .to_string(),
        };
    }

    let processing_task = tokio::spawn(process_native_events(
        native_event_receiver,
        trigger_sender,
        idle_threshold,
    ));

    EventListenerStart::Active(EventListenerGuard { processing_task })
}

impl Drop for EventListenerGuard {
    fn drop(&mut self) {
        native::stop_native_event_listener();
        clear_native_event_sender();
        self.processing_task.abort();
    }
}

static NATIVE_EVENT_SENDER: OnceLock<Mutex<Option<NativeEventSender>>> = OnceLock::new();

fn native_event_sender_slot() -> &'static Mutex<Option<NativeEventSender>> {
    NATIVE_EVENT_SENDER.get_or_init(|| Mutex::new(None))
}

fn set_native_event_sender(sender: NativeEventSender) {
    let mut guard = native_event_sender_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(sender);
}

fn clear_native_event_sender() {
    let mut guard = native_event_sender_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = None;
}

extern "C" fn on_native_event(kind: u32, x: f64, y: f64) {
    let sender = native_event_sender_slot()
        .lock()
        .ok()
        .and_then(|guard| guard.clone());

    let Some(sender) = sender else {
        return;
    };

    let Some(event) = NativeInputEvent::from_ffi(kind, x, y, Instant::now()) else {
        return;
    };

    let _ = sender.try_send(event);
}

async fn process_native_events(
    mut native_event_receiver: mpsc::Receiver<NativeInputEvent>,
    trigger_sender: mpsc::Sender<CaptureTrigger>,
    idle_threshold: Duration,
) {
    let mut detector = TriggerDetector::new(idle_threshold);

    while let Some(event) = native_event_receiver.recv().await {
        let Some(trigger) = detector.observe(event) else {
            continue;
        };

        if trigger_sender.send(trigger).await.is_err() {
            break;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CursorPosition {
    x: f64,
    y: f64,
}

impl CursorPosition {
    fn distance_to(self, other: Self) -> f64 {
        let delta_x = self.x - other.x;
        let delta_y = self.y - other.y;
        (delta_x.powi(2) + delta_y.powi(2)).sqrt()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct NativeInputEvent {
    kind: NativeInputKind,
    observed_at: Instant,
}

impl NativeInputEvent {
    fn from_ffi(kind: u32, x: f64, y: f64, observed_at: Instant) -> Option<Self> {
        let kind = match kind {
            NATIVE_EVENT_APP_SWITCH => NativeInputKind::AppSwitch,
            NATIVE_EVENT_KEY_DOWN => NativeInputKind::KeyDown,
            NATIVE_EVENT_MOUSE_MOVE => NativeInputKind::MouseMove(CursorPosition { x, y }),
            _ => return None,
        };

        Some(Self { kind, observed_at })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NativeInputKind {
    AppSwitch,
    KeyDown,
    MouseMove(CursorPosition),
}

#[derive(Debug)]
struct TriggerDetector {
    idle_threshold: Duration,
    last_input_at: Option<Instant>,
    keyboard_burst: Option<KeyboardBurstState>,
    mouse_resume: Option<MouseResumeState>,
}

impl TriggerDetector {
    fn new(idle_threshold: Duration) -> Self {
        Self {
            idle_threshold,
            last_input_at: None,
            keyboard_burst: None,
            mouse_resume: None,
        }
    }

    fn observe(&mut self, event: NativeInputEvent) -> Option<CaptureTrigger> {
        let was_idle = self
            .last_input_at
            .map(|last_input| event.observed_at.saturating_duration_since(last_input) >= self.idle_threshold)
            .unwrap_or(true);
        self.last_input_at = Some(event.observed_at);

        match event.kind {
            NativeInputKind::AppSwitch => {
                self.reset_pending_activity();
                Some(CaptureTrigger::AppSwitch)
            }
            NativeInputKind::KeyDown => self.observe_key_down(event.observed_at, was_idle),
            NativeInputKind::MouseMove(position) => self.observe_mouse_move(position, was_idle),
        }
    }

    fn observe_key_down(&mut self, observed_at: Instant, was_idle: bool) -> Option<CaptureTrigger> {
        self.mouse_resume = None;

        if was_idle {
            self.keyboard_burst = Some(KeyboardBurstState::new(observed_at));
            return None;
        }

        let burst = self.keyboard_burst.as_mut()?;

        if observed_at.saturating_duration_since(burst.started_at) > KEYBOARD_BURST_WINDOW {
            self.keyboard_burst = None;
            return None;
        }

        burst.record_key();
        if burst.key_count >= KEYBOARD_BURST_MIN_KEYS {
            self.reset_pending_activity();
            return Some(CaptureTrigger::KeyboardBurst);
        }

        None
    }

    fn observe_mouse_move(&mut self, position: CursorPosition, was_idle: bool) -> Option<CaptureTrigger> {
        self.keyboard_burst = None;

        if was_idle {
            self.mouse_resume = Some(MouseResumeState::new(position));
            return None;
        }

        let mouse_resume = self.mouse_resume?;

        if mouse_resume.anchor.distance_to(position) >= MOUSE_RESUME_DISTANCE {
            self.reset_pending_activity();
            return Some(CaptureTrigger::MouseResume);
        }

        None
    }

    fn reset_pending_activity(&mut self) {
        self.keyboard_burst = None;
        self.mouse_resume = None;
    }
}

#[derive(Debug, Clone, Copy)]
struct KeyboardBurstState {
    started_at: Instant,
    key_count: usize,
}

impl KeyboardBurstState {
    fn new(started_at: Instant) -> Self {
        Self {
            started_at,
            key_count: 1,
        }
    }

    fn record_key(&mut self) {
        self.key_count += 1;
    }
}

#[derive(Debug, Clone, Copy)]
struct MouseResumeState {
    anchor: CursorPosition,
}

impl MouseResumeState {
    fn new(anchor: CursorPosition) -> Self {
        Self { anchor }
    }
}

#[cfg(not(feature = "mock-capture"))]
mod native {
    unsafe extern "C" {
        #[link_name = "start_native_event_listener"]
        fn ffi_start_native_event_listener(callback: extern "C" fn(u32, f64, f64)) -> bool;

        #[link_name = "stop_native_event_listener"]
        fn ffi_stop_native_event_listener();
    }

    pub(super) fn start_native_event_listener(callback: extern "C" fn(u32, f64, f64)) -> bool {
        unsafe { ffi_start_native_event_listener(callback) }
    }

    pub(super) fn stop_native_event_listener() {
        unsafe { ffi_stop_native_event_listener() };
    }
}

#[cfg(feature = "mock-capture")]
mod native {
    use std::sync::atomic::{AtomicBool, Ordering};

    static LISTENER_AVAILABLE: AtomicBool = AtomicBool::new(true);

    pub(super) fn start_native_event_listener(_callback: extern "C" fn(u32, f64, f64)) -> bool {
        LISTENER_AVAILABLE.load(Ordering::SeqCst)
    }

    pub(super) fn stop_native_event_listener() {}

    #[cfg(test)]
    pub(super) fn set_listener_available(available: bool) {
        LISTENER_AVAILABLE.store(available, Ordering::SeqCst);
    }
}

#[cfg(all(test, feature = "mock-capture"))]
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc;

    use super::{
        native, CaptureTrigger, EventListenerStart, NativeInputEvent, TriggerDetector,
        NATIVE_EVENT_APP_SWITCH, NATIVE_EVENT_KEY_DOWN, NATIVE_EVENT_MOUSE_MOVE,
    };

    fn key_event(at: std::time::Instant) -> NativeInputEvent {
        NativeInputEvent::from_ffi(NATIVE_EVENT_KEY_DOWN, 0.0, 0.0, at).expect("key event")
    }

    fn app_switch_event(at: std::time::Instant) -> NativeInputEvent {
        NativeInputEvent::from_ffi(NATIVE_EVENT_APP_SWITCH, 0.0, 0.0, at).expect("app switch event")
    }

    fn mouse_event(at: std::time::Instant, x: f64, y: f64) -> NativeInputEvent {
        NativeInputEvent::from_ffi(NATIVE_EVENT_MOUSE_MOVE, x, y, at).expect("mouse event")
    }

    #[test]
    fn app_switch_triggers_immediately() {
        let mut detector = TriggerDetector::new(Duration::from_secs(300));
        let now = std::time::Instant::now();

        assert_eq!(
            detector.observe(app_switch_event(now)),
            Some(CaptureTrigger::AppSwitch)
        );
    }

    #[test]
    fn keyboard_burst_after_idle_triggers_on_sixth_key() {
        let idle_threshold = Duration::from_secs(300);
        let base = std::time::Instant::now();
        let mut detector = TriggerDetector::new(idle_threshold);

        assert_eq!(detector.observe(key_event(base)), None);
        for offset_ms in [200_u64, 400, 600, 800] {
            assert_eq!(detector.observe(key_event(base + Duration::from_millis(offset_ms))), None);
        }
        assert_eq!(
            detector.observe(key_event(base + Duration::from_millis(1_000))),
            Some(CaptureTrigger::KeyboardBurst)
        );
    }

    #[test]
    fn keyboard_burst_without_idle_does_not_trigger() {
        let base = std::time::Instant::now();
        let mut detector = TriggerDetector::new(Duration::from_secs(300));

        assert_eq!(detector.observe(app_switch_event(base)), Some(CaptureTrigger::AppSwitch));
        for offset_ms in [100_u64, 200, 300, 400, 500, 600] {
            assert_eq!(detector.observe(key_event(base + Duration::from_millis(offset_ms))), None);
        }
    }

    #[test]
    fn mouse_resume_requires_significant_distance_after_idle() {
        let idle_threshold = Duration::from_secs(300);
        let base = std::time::Instant::now();
        let mut detector = TriggerDetector::new(idle_threshold);

        assert_eq!(detector.observe(app_switch_event(base)), Some(CaptureTrigger::AppSwitch));
        assert_eq!(
            detector.observe(mouse_event(base + idle_threshold + Duration::from_secs(1), 50.0, 50.0)),
            None
        );
        assert_eq!(
            detector.observe(mouse_event(
                base + idle_threshold + Duration::from_secs(1) + Duration::from_millis(100),
                120.0,
                100.0,
            )),
            None
        );
        assert_eq!(
            detector.observe(mouse_event(
                base + idle_threshold + Duration::from_secs(1) + Duration::from_millis(200),
                210.0,
                160.0,
            )),
            Some(CaptureTrigger::MouseResume)
        );
    }

    #[tokio::test]
    async fn listener_start_reports_disabled_when_native_listener_is_unavailable() {
        native::set_listener_available(false);
        let (sender, _receiver) = mpsc::channel(1);

        let start = super::start_capture_trigger_listener(sender, Duration::from_secs(300));
        assert!(matches!(start, EventListenerStart::Disabled { .. }));

        native::set_listener_available(true);
    }
}
