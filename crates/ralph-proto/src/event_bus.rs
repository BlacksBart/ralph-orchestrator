//! Event bus for pub/sub messaging.
//!
//! The event bus routes events to subscribed hats based on topic patterns.
//! An optional observer can be set to receive all published events for
//! recording and benchmarking purposes.

use crate::{Event, Hat, HatId};
use std::collections::HashMap;

/// Type alias for the observer callback function.
type Observer = Box<dyn Fn(&Event) + Send + 'static>;

/// Central pub/sub hub for routing events between hats.
#[derive(Default)]
pub struct EventBus {
    /// Registered hats indexed by ID.
    hats: HashMap<HatId, Hat>,

    /// Pending events for each hat.
    pending: HashMap<HatId, Vec<Event>>,

    /// Optional observer that receives all published events.
    observer: Option<Observer>,
}


impl EventBus {
    /// Creates a new empty event bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets an observer that receives all published events.
    ///
    /// This enables recording sessions by subscribing to the event stream
    /// without modifying the routing logic. The observer is called before
    /// events are routed to subscribers.
    pub fn set_observer<F>(&mut self, observer: F)
    where
        F: Fn(&Event) + Send + 'static,
    {
        self.observer = Some(Box::new(observer));
    }

    /// Clears the observer callback.
    pub fn clear_observer(&mut self) {
        self.observer = None;
    }

    /// Registers a hat with the event bus.
    pub fn register(&mut self, hat: Hat) {
        let id = hat.id.clone();
        self.hats.insert(id.clone(), hat);
        self.pending.entry(id).or_default();
    }

    /// Publishes an event to all subscribed hats.
    ///
    /// Returns the list of hat IDs that received the event.
    /// If an observer is set, it receives the event before routing.
    #[allow(clippy::needless_pass_by_value)] // Event is cloned to multiple recipients
    pub fn publish(&mut self, event: Event) -> Vec<HatId> {
        // Notify observer before routing
        if let Some(ref observer) = self.observer {
            observer(&event);
        }

        let mut recipients = Vec::new();

        // If there's a direct target, route only to that hat
        if let Some(ref target) = event.target {
            if self.hats.contains_key(target) {
                self.pending
                    .entry(target.clone())
                    .or_default()
                    .push(event.clone());
                recipients.push(target.clone());
            }
            return recipients;
        }

        // Otherwise, route to all subscribers
        for (id, hat) in &self.hats {
            if hat.is_subscribed(&event.topic) {
                self.pending
                    .entry(id.clone())
                    .or_default()
                    .push(event.clone());
                recipients.push(id.clone());
            }
        }

        recipients
    }

    /// Takes all pending events for a hat.
    pub fn take_pending(&mut self, hat_id: &HatId) -> Vec<Event> {
        self.pending.remove(hat_id).unwrap_or_default()
    }

    /// Checks if there are any pending events for any hat.
    pub fn has_pending(&self) -> bool {
        self.pending.values().any(|events| !events.is_empty())
    }

    /// Returns the next hat with pending events.
    pub fn next_hat_with_pending(&self) -> Option<&HatId> {
        self.pending
            .iter()
            .find(|(_, events)| !events.is_empty())
            .map(|(id, _)| id)
    }

    /// Gets a hat by ID.
    pub fn get_hat(&self, id: &HatId) -> Option<&Hat> {
        self.hats.get(id)
    }

    /// Returns all registered hat IDs.
    pub fn hat_ids(&self) -> impl Iterator<Item = &HatId> {
        self.hats.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_publish_to_subscriber() {
        let mut bus = EventBus::new();

        let hat = Hat::new("impl", "Implementer").subscribe("task.*");
        bus.register(hat);

        let event = Event::new("task.start", "Start implementing");
        let recipients = bus.publish(event);

        assert_eq!(recipients.len(), 1);
        assert_eq!(recipients[0].as_str(), "impl");
    }

    #[test]
    fn test_no_match() {
        let mut bus = EventBus::new();

        let hat = Hat::new("impl", "Implementer").subscribe("task.*");
        bus.register(hat);

        let event = Event::new("review.done", "Review complete");
        let recipients = bus.publish(event);

        assert!(recipients.is_empty());
    }

    #[test]
    fn test_direct_target() {
        let mut bus = EventBus::new();

        let impl_hat = Hat::new("impl", "Implementer").subscribe("task.*");
        let review_hat = Hat::new("reviewer", "Reviewer").subscribe("impl.*");
        bus.register(impl_hat);
        bus.register(review_hat);

        // Direct target bypasses subscription matching
        let event = Event::new("handoff", "Please review").with_target("reviewer");
        let recipients = bus.publish(event);

        assert_eq!(recipients.len(), 1);
        assert_eq!(recipients[0].as_str(), "reviewer");
    }

    #[test]
    fn test_take_pending() {
        let mut bus = EventBus::new();

        let hat = Hat::new("impl", "Implementer").subscribe("*");
        bus.register(hat);

        bus.publish(Event::new("task.start", "Start"));
        bus.publish(Event::new("task.continue", "Continue"));

        let hat_id = HatId::new("impl");
        let events = bus.take_pending(&hat_id);

        assert_eq!(events.len(), 2);
        assert!(bus.take_pending(&hat_id).is_empty());
    }

    #[test]
    fn test_self_routing_allowed() {
        // Self-routing is allowed to handle LLM non-determinism.
        // Spec acceptance criteria: planner emits build.done (even though builder "should"),
        // event routes back to planner, planner continues (no source-based blocking).
        let mut bus = EventBus::new();

        let planner = Hat::new("planner", "Planner").subscribe("build.done");
        bus.register(planner);

        // Planner emits build.done (wrong hat, but LLMs are non-deterministic)
        let event = Event::new("build.done", "Done").with_source("planner");
        let recipients = bus.publish(event);

        // Event SHOULD route back to planner (self-routing allowed, no source filtering)
        assert_eq!(recipients.len(), 1);
        assert_eq!(recipients[0].as_str(), "planner");
    }

    #[test]
    fn test_observer_receives_all_events() {
        use std::sync::{Arc, Mutex};

        let mut bus = EventBus::new();
        let observed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let observed_clone = Arc::clone(&observed);
        bus.set_observer(move |event| {
            observed_clone
                .lock()
                .unwrap()
                .push(event.payload.clone());
        });

        let hat = Hat::new("impl", "Implementer").subscribe("task.*");
        bus.register(hat);

        // Publish events - observer should see all regardless of routing
        bus.publish(Event::new("task.start", "Start"));
        bus.publish(Event::new("other.event", "Other")); // No subscriber
        bus.publish(Event::new("task.done", "Done"));

        let captured = observed.lock().unwrap();
        assert_eq!(captured.len(), 3);
        assert_eq!(captured[0], "Start");
        assert_eq!(captured[1], "Other");
        assert_eq!(captured[2], "Done");
    }

    #[test]
    fn test_clear_observer() {
        use std::sync::{Arc, Mutex};

        let mut bus = EventBus::new();
        let count = Arc::new(Mutex::new(0));

        let count_clone = Arc::clone(&count);
        bus.set_observer(move |_| {
            *count_clone.lock().unwrap() += 1;
        });

        bus.publish(Event::new("test", "1"));
        assert_eq!(*count.lock().unwrap(), 1);

        bus.clear_observer();
        bus.publish(Event::new("test", "2"));
        assert_eq!(*count.lock().unwrap(), 1); // Still 1, observer cleared
    }
}
