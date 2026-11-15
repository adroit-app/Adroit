use tracing::{Dispatch, Level, Metadata};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer, Registry, filter::filter_fn};

/// The environment variable that chooses what is logged.
pub const VARIABLE: &str = "ADROIT_LOG";

/// Sets up the subscriber every event of this process is written by: to
/// standard error, filtered as `ADROIT_LOG` asks.
pub fn init() {
    use std::io::IsTerminal;
    let asked = std::env::var(VARIABLE).ok();
    let ansi = std::io::stderr().is_terminal();
    let (dispatch, refused) = dispatch(asked.as_deref(), std::io::stderr, ansi);
    if tracing::dispatcher::set_global_default(dispatch).is_ok()
        && let Some(refused) = refused
    {
        tracing::warn!(variable = VARIABLE, %refused, "ignoring an unreadable filter");
    }
}

/// The subscriber writing to `writer` what `asked` lets through, and why
/// `asked` was not taken when it could not be read.
fn dispatch<W>(asked: Option<&str>, writer: W, ansi: bool) -> (Dispatch, Option<String>)
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(ansi);
    let (filtered, refused) = match asked.map(EnvFilter::try_new) {
        Some(Ok(filter)) => (layer.with_filter(filter).boxed(), None),
        Some(Err(error)) => (
            layer.with_filter(filter_fn(by_default)).boxed(),
            Some(error.to_string()),
        ),
        None => (layer.with_filter(filter_fn(by_default)).boxed(), None),
    };
    (Dispatch::new(Registry::default().with(filtered)), refused)
}

/// Whether an event is logged when nothing is asked: Adroit's own from info
/// up, the libraries' warnings and errors.
///
/// Matched by the target's prefix rather than by directives naming each
/// crate, so a crate added to the workspace logs without being listed.
fn by_default(metadata: &Metadata<'_>) -> bool {
    let target = metadata.target();
    let own = target == "adroit" || target.starts_with("adroit::") || target.starts_with("adroit_");
    let least = if own { Level::INFO } else { Level::WARN };
    *metadata.level() <= least
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Written(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Written {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Written {
        type Writer = Written;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// What is written when `asked` filters the events `emit` sends.
    fn written(asked: Option<&str>, emit: impl FnOnce()) -> (String, Option<String>) {
        let out = Written::default();
        let (dispatch, refused) = dispatch(asked, out.clone(), false);
        tracing::dispatcher::with_default(&dispatch, emit);
        let text = String::from_utf8(out.0.lock().unwrap().clone()).unwrap();
        (text, refused)
    }

    fn every_level() {
        tracing::info!(target: "adroit_host::gateway", "own info");
        tracing::debug!(target: "adroit_host::gateway", "own debug");
        tracing::info!(target: "adroit", "program info");
        tracing::warn!(target: "wry", "library warning");
        tracing::info!(target: "wry", "library info");
        tracing::info!(target: "adroitish", "lookalike info");
    }

    #[test]
    fn by_default_adroit_s_own_events_are_written_from_info_and_libraries_from_warn() {
        let (text, refused) = written(None, every_level);
        assert!(text.contains("own info"), "{text}");
        assert!(text.contains("program info"), "{text}");
        assert!(text.contains("library warning"), "{text}");
        assert!(!text.contains("own debug"), "{text}");
        assert!(!text.contains("library info"), "{text}");
        assert!(!text.contains("lookalike info"), "{text}");
        assert_eq!(refused, None);
    }

    #[test]
    fn adroit_log_chooses_what_is_written_as_tracing_directives() {
        let (text, _) = written(Some("adroit_host=debug,error"), every_level);
        assert!(text.contains("own debug"), "{text}");
        assert!(!text.contains("program info"), "{text}");
        assert!(!text.contains("library warning"), "{text}");
    }

    #[test]
    fn an_unreadable_adroit_log_is_refused_and_the_default_is_used() {
        let (text, refused) = written(Some("adroit_host=loud"), every_level);
        assert!(refused.is_some());
        assert!(text.contains("own info"), "{text}");
        assert!(!text.contains("own debug"), "{text}");
    }

    #[test]
    fn events_are_written_with_their_level_and_target() {
        let (text, _) = written(None, || {
            tracing::warn!(target: "adroit_host::gateway", code = 3, "said");
        });
        assert!(text.contains("WARN"), "{text}");
        assert!(text.contains("adroit_host::gateway"), "{text}");
        assert!(text.contains("code=3"), "{text}");
    }
}
