//! Keeping the Mac from idle sleep while the daemon runs a session or a
//! shell, so they go on with the display off.

/// What the assertion is called where the system lists them, as in
/// `pmset -g assertions`.
pub const NAME: &str = "adroit: sessions are running";

/// Whether the Mac is to be kept awake, with `running` sessions and shells
/// whose programs still run.
pub fn wanted(running: usize, keep_awake: bool) -> bool {
    keep_awake && running > 0
}

/// Holds what `take` gives, dropping it to let go, while it is wanted.
pub struct Keeper<T> {
    take: fn() -> Option<T>,
    held: Option<T>,
}

impl<T> Keeper<T> {
    pub fn new(take: fn() -> Option<T>) -> Self {
        Self { take, held: None }
    }

    /// Hold it or let it go, as `wanted` says; a take that failed is tried
    /// again on the next.
    pub fn keep(&mut self, wanted: bool) {
        if !wanted {
            self.held = None;
        } else if self.held.is_none() {
            self.held = (self.take)();
        }
    }
}

pub use power::Assertion;

/// The system's own: an assertion that keeps it from idle sleep and lets
/// the display sleep, as `caffeinate -i` takes.
#[cfg(target_os = "macos")]
mod power {
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOPMAssertionCreateWithName(
            kind: CFStringRef,
            level: u32,
            name: CFStringRef,
            id: *mut u32,
        ) -> i32;
        fn IOPMAssertionRelease(id: u32) -> i32;
    }

    /// `kIOPMAssertionLevelOn`.
    const LEVEL_ON: u32 = 255;

    /// Held until dropped; the system also lets it go when the process ends.
    #[derive(Debug)]
    pub struct Assertion(u32);

    impl Assertion {
        pub fn take(name: &str) -> Option<Self> {
            let kind = CFString::from_static_string("PreventUserIdleSystemSleep");
            let name = CFString::new(name);
            let mut id = 0;
            // SAFETY: both strings outlive the call, which copies what it
            // keeps, and `id` is a valid place for the id it writes.
            let got = unsafe {
                IOPMAssertionCreateWithName(
                    kind.as_concrete_TypeRef(),
                    LEVEL_ON,
                    name.as_concrete_TypeRef(),
                    &mut id,
                )
            };
            (got == 0).then_some(Self(id))
        }
    }

    impl Drop for Assertion {
        fn drop(&mut self) {
            // SAFETY: the id is one this process took and has not let go.
            unsafe {
                IOPMAssertionRelease(self.0);
            }
        }
    }
}

/// Elsewhere adroit keeps nothing awake.
#[cfg(not(target_os = "macos"))]
mod power {
    #[derive(Debug)]
    pub enum Assertion {}

    impl Assertion {
        pub fn take(_name: &str) -> Option<Self> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

    #[cfg(target_os = "macos")]
    #[test]
    fn the_system_lists_the_assertion_by_its_name_while_it_is_held() {
        fn listed(name: &str) -> bool {
            let out = std::process::Command::new("pmset")
                .args(["-g", "assertions"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).contains(name)
        }
        let name = format!("adroit test {}", std::process::id());
        let held = Assertion::take(&name).expect("taken");
        assert!(listed(&name));
        drop(held);
        assert!(!listed(&name));
    }

    #[test]
    fn the_mac_is_kept_awake_only_while_something_runs_and_the_setting_is_on() {
        let table = [
            (0, true, false),
            (1, true, true),
            (3, true, true),
            (0, false, false),
            (1, false, false),
        ];
        for (running, keep_awake, want) in table {
            assert_eq!(
                wanted(running, keep_awake),
                want,
                "running {running}, keep_awake {keep_awake}"
            );
        }
    }

    static TAKEN: AtomicUsize = AtomicUsize::new(0);
    static HELD: AtomicIsize = AtomicIsize::new(0);

    struct Counted;

    impl Drop for Counted {
        fn drop(&mut self) {
            HELD.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn counted() -> Option<Counted> {
        TAKEN.fetch_add(1, Ordering::SeqCst);
        HELD.fetch_add(1, Ordering::SeqCst);
        Some(Counted)
    }

    #[test]
    fn it_is_taken_once_while_wanted_and_let_go_once_no_longer() {
        let mut keeper = Keeper::new(counted);
        keeper.keep(false);
        assert!(keeper.held.is_none());
        keeper.keep(true);
        keeper.keep(true);
        keeper.keep(true);
        assert!(keeper.held.is_some());
        assert_eq!(TAKEN.load(Ordering::SeqCst), 1);
        assert_eq!(HELD.load(Ordering::SeqCst), 1);
        keeper.keep(false);
        assert!(keeper.held.is_none());
        assert_eq!(HELD.load(Ordering::SeqCst), 0);
        keeper.keep(true);
        assert_eq!(TAKEN.load(Ordering::SeqCst), 2);
        drop(keeper);
        assert_eq!(HELD.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_take_that_failed_is_tried_again_while_still_wanted() {
        static TRIED: AtomicUsize = AtomicUsize::new(0);
        fn refused() -> Option<()> {
            TRIED.fetch_add(1, Ordering::SeqCst);
            None
        }
        let mut keeper = Keeper::new(refused);
        keeper.keep(true);
        keeper.keep(true);
        assert!(keeper.held.is_none());
        assert_eq!(TRIED.load(Ordering::SeqCst), 2);
    }
}
