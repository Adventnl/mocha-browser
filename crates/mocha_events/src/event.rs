//! The event data model.

use mocha_dom::NodeId;

/// The propagation phase an event is currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase {
    /// Not being dispatched.
    None,
    /// Travelling from the root down toward the target.
    Capturing,
    /// At the target node.
    AtTarget,
    /// Travelling from the target back up to the root.
    Bubbling,
}

/// A pointer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Primary (usually left) button.
    Left,
    /// Middle button.
    Middle,
    /// Secondary (usually right) button.
    Right,
}

/// Event-type-specific payload.
#[derive(Debug, Clone, PartialEq)]
pub enum EventData {
    /// No extra data.
    Generic,
    /// A mouse/pointer event at viewport coordinates `(x, y)`.
    Mouse {
        /// X coordinate.
        x: f32,
        /// Y coordinate.
        y: f32,
        /// Which button.
        button: MouseButton,
    },
    /// A keyboard event.
    Keyboard {
        /// The logical key value (e.g. "a", "Enter").
        key: String,
        /// The physical key code (e.g. "KeyA").
        code: String,
    },
}

/// A DOM-like event.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// The event type, e.g. `"click"`.
    pub event_type: String,
    /// The node the event was dispatched at.
    pub target: NodeId,
    /// The node currently handling the event (set during dispatch).
    pub current_target: Option<NodeId>,
    /// The current propagation phase.
    pub phase: EventPhase,
    /// Whether the event bubbles.
    pub bubbles: bool,
    /// Whether the event's default action can be cancelled.
    pub cancelable: bool,
    /// Whether `prevent_default` has taken effect.
    pub default_prevented: bool,
    /// Type-specific payload.
    pub data: EventData,

    pub(crate) propagation_stopped: bool,
    pub(crate) immediate_stopped: bool,
}

impl Event {
    /// A bubbling, cancelable generic event.
    pub fn new(event_type: impl Into<String>, target: NodeId) -> Event {
        Event::with_options(event_type, target, true, true)
    }

    /// A generic event with explicit `bubbles`/`cancelable`.
    pub fn with_options(
        event_type: impl Into<String>,
        target: NodeId,
        bubbles: bool,
        cancelable: bool,
    ) -> Event {
        Event {
            event_type: event_type.into(),
            target,
            current_target: None,
            phase: EventPhase::None,
            bubbles,
            cancelable,
            default_prevented: false,
            data: EventData::Generic,
            propagation_stopped: false,
            immediate_stopped: false,
        }
    }

    /// A mouse event (`mousedown`/`mouseup`/`mousemove`/`click`). Bubbling and
    /// cancelable.
    pub fn mouse(
        event_type: impl Into<String>,
        target: NodeId,
        x: f32,
        y: f32,
        button: MouseButton,
    ) -> Event {
        let mut event = Event::with_options(event_type, target, true, true);
        event.data = EventData::Mouse { x, y, button };
        event
    }

    /// A `click` event (a left-button mouse event).
    pub fn click(target: NodeId, x: f32, y: f32) -> Event {
        Event::mouse("click", target, x, y, MouseButton::Left)
    }

    /// A keyboard event (`keydown`/`keyup`). Bubbling and cancelable.
    pub fn keyboard(
        event_type: impl Into<String>,
        target: NodeId,
        key: impl Into<String>,
        code: impl Into<String>,
    ) -> Event {
        let mut event = Event::with_options(event_type, target, true, true);
        event.data = EventData::Keyboard {
            key: key.into(),
            code: code.into(),
        };
        event
    }

    /// Stop the event reaching later nodes in the propagation path. Listeners
    /// already scheduled on the current node still run.
    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }

    /// Like [`stop_propagation`](Self::stop_propagation), and also stop any
    /// remaining listeners on the current node.
    pub fn stop_immediate_propagation(&mut self) {
        self.propagation_stopped = true;
        self.immediate_stopped = true;
    }

    /// Mark the default action as prevented — only effective for cancelable events.
    pub fn prevent_default(&mut self) {
        if self.cancelable {
            self.default_prevented = true;
        }
    }
}
