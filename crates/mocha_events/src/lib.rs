//! Mocha Browser's internal DOM event system.
//!
//! This is the engine-internal event model — **not** JavaScript. Listeners are
//! Rust callbacks; a later milestone will bridge JavaScript listeners onto this
//! same dispatch. It implements the DOM event flow (capturing → at-target →
//! bubbling), listener registration/removal, `once` listeners, propagation
//! control (`stopPropagation`/`stopImmediatePropagation`), and cancelation
//! (`preventDefault`, effective only for cancelable events).
//!
//! See `docs/architecture/events.md`.

mod dispatcher;
mod event;

pub use dispatcher::{DispatchResult, EventDispatcher, EventListenerOptions, ListenerId};
pub use event::{Event, EventData, EventPhase, MouseButton};

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_dom::{Document, NodeId};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A document `root → a → b → target`, returning (document, a, b, target).
    fn chain_document() -> (Document, NodeId, NodeId, NodeId) {
        let mut document = Document::new();
        let root = document.root_id();
        let a = document.create_element("div", Vec::new());
        let b = document.create_element("div", Vec::new());
        let target = document.create_element("span", Vec::new());
        document.append_child(root, a).unwrap();
        document.append_child(a, b).unwrap();
        document.append_child(b, target).unwrap();
        (document, a, b, target)
    }

    /// A shared recording log for listener invocation order.
    type Log = Rc<RefCell<Vec<String>>>;

    fn recorder(log: &Log, label: &'static str) -> Box<dyn FnMut(&mut Event)> {
        let log = Rc::clone(log);
        Box::new(move |_event: &mut Event| log.borrow_mut().push(label.to_string()))
    }

    #[test]
    fn event_initializes_correctly() {
        let event = Event::new("click", NodeId(3));
        assert_eq!(event.event_type, "click");
        assert_eq!(event.target, NodeId(3));
        assert_eq!(event.phase, EventPhase::None);
        assert!(event.bubbles && event.cancelable);
        assert!(!event.default_prevented);
    }

    #[test]
    fn prevent_default_only_works_when_cancelable() {
        let mut cancelable = Event::with_options("x", NodeId(0), true, true);
        cancelable.prevent_default();
        assert!(cancelable.default_prevented);

        let mut not_cancelable = Event::with_options("x", NodeId(0), true, false);
        not_cancelable.prevent_default();
        assert!(!not_cancelable.default_prevented);
    }

    #[test]
    fn capturing_runs_root_to_target_then_bubbling_target_to_root() {
        let (document, a, b, target) = chain_document();
        let log: Log = Rc::default();
        let mut dispatcher = EventDispatcher::new();

        dispatcher.add_event_listener(
            a,
            "click",
            EventListenerOptions::capture(),
            recorder(&log, "a-cap"),
        );
        dispatcher.add_event_listener(
            b,
            "click",
            EventListenerOptions::capture(),
            recorder(&log, "b-cap"),
        );
        dispatcher.add_event_listener(
            target,
            "click",
            EventListenerOptions::capture(),
            recorder(&log, "t-cap"),
        );
        dispatcher.add_event_listener(
            target,
            "click",
            EventListenerOptions::bubble(),
            recorder(&log, "t-bub"),
        );
        dispatcher.add_event_listener(
            b,
            "click",
            EventListenerOptions::bubble(),
            recorder(&log, "b-bub"),
        );
        dispatcher.add_event_listener(
            a,
            "click",
            EventListenerOptions::bubble(),
            recorder(&log, "a-bub"),
        );

        let mut event = Event::new("click", target);
        let result = dispatcher.dispatch_event(&document, &mut event).unwrap();

        assert_eq!(
            *log.borrow(),
            vec!["a-cap", "b-cap", "t-cap", "t-bub", "b-bub", "a-bub"]
        );
        assert_eq!(result.invoked_listeners, 6);
        // State resets after dispatch.
        assert_eq!(event.phase, EventPhase::None);
        assert_eq!(event.current_target, None);
    }

    #[test]
    fn non_bubbling_event_does_not_bubble() {
        let (document, a, _b, target) = chain_document();
        let log: Log = Rc::default();
        let mut dispatcher = EventDispatcher::new();
        dispatcher.add_event_listener(
            a,
            "x",
            EventListenerOptions::bubble(),
            recorder(&log, "a-bub"),
        );
        dispatcher.add_event_listener(
            target,
            "x",
            EventListenerOptions::bubble(),
            recorder(&log, "t-bub"),
        );

        let mut event = Event::with_options("x", target, false, true);
        dispatcher.dispatch_event(&document, &mut event).unwrap();
        // Only the target's bubble listener runs; ancestors do not.
        assert_eq!(*log.borrow(), vec!["t-bub"]);
    }

    #[test]
    fn listeners_on_same_node_run_in_registration_order() {
        let (document, _a, _b, target) = chain_document();
        let log: Log = Rc::default();
        let mut dispatcher = EventDispatcher::new();
        dispatcher.add_event_listener(
            target,
            "click",
            EventListenerOptions::bubble(),
            recorder(&log, "first"),
        );
        dispatcher.add_event_listener(
            target,
            "click",
            EventListenerOptions::bubble(),
            recorder(&log, "second"),
        );
        let mut event = Event::new("click", target);
        dispatcher.dispatch_event(&document, &mut event).unwrap();
        assert_eq!(*log.borrow(), vec!["first", "second"]);
    }

    #[test]
    fn stop_propagation_stops_later_nodes() {
        let (document, a, b, target) = chain_document();
        let log: Log = Rc::default();
        let mut dispatcher = EventDispatcher::new();
        // Stop during bubbling at b; a should not see the event.
        dispatcher.add_event_listener(
            target,
            "click",
            EventListenerOptions::bubble(),
            recorder(&log, "t"),
        );
        {
            let log = Rc::clone(&log);
            dispatcher.add_event_listener(
                b,
                "click",
                EventListenerOptions::bubble(),
                Box::new(move |event: &mut Event| {
                    log.borrow_mut().push("b".to_string());
                    event.stop_propagation();
                }),
            );
        }
        dispatcher.add_event_listener(
            a,
            "click",
            EventListenerOptions::bubble(),
            recorder(&log, "a"),
        );

        let mut event = Event::new("click", target);
        let result = dispatcher.dispatch_event(&document, &mut event).unwrap();
        assert_eq!(*log.borrow(), vec!["t", "b"]);
        assert!(result.propagation_stopped);
    }

    #[test]
    fn stop_immediate_propagation_stops_same_node_listeners() {
        let (document, _a, _b, target) = chain_document();
        let log: Log = Rc::default();
        let mut dispatcher = EventDispatcher::new();
        {
            let log = Rc::clone(&log);
            dispatcher.add_event_listener(
                target,
                "click",
                EventListenerOptions::bubble(),
                Box::new(move |event: &mut Event| {
                    log.borrow_mut().push("first".to_string());
                    event.stop_immediate_propagation();
                }),
            );
        }
        dispatcher.add_event_listener(
            target,
            "click",
            EventListenerOptions::bubble(),
            recorder(&log, "second"),
        );

        let mut event = Event::new("click", target);
        dispatcher.dispatch_event(&document, &mut event).unwrap();
        assert_eq!(*log.borrow(), vec!["first"]);
    }

    #[test]
    fn remove_event_listener_prevents_future_calls() {
        let (document, _a, _b, target) = chain_document();
        let log: Log = Rc::default();
        let mut dispatcher = EventDispatcher::new();
        let id = dispatcher.add_event_listener(
            target,
            "click",
            EventListenerOptions::bubble(),
            recorder(&log, "x"),
        );
        assert!(dispatcher.remove_event_listener(id));
        assert!(!dispatcher.remove_event_listener(id)); // already gone

        let mut event = Event::new("click", target);
        let result = dispatcher.dispatch_event(&document, &mut event).unwrap();
        assert!(log.borrow().is_empty());
        assert_eq!(result.invoked_listeners, 0);
    }

    #[test]
    fn once_listener_runs_only_once() {
        let (document, _a, _b, target) = chain_document();
        let log: Log = Rc::default();
        let mut dispatcher = EventDispatcher::new();
        dispatcher.add_event_listener(
            target,
            "click",
            EventListenerOptions {
                capture: false,
                once: true,
            },
            recorder(&log, "once"),
        );

        let mut first = Event::new("click", target);
        dispatcher.dispatch_event(&document, &mut first).unwrap();
        let mut second = Event::new("click", target);
        dispatcher.dispatch_event(&document, &mut second).unwrap();
        assert_eq!(*log.borrow(), vec!["once"]);
    }

    #[test]
    fn only_matching_event_type_runs() {
        let (document, _a, _b, target) = chain_document();
        let log: Log = Rc::default();
        let mut dispatcher = EventDispatcher::new();
        dispatcher.add_event_listener(
            target,
            "keydown",
            EventListenerOptions::bubble(),
            recorder(&log, "key"),
        );
        let mut event = Event::new("click", target);
        dispatcher.dispatch_event(&document, &mut event).unwrap();
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn invalid_target_errors_clearly() {
        let document = Document::new();
        let mut dispatcher = EventDispatcher::new();
        let mut event = Event::new("click", NodeId(999));
        let error = dispatcher
            .dispatch_event(&document, &mut event)
            .unwrap_err();
        assert!(matches!(error, mocha_error::MochaError::Dom(_)));
    }
}
