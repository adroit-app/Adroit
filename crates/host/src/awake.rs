//! Keeping the machine from idle sleep while the daemon runs a session or a
//! shell, so they go on with the display off.

/// What the assertion is called where the system lists them, as in
/// `pmset -g assertions`, `powercfg /requests` or `systemd-inhibit --list`.
pub const NAME: &str = "adroit: sessions are running";

/// Whether the machine is to be kept awake, with `running` sessions and terminals
/// whose programs still run.
pub fn wanted(running: usize, keep_awake: bool) -> bool {
    keep_awake && running > 0
}

pub fn environments_wanted(
    running: &std::collections::BTreeMap<Option<String>, usize>,
    preference: impl Fn(Option<&str>) -> bool,
) -> bool {
    running
        .iter()
        .any(|(scope, count)| wanted(*count, preference(scope.as_deref())))
}

/// Holds what `take` gives, dropping it to let go, while it is wanted.
pub struct Keeper<T> {
    take: fn() -> Option<T>,
    look: fn(&mut T),
    held: Option<T>,
}

impl<T> Keeper<T> {
    pub fn new(take: fn() -> Option<T>) -> Self {
        Self {
            take,
            look: |_| {},
            held: None,
        }
    }

    /// Have `look` look after what is held on each keep while it is wanted,
    /// to take again what of it the system let go of.
    #[cfg(any(test, all(unix, not(target_os = "macos"))))]
    pub fn watching(self, look: fn(&mut T)) -> Self {
        Self { look, ..self }
    }

