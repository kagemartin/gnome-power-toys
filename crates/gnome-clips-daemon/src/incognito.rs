use tokio::sync::watch;

/// Shared incognito state. Wraps a `watch` channel so multiple consumers
/// (clipboard loop, D-Bus interface) can read and react to changes.
pub struct IncognitoState {
    tx: watch::Sender<bool>,
}

impl IncognitoState {
    pub fn new(initial: bool) -> Self {
        let (tx, _) = watch::channel(initial);
        Self { tx }
    }

    pub fn set(&self, enabled: bool) {
        self.tx.send_replace(enabled);
    }

    pub fn get(&self) -> bool {
        *self.tx.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.tx.subscribe()
    }

    /// Returns a clone of the sender so other subsystems (e.g. the D-Bus
    /// interface) can broadcast incognito changes to all subscribers.
    pub fn sender(&self) -> watch::Sender<bool> {
        self.tx.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_changes_state() {
        let state = IncognitoState::new(false);
        assert!(!state.get());
        state.set(true);
        assert!(state.get());
        state.set(false);
        assert!(!state.get());
    }

    #[test]
    fn receiver_sees_update() {
        let state = IncognitoState::new(false);
        let mut rx = state.subscribe();
        state.set(true);
        assert!(*rx.borrow_and_update());
    }
}
