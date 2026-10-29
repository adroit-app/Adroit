//! Remote work is replaced from a snapshot when the UI cannot keep up.

use adroit_protocol::Event;
use tokio::sync::mpsc;

pub(crate) enum Events {
    Local(mpsc::UnboundedReceiver<Event>),
    Remote(mpsc::Receiver<Event>),
}

impl Events {
    pub(super) fn remote() -> (mpsc::Sender<Event>, Self) {
        let (sent, events) = mpsc::channel(8);
        (sent, Self::Remote(events))
    }
    pub(crate) async fn recv(&mut self) -> Option<Event> {
        match self {
            Self::Local(events) => events.recv().await,
            Self::Remote(events) => events.recv().await,
        }
    }
    pub(crate) fn try_recv(&mut self) -> Result<Event, mpsc::error::TryRecvError> {
        match self {
            Self::Local(events) => events.try_recv(),
            Self::Remote(events) => events.try_recv(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_protocol::Version;

    #[tokio::test]
    async fn a_stalled_remote_consumer_is_bounded_and_receiving_releases_capacity() {
        let (sent, mut events) = Events::remote();
        for version in 0..8 {
            sent.try_send(Event::Welcome {
                version: Version(version),
            })
            .unwrap();
        }
        assert!(matches!(
            sent.try_send(Event::Welcome {
                version: Version(9)
            }),
            Err(mpsc::error::TrySendError::Full(_))
        ));
        assert!(matches!(
            events.recv().await,
            Some(Event::Welcome {
                version: Version(0)
            })
        ));
        sent.try_send(Event::Welcome {
            version: Version(9),
        })
        .unwrap();
        for version in 1..8 {
            assert!(
                matches!(events.try_recv(), Ok(Event::Welcome { version: got }) if got == Version(version))
            );
        }
        assert!(matches!(
            events.try_recv(),
            Ok(Event::Welcome {
                version: Version(9)
            })
        ));
        drop(sent);
        assert!(events.recv().await.is_none());
    }
}