    /// Hold it or let it go, as `wanted` says; a take that failed is tried
    /// again on the next.
    pub fn keep(&mut self, wanted: bool) {
        if !wanted {
            self.held = None;
            return;
        }
        match self.held.as_mut() {
            Some(held) => (self.look)(held),
            None => self.held = (self.take)(),
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

/// The system's own: a power request that keeps it from idle sleep and lets
/// the display sleep, as `powercfg /requests` lists them.
#[cfg(windows)]
mod power {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Power::{
        PowerClearRequest, PowerCreateRequest, PowerRequestSystemRequired, PowerSetRequest,
    };
    use windows_sys::Win32::System::Threading::{
        POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
    };

    /// `POWER_REQUEST_CONTEXT_VERSION`.
    const CONTEXT_VERSION: u32 = 0;

    /// Held until dropped; the system also lets it go when the process ends.
    #[derive(Debug)]
    pub struct Assertion(HANDLE);

    // SAFETY: the handle is a kernel object's, usable from any thread.
    unsafe impl Send for Assertion {}

    impl Assertion {
        pub fn take(name: &str) -> Option<Self> {
            let mut reason: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let context = REASON_CONTEXT {
                Version: CONTEXT_VERSION,
                Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
                Reason: REASON_CONTEXT_0 {
                    SimpleReasonString: reason.as_mut_ptr(),
                },
            };
            // SAFETY: the context and the string it points to outlive the
            // call, which copies what it keeps; the handle is closed on
            // failure and kept otherwise.
            unsafe {
                let request = PowerCreateRequest(&context);
                if request.is_null() || request as isize == -1 {
                    return None;
                }
                if PowerSetRequest(request, PowerRequestSystemRequired) == 0 {
                    CloseHandle(request);
                    return None;
                }
                Some(Self(request))
            }
        }
    }

    impl Drop for Assertion {
        fn drop(&mut self) {
            // SAFETY: the handle is the request this process set and has
            // not closed.
            unsafe {
                PowerClearRequest(self.0, PowerRequestSystemRequired);
                CloseHandle(self.0);
            }
        }
    }
}

/// The inhibitors that keep the system from idle sleep: logind's, which keeps
/// it from going idle, and GNOME's session's, as GNOME suspends an idle
/// machine on a timer of its own that only its session's inhibitors stop.
/// Each is held by a program for as long as it waits on the standard input
/// this process holds; one whose program is not installed or ends at once,
/// as where logind or GNOME's session does not run, is not held.
#[cfg(all(unix, not(target_os = "macos")))]
mod power {
    /// Held until dropped; a process that ends lets its ends of the pipes
    /// go, and with them the inhibitors.
    #[derive(Debug)]
    pub struct Assertion(Held);

    use std::time::{Duration, Instant};

    /// How long a program that ended after holding waits to be started
    /// again, first; each start that ends before it is seen holding doubles
    /// it, up to `RETRY_MOST`.
    const RETRY_FIRST: Duration = Duration::from_secs(5);
    const RETRY_MOST: Duration = Duration::from_secs(300);

    impl Assertion {
        pub fn take(name: &str) -> Option<Self> {
            Held::start(super::inhibitors(name), RETRY_FIRST).map(Self)
        }

        /// Start again the programs that ended after holding their
        /// inhibitors, when their wait is over.
        pub fn look(&mut self) {
            self.0.look();
        }
    }

    /// The programs that hold inhibitors. One that has ever been seen holding
    /// its inhibitor is started again whenever it has ended, as what it
    /// holds was let go or, restarting, refused it for a while, each time a
    /// while later than the last while it is not seen holding again; one
    /// that ended before it was ever seen holding is let be, as what it asks
    /// does not run here, and would refuse it again. One that cannot be
    /// started again is tried when its wait is over.
    #[derive(Debug)]
    pub(super) struct Held {
        holding: Vec<Holding>,
        retry: Duration,
    }

    #[derive(Debug)]
    struct Holding {
        program: &'static str,
        args: Vec<String>,
        child: Option<std::process::Child>,
        held_once: bool,
        refused: u32,
        next: Instant,
    }

    fn spawn(program: &str, args: &[String]) -> Option<std::process::Child> {
        std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()
    }

    impl Held {
        pub(super) fn start(
            programs: Vec<(&'static str, Vec<String>)>,
            retry: Duration,
        ) -> Option<Self> {
            let now = Instant::now();
            let holding: Vec<_> = programs
                .into_iter()
                .filter_map(|(program, args)| {
                    let child = spawn(program, &args)?;
                    Some(Holding {
                        program,
                        args,
                        child: Some(child),
                        held_once: false,
                        refused: 0,
                        next: now,
                    })
                })
                .collect();
            (!holding.is_empty()).then_some(Self { holding, retry })
        }

        pub(super) fn look(&mut self) {
            let now = Instant::now();
            let retry = self.retry;
            self.holding.retain_mut(|held| {
                let running = held
                    .child
                    .as_mut()
                    .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
                if running {
                    held.held_once = true;
                    held.refused = 0;
                    return true;
                }
                if !held.held_once {
                    return false;
                }
                if now < held.next {
                    return true;
                }
                held.child = spawn(held.program, &held.args);
                held.next = now
                    + retry
                        .saturating_mul(1 << held.refused.min(16))
                        .min(RETRY_MOST);
                held.refused = held.refused.saturating_add(1);
                true
            });
        }
    }

    impl Drop for Held {
        fn drop(&mut self) {
            let mut children: Vec<_> = self
                .holding
                .drain(..)
                .filter_map(|held| held.child)
                .collect();
            for child in &mut children {
                drop(child.stdin.take());
            }
            // Reaped apart: a program slow to end would otherwise hold up
            // the daemon's loop, which drops this.
            if !children.is_empty() {
                std::thread::spawn(move || {
                    for mut child in children {
                        let _ = child.wait();
                    }
                });
            }
        }
    }
}

/// The programs, with their arguments, that hold an inhibitor named `name`
/// while `cat` waits on their input: logind's, keeping the system from going
/// idle, and GNOME's session's, keeping it from suspending; neither keeps the
/// display from sleeping.
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
fn inhibitors(name: &str) -> Vec<(&'static str, Vec<String>)> {
    vec![
        (
            "systemd-inhibit",
            vec![
                "--what=idle".into(),
                "--who=Adroit".into(),
                format!("--why={name}"),
                "--mode=block".into(),
                "cat".into(),
            ],
        ),
        // Each option and its value apart: gnome-session-inhibit takes
        // `--reason=…` for the command to run.
        (
            "gnome-session-inhibit",
            vec![
                "--app-id".into(),
                "Adroit".into(),
                "--reason".into(),
                name.into(),
                "--inhibit".into(),
                "suspend".into(),
                "cat".into(),
            ],
        ),
    ]
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

    #[cfg(windows)]
    #[test]
    fn windows_takes_a_request_to_stay_awake_and_lets_it_go() {
        let held = Assertion::take("adroit test").expect("taken");
        drop(held);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn logind_lists_the_inhibitor_by_its_name_while_it_is_held() {
        fn listed(name: &str) -> Option<bool> {
            let out = std::process::Command::new("systemd-inhibit")
                .arg("--list")
                .output()
                .ok()
                .filter(|out| out.status.success())?;
            Some(String::from_utf8_lossy(&out.stdout).contains(name))
        }
        let name = format!("adroit test {}", std::process::id());
        if listed(&name).is_none() {
            eprintln!("logind does not run here");
            return;
        }
        let held = Assertion::take(&name).expect("taken");
        let shown = (0..50).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            listed(&name) == Some(true)
        });
        assert!(shown);
        drop(held);
        let gone = (0..50).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            listed(&name) == Some(false)
        });
        assert!(gone);
    }

    #[test]
    fn linux_is_kept_from_idling_and_from_gnome_s_suspend_but_not_from_sleeping_its_display() {
        let asked: std::collections::HashMap<_, _> = inhibitors("adroit: sessions are running")
            .into_iter()
            .collect();
        let logind = &asked["systemd-inhibit"];
        assert!(logind.contains(&"--what=idle".to_string()));
        assert!(logind.contains(&"--why=adroit: sessions are running".to_string()));
        let gnome = &asked["gnome-session-inhibit"];
        let given = |option: &str| {
            gnome
                .windows(2)
                .find(|pair| pair[0] == option)
                .map(|pair| pair[1].as_str())
        };
        assert_eq!(given("--inhibit"), Some("suspend"));
        assert_eq!(given("--reason"), Some("adroit: sessions are running"));
        assert!(!gnome.iter().any(|a| a.starts_with("--") && a.contains('=')));
        for args in asked.values() {
            assert_eq!(args.last().map(String::as_str), Some("cat"));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn an_inhibitor_let_go_is_taken_again_and_one_refused_at_once_is_let_be() {
        let dir = tempfile::tempdir().unwrap();
        let file = |name: &str| dir.path().join(name);
        let shell = |script: String| ("/bin/sh", vec!["-c".to_string(), script]);
        let count = |path: &std::path::Path| {
            std::fs::read_to_string(path)
                .unwrap_or_default()
                .lines()
                .count()
        };
        // A stand-in that holds its inhibitor until `stop` is there.
        let holds_until = |stop: &std::path::Path| {
            format!("while [ ! -e '{}' ]; do sleep 0.02; done", stop.display())
        };
        // Waits until `ran` has run, and a moment more for it to end: a
        // program the first look finds still running counts as holding, as
        // one does when the work host first looks, seconds after it starts.
        let ended = |ran: &std::path::Path| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while count(ran) == 0 && std::time::Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        };
        // Looks again and again until `done` holds, at most for a while.
        let look_until = |held: &mut power::Held, done: &dyn Fn() -> bool| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !done() && std::time::Instant::now() < until {
                held.look();
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            done()
        };
        // Looks again and again for `long`.
        let look_for = |held: &mut power::Held, long: std::time::Duration| {
            let until = std::time::Instant::now() + long;
            while std::time::Instant::now() < until {
                held.look();
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        let retry = std::time::Duration::from_millis(50);

        let (runs, refused, stop) = (file("runs"), file("refused"), file("stop"));
        let mut held = power::Held::start(
            vec![
                shell(format!("echo >> '{}'; exit 1", refused.display())),
                shell(format!(
                    "echo >> '{}'; {}",
                    runs.display(),
                    holds_until(&stop)
                )),
                shell("cat".into()),
            ],
            retry,
        )
        .unwrap();
        ended(&refused);
        held.look();
        std::fs::write(&stop, "").unwrap();
        assert!(
            look_until(&mut held, &|| count(&runs) >= 2),
            "the one seen holding ran again"
        );
        assert_eq!(count(&refused), 1, "the one never seen holding ran once");

        let alone = file("alone");
        let mut refused_alone = power::Held::start(
            vec![shell(format!("echo >> '{}'; exit 1", alone.display()))],
            retry,
        )
        .unwrap();
        ended(&alone);
        look_for(&mut refused_alone, std::time::Duration::from_millis(500));
        assert_eq!(count(&alone), 1, "the one refused alone is let be");

        // Held once, then refused for a while, as while logind restarts.
        let (tries, again) = (file("tries"), file("again"));
        let mut comes_back = power::Held::start(
            vec![shell(format!(
                "echo >> '{0}'; case $(wc -l < '{0}') in 1) {1} ;; 2) exit 1 ;; *) cat ;; esac",
                tries.display(),
                holds_until(&again)
            ))],
            retry,
        )
        .unwrap();
        comes_back.look();
        std::fs::write(&again, "").unwrap();
        assert!(
            look_until(&mut comes_back, &|| count(&tries) >= 3),
            "one held once is tried again after a refusal"
        );

        // Held once, then refused for good, as once GNOME's session is gone:
        // tried again ever later rather than on every look.
        let (gave_up, end) = (file("gave_up"), file("end"));
        let mut refuses_for_good = power::Held::start(
            vec![shell(format!(
                "echo >> '{0}'; [ \"$(wc -l < '{0}')\" -gt 1 ] && exit 1; {1}",
                gave_up.display(),
                holds_until(&end)
            ))],
            retry,
        )
        .unwrap();
        refuses_for_good.look();
        std::fs::write(&end, "").unwrap();
        look_for(
            &mut refuses_for_good,
            std::time::Duration::from_millis(1500),
        );
        let tried = count(&gave_up);
        assert!(
            (3..=15).contains(&tried),
            "tried {tried} times in 1.5s, rather than on each of ~75 looks"
        );
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn gnome_s_session_holds_the_inhibitor_while_it_is_held() {
        fn inhibited() -> Option<bool> {
            let out = std::process::Command::new("gdbus")
                .args([
                    "call",
                    "--session",
                    "--dest",
                    "org.gnome.SessionManager",
                    "--object-path",
                    "/org/gnome/SessionManager",
                    "--method",
                    "org.gnome.SessionManager.IsInhibited",
                    "4",
                ])
                .output()
                .ok()
                .filter(|out| out.status.success())?;
            Some(String::from_utf8_lossy(&out.stdout).contains("true"))
        }
        if inhibited() != Some(false) {
            eprintln!("no GNOME session runs here, or something else inhibits it");
            return;
        }
        let held = Assertion::take("adroit test").expect("taken");
        let shown = (0..50).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            inhibited() == Some(true)
        });
        assert!(shown);
        drop(held);
        let gone = (0..50).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            inhibited() == Some(false)
        });
        assert!(gone);
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

    #[test]
    fn what_is_held_is_looked_after_on_each_keep_and_not_taken_again() {
        static TAKEN: AtomicUsize = AtomicUsize::new(0);
        fn looks() -> Option<usize> {
            TAKEN.fetch_add(1, Ordering::SeqCst);
            Some(0)
        }
        let mut keeper = Keeper::new(looks).watching(|held| *held += 1);
        keeper.keep(true);
        assert_eq!(keeper.held, Some(0));
        keeper.keep(true);
        keeper.keep(true);
        assert_eq!(keeper.held, Some(2));
        assert_eq!(TAKEN.load(Ordering::SeqCst), 1);
        keeper.keep(false);
        keeper.keep(true);
        assert_eq!(keeper.held, Some(0));
        assert_eq!(TAKEN.load(Ordering::SeqCst), 2);
    }
}

#[cfg(test)]
mod environment_tests {
    use super::*;
    #[test]
    fn only_environments_with_running_work_contribute_their_awake_preference() {
        let work_only = [(Some("work".to_string()), 1)].into_iter().collect();
        assert!(!environments_wanted(&work_only, |scope| scope.is_none()));
        let mixed = [
            (Some("work".to_string()), 1),
            (Some("personal".to_string()), 1),
        ]
        .into_iter()
        .collect();
        assert!(environments_wanted(&mixed, |scope| scope == Some("personal")));
        let ended = [(Some("personal".to_string()), 0)].into_iter().collect();
        assert!(!environments_wanted(&ended, |_| true));
    }
}
