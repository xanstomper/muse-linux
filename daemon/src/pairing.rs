// One-step device pairing: the desktop app requests a pairing code, the user
// approves it ON the device (physical/local trust), then the app receives the
// real bearer token. No token ever appears in the UI by hand-copying.
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct Pairing {
    pub code: String,
    pub created: Instant,
    pub approved: bool,
}

pub struct PairingState {
    inner: Mutex<Option<Pairing>>,
}

const TTL: Duration = Duration::from_secs(300);

impl PairingState {
    pub fn new() -> Self {
        Self { inner: Mutex::new(None) }
    }

    /// Create a fresh 6-digit code, replacing any stale one.
    pub fn start(&self) -> String {
        let code = format!(
            "{:06}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64
                % 1_000_000
        );
        *self.inner.lock().unwrap() = Some(Pairing { code: code.clone(), created: Instant::now(), approved: false });
        code
    }

    /// The pending code, if still fresh. None otherwise.
    pub fn pending(&self) -> Option<String> {
        let g = self.inner.lock().unwrap();
        match g.as_ref() {
            Some(p) if p.created.elapsed() < TTL => Some(p.code.clone()),
            _ => None,
        }
    }

    /// Approve with a code: if it matches the pending code, mark approved and
    /// keep it for the poller to collect once.
    pub fn approve(&self, code: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.as_ref() {
            Some(p) if p.created.elapsed() < TTL && p.code == code => {
                g.as_mut().unwrap().approved = true;
                true
            }
            _ => false,
        }
    }

    /// Auto-approve the pending request unconditionally (tailnet peer = trusted owner).
    pub fn approve_all_pending(&self) {
        if let Some(g) = self.inner.lock().unwrap().as_mut() {
            g.approved = true;
        }
    }

    /// One-shot handoff for the poller: true only the FIRST time after approval.
    pub fn take_approved(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        if let Some(p) = g.as_ref() {
            if p.approved && p.created.elapsed() < TTL {
                *g = None;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_lifecycle() {
        let st = PairingState::new();
        let code = st.start();
        assert_eq!(code.len(), 6);
        // wrong code does not approve
        assert!(!st.approve("000000") || code == "000000");
        assert!(!st.approve("999999") || code == "999999");
        // right code approves once
        assert!(st.approve(&code));
        // poller gets one shot at the token handoff
        assert!(st.take_approved());
        assert!(!st.take_approved());
        assert!(st.pending().is_none());
    }
}
