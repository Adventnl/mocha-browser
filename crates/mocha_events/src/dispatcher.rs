//! Listener registration and the event dispatch algorithm.
//!
//! Listeners are Rust callbacks (`Box<dyn FnMut(&mut Event)>`) — there is **no
//! JavaScript**. The dispatch follows the DOM flow: capturing (root → parent),
//! at-target (capture- then bubble-registered listeners), then bubbling
//! (parent → root) when the event bubbles.

use std::collections::HashMap;

use mocha_dom::{Document, NodeId};
use mocha_error::MochaResult;

use crate::event::{Event, EventPhase};

/// An opaque listener handle returned by [`EventDispatcher::add_event_listener`].
pub type ListenerId = u64;

/// Options for a registered listener.
///
/// `passive` is intentionally **not** modelled (it would be a field that does
/// nothing in this milestone); see `docs/architecture/events.md`.
#[derive(Debug, Clone, Copy, Default)]
pub struct EventListenerOptions {
    /// Register for the capturing phase instead of bubbling.
    pub capture: bool,
    /// Remove the listener automatically after it runs once.
    pub once: bool,
}

impl EventListenerOptions {
    /// Bubble-phase, persistent listener (the default).
    pub fn bubble() -> EventListenerOptions {
        EventListenerOptions::default()
    }

    /// Capture-phase listener.
    pub fn capture() -> EventListenerOptions {
        EventListenerOptions {
            capture: true,
            once: false,
        }
    }
}

struct Listener {
    id: ListenerId,
    event_type: String,
    capture: bool,
    once: bool,
    callback: Box<dyn FnMut(&mut Event)>,
}

/// The outcome of a dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchResult {
    /// Whether the default action was prevented.
    pub default_prevented: bool,
    /// Whether propagation was stopped during dispatch.
    pub propagation_stopped: bool,
    /// How many listeners were invoked.
    pub invoked_listeners: usize,
}

/// Stores listeners per node and runs the dispatch algorithm.
#[derive(Default)]
pub struct EventDispatcher {
    listeners: HashMap<NodeId, Vec<Listener>>,
    next_id: ListenerId,
}

impl EventDispatcher {
    /// Create an empty dispatcher.
    pub fn new() -> EventDispatcher {
        EventDispatcher::default()
    }

    /// Register a listener on `target`, returning its id.
    pub fn add_event_listener(
        &mut self,
        target: NodeId,
        event_type: impl Into<String>,
        options: EventListenerOptions,
        callback: Box<dyn FnMut(&mut Event)>,
    ) -> ListenerId {
        let id = self.next_id;
        self.next_id += 1;
        self.listeners.entry(target).or_default().push(Listener {
            id,
            event_type: event_type.into(),
            capture: options.capture,
            once: options.once,
            callback,
        });
        id
    }

    /// Remove a listener by id. Returns `true` if one was removed.
    pub fn remove_event_listener(&mut self, listener_id: ListenerId) -> bool {
        for listeners in self.listeners.values_mut() {
            if let Some(position) = listeners.iter().position(|l| l.id == listener_id) {
                listeners.remove(position);
                return true;
            }
        }
        false
    }

    /// Dispatch `event` against the DOM, running capture/target/bubble phases.
    ///
    /// Returns a [`MochaError::Dom`](mocha_error::MochaError::Dom) error if the
    /// target node does not exist. After dispatch the event's `phase` is
    /// [`EventPhase::None`] and its `current_target` is `None`.
    pub fn dispatch_event(
        &mut self,
        document: &Document,
        event: &mut Event,
    ) -> MochaResult<DispatchResult> {
        // Validate the target exists.
        document.node(event.target)?;
        let ancestors = document.ancestors(event.target)?; // parent → root
        let mut invoked = 0;

        // Capturing phase: root → parent.
        event.phase = EventPhase::Capturing;
        for &node in ancestors.iter().rev() {
            if event.propagation_stopped {
                break;
            }
            self.invoke(node, true, event, &mut invoked);
        }

        // At target: capture-registered then bubble-registered listeners.
        if !event.propagation_stopped {
            event.phase = EventPhase::AtTarget;
            self.invoke(event.target, true, event, &mut invoked);
            if !event.immediate_stopped {
                self.invoke(event.target, false, event, &mut invoked);
            }
        }

        // Bubbling phase: parent → root (only if the event bubbles).
        if event.bubbles {
            event.phase = EventPhase::Bubbling;
            for &node in ancestors.iter() {
                if event.propagation_stopped {
                    break;
                }
                self.invoke(node, false, event, &mut invoked);
            }
        }

        event.phase = EventPhase::None;
        event.current_target = None;

        Ok(DispatchResult {
            default_prevented: event.default_prevented,
            propagation_stopped: event.propagation_stopped,
            invoked_listeners: invoked,
        })
    }

    /// Invoke the listeners on `node` registered for the current event type and
    /// `capture` flag, in registration order. Honors `stopImmediatePropagation`
    /// and removes `once` listeners after they run.
    fn invoke(&mut self, node: NodeId, capture: bool, event: &mut Event, invoked: &mut usize) {
        event.current_target = Some(node);

        // Snapshot the ids to invoke so listeners added/removed during a callback
        // do not change this dispatch's listener set.
        let ids: Vec<ListenerId> = match self.listeners.get(&node) {
            Some(listeners) => listeners
                .iter()
                .filter(|l| l.capture == capture && l.event_type == event.event_type)
                .map(|l| l.id)
                .collect(),
            None => return,
        };

        for id in ids {
            if event.immediate_stopped {
                break;
            }
            let Some(listeners) = self.listeners.get_mut(&node) else {
                break;
            };
            let Some(position) = listeners.iter().position(|l| l.id == id) else {
                continue; // removed during this dispatch
            };
            (listeners[position].callback)(event);
            *invoked += 1;
            if listeners[position].once {
                listeners.remove(position);
            }
        }
    }
}
