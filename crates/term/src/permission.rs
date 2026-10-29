//! Revocable input permission shared by all panes on one connection.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug)]
pub struct InputPermission(Arc<AtomicU64>);

impl PartialEq for InputPermission {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for InputPermission {}

impl Default for InputPermission {
    fn default() -> Self {
        Self(Arc::new(AtomicU64::new(1)))
    }
}

impl InputPermission {
    pub fn closed() -> Self {
        Self(Arc::new(AtomicU64::new(0)))
    }
    pub fn set(&self, enabled: bool) {
        let _ = self
            .0
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |epoch| {
                if epoch >= u64::MAX - 2 {
                    return Some(u64::MAX - 1);
                }
                (epoch % 2 != u64::from(enabled)).then_some(epoch + 1)
            });
    }
    pub fn ticket(&self) -> Option<u64> {
        let epoch = self.0.load(Ordering::SeqCst);
        (epoch % 2 == 1).then_some(epoch)
    }
    pub fn allows(&self, ticket: u64) -> bool {
        self.ticket() == Some(ticket)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revoking_and_reopening_permission_never_makes_a_queued_input_current_again() {
        let permission = InputPermission::closed();
        assert_eq!(permission.ticket(), None);
        permission.set(true);
        let old = permission.ticket().unwrap();
        assert!(permission.allows(old));
        permission.set(false);
        assert!(!permission.allows(old));
        permission.set(true);
        assert!(!permission.allows(old));
        assert!(permission.allows(permission.ticket().unwrap()));
    }
}
