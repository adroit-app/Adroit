//! Bounded work requests, with durable identity before any mutation write.

use crate::journal::{Intent, Journal};
use adroit_protocol::client::Connection;
use adroit_protocol::{
    Answer, Ask, Event, MutationStamp, MutationState, RemoteInfo, Request, SPOKEN, agree,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const REPLY: Duration = Duration::from_secs(30);

pub struct Work {
    socket: PathBuf,
    info: RemoteInfo,
    journal: Arc<Journal>,
    pending: tokio::sync::Semaphore,
}

pub struct Outcome {
    pub intent: Intent,
    pub state: MutationState,
    pub problem: Option<String>,
}

impl Work {
    pub(crate) fn new(
        socket: PathBuf,
        info: RemoteInfo,
        journal: Arc<Journal>,
    ) -> Result<Self, String> {
        info.validate()?;
        if journal.instance() != info.instance {
            return Err("mutation journal belongs to another remote instance".into());
        }
        Ok(Self {
            socket,
            info,
            journal,
            pending: tokio::sync::Semaphore::new(8),
        })
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    async fn connection(&self) -> Result<Connection, String> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut link = Connection::open(&self.socket)
                .await
                .map_err(|e| e.to_string())?;
            if link.hello.endpoint.as_ref() != Some(&self.info.work)
                || link.hello.terminal.as_ref() != Some(&self.info.terminal)
                || agree(SPOKEN, &link.hello.speaks).is_none()
            {
                return Err("remote work identity changed before the request".into());
            }
            link.requests
                .send(&Request::Hello {
                    versions: SPOKEN.to_vec(),
                })
                .await
                .map_err(|e| e.to_string())?;
            loop {
                match link.events.next().await.map_err(|e| e.to_string())? {
                    Some(Event::Welcome { .. }) => return Ok(link),
                    Some(Event::Refused { .. }) | None => {
                        return Err("remote work host refused the connection".into());
                    }
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| "remote work greeting timed out".to_string())?
    }

    async fn ask(link: &mut Connection, ask: Ask) -> Result<Answer, String> {
        tokio::time::timeout(REPLY, async {
            link.requests
                .send(&Request::Ask {
                    asked: 1,
                    ask: Box::new(ask),
                })
                .await
                .map_err(|e| e.to_string())?;
            loop {
                match link.events.next().await.map_err(|e| e.to_string())? {
                    Some(Event::Answer { asked: 1, answer }) => return Ok(answer),
                    None => return Err("remote work host went away before its answer".into()),
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| "remote work reply timed out".to_string())?
    }

    pub async fn read(&self, ask: Ask) -> Result<Answer, String> {
        if ask.mutates() || matches!(ask, Ask::Apply { .. }) {
            return Err("work mutations must be journaled before sending".into());
        }
        let _permit = self
            .pending
            .try_acquire()
            .map_err(|_| "too many remote requests are pending".to_string())?;
        Self::ask(&mut self.connection().await?, ask).await
    }

    /// Errors before admission are returned as errors; after persistence, a
    /// missing reply is an outcome to resolve, never a reason to send again.
    pub async fn mutate(&self, request: Request) -> Result<Outcome, String> {
        self.change(request, None).await
    }

    pub async fn mutate_prompt(
        &self,
        request: Request,
        prompt: crate::journal::PromptContext,
    ) -> Result<Outcome, String> {
        if !matches!(&request, Request::Ask { ask, .. } if matches!(ask.as_ref(), Ask::SubmitPrompt { .. } | Ask::StartSession(_)))
        {
            return Err("prompt context requires a prompt or session-start request".into());
        }
        self.change(request, Some(prompt)).await
    }

    async fn change(
        &self,
        request: Request,
        prompt: Option<crate::journal::PromptContext>,
    ) -> Result<Outcome, String> {
        if !request.mutates() {
            return Err("only work mutations can be applied".into());
        }
        let _permit = self
            .pending
            .try_acquire()
            .map_err(|_| "too many remote changes are pending".to_string())?;
        let mut link = self.connection().await?;
        let epoch = match Self::ask(&mut link, Ask::MutationEpoch).await? {
            Answer::MutationEpoch { epoch } => epoch,
            _ => return Err("remote work host supplied no mutation epoch".into()),
        };
        let mut intent = Intent::new(&self.info, epoch, request)?;
        intent.prompt = prompt;
        self.journal.prepare(&intent)?;
        let result = tokio::time::timeout(REPLY, async {
            let mut answer = Self::ask(&mut link, intent.apply()).await?;
            loop {
                match answer {
                    Answer::Mutation {
                        status: MutationState::Accepted,
                    } => {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        answer = Self::ask(&mut link, intent.query()).await?;
                    }
                    Answer::Mutation { status } => return Ok(status),
                    _ => return Err("remote work host supplied no mutation outcome".to_string()),
                }
            }
        })
        .await
        .unwrap_or_else(|_| Err("remote mutation outcome timed out".into()));
        let (state, mut problem) = match result {
            Ok(state) => (state, None),
            Err(text) => (MutationState::Unknown, Some(text)),
        };
        if let Err(text) = self.journal.finish(&intent.stamp, &state) {
            problem = Some(text);
        }
        Ok(Outcome {
            intent,
            state,
            problem,
        })
    }

    /// Recovery only queries the original ID. Even an intent saved before a
    /// write that never happened is left unknown rather than applied again.
    pub async fn recover(&self, stamp: &MutationStamp) -> Result<Outcome, String> {
        let record = self
            .journal
            .records()?
            .into_iter()
            .find(|r| r.intent.stamp == *stamp)
            .ok_or("remote mutation is absent from the journal")?;
        if let Some(state) = record.outcome {
            return Ok(Outcome {
                intent: record.intent,
                state,
                problem: None,
            });
        }
        if stamp.work != self.info.work.generation
            || stamp.terminal != self.info.terminal.generation
        {
            return Ok(Outcome {
                intent: record.intent,
                state: MutationState::Unknown,
                problem: Some(
                    "remote host incarnation changed before the outcome was confirmed".into(),
                ),
            });
        }
        let _permit = self
            .pending
            .try_acquire()
            .map_err(|_| "too many remote requests are pending".to_string())?;
        let mut link = self.connection().await?;
        let state = match Self::ask(&mut link, record.intent.query()).await? {
            Answer::Mutation { status } => status,
            _ => return Err("remote work host supplied no mutation outcome".into()),
        };
        self.journal.finish(stamp, &state)?;
        Ok(Outcome {
            intent: record.intent,
            state,
            problem: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adroit_protocol::{Hello, RemoteEndpoint, SPOKEN, encode};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    fn info() -> RemoteInfo {
        let endpoint = |generation: &str| RemoteEndpoint {
            instance: "a".repeat(32),
            generation: generation.repeat(32),
            pid: 1,
            build: "test".into(),
            protocols: vec![1],
            socket: format!(
                "/tmp/adroit-r-501/{}/{}.sock",
                "a".repeat(32),
                generation.repeat(32)
            )
            .into(),
        };
        RemoteInfo {
            instance: "a".repeat(32),
            work: endpoint("b"),
            terminal: endpoint("c"),
        }
    }

    #[tokio::test]
    async fn two_lost_replies_and_two_client_restarts_never_apply_the_intent_again() {
        let dir = tempfile::Builder::new()
            .prefix("adroit-w-")
            .tempdir_in("/tmp")
            .unwrap();
        let socket = dir.path().join("work.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let info = info();
        let server_info = info.clone();
        let effects = Arc::new(AtomicUsize::new(0));
        let queries = Arc::new(AtomicUsize::new(0));
        let (effect_count, query_count) = (effects.clone(), queries.clone());
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                stream
                    .write_all(
                        encode(&Hello {
                            speaks: SPOKEN.to_vec(),
                            build: "test".into(),
                            endpoint: Some(server_info.work.clone()),
                            terminal: Some(server_info.terminal.clone()),
                            mutation_epoch: Some(1),
                        })
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                let (read, mut write) = stream.into_split();
                let mut read = BufReader::new(read);
                assert!(matches!(
                    adroit_protocol::decode::<Request>(
                        &adroit_protocol::client::read_line(&mut read)
                            .await
                            .unwrap()
                            .unwrap()
                    )
                    .unwrap(),
                    Request::Hello { .. }
                ));
                write
                    .write_all(encode(&Event::Welcome { version: SPOKEN[0] }).as_bytes())
                    .await
                    .unwrap();
                while let Some(line) = adroit_protocol::client::read_line(&mut read).await.unwrap()
                {
                    let Request::Ask { asked, ask } = adroit_protocol::decode(&line).unwrap()
                    else {
                        panic!("expected ask");
                    };
                    let answer = match *ask {
                        Ask::MutationEpoch => Answer::MutationEpoch { epoch: 1 },
                        Ask::Apply { .. } => {
                            effect_count.fetch_add(1, Ordering::SeqCst);
                            break;
                        }
                        Ask::MutationStatus { .. } => {
                            if query_count.fetch_add(1, Ordering::SeqCst) == 0 {
                                break;
                            }
                            Answer::Mutation {
                                status: MutationState::Result {
                                    answer: Box::new(Answer::Done),
                                },
                            }
                        }
                        _ => panic!("unexpected ask"),
                    };
                    write
                        .write_all(encode(&Event::Answer { asked, answer }).as_bytes())
                        .await
                        .unwrap();
                }
            }
        });
        let journal =
            || Arc::new(Journal::open(&dir.path().join("journal"), &info.instance).unwrap());
        let client = Work::new(socket.clone(), info.clone(), journal()).unwrap();
        let prompt = crate::journal::PromptContext {
            target: crate::journal::PromptTarget::Session {
                id: "session".into(),
            },
            text: "日本語の依頼".into(),
            images: vec![],
        };
        let outcome = client
            .mutate_prompt(
                Request::Ask {
                    asked: 1,
                    ask: Box::new(Ask::SubmitPrompt {
                        session: "session".into(),
                        agent: adroit_model::Agent::Codex,
                        text: prompt.text.clone(),
                        images: vec![],
                    }),
                },
                prompt.clone(),
            )
            .await
            .unwrap();
        assert_eq!(outcome.state, MutationState::Unknown);
        let stamp = outcome.intent.stamp;
        drop(client);
        let client = Work::new(socket.clone(), info.clone(), journal()).unwrap();
        assert!(client.recover(&stamp).await.is_err());
        drop(client);
        let client = Work::new(socket, info.clone(), journal()).unwrap();
        let outcome = client.recover(&stamp).await.unwrap();
        assert_eq!(outcome.intent.prompt, Some(prompt));
        assert_eq!(
            outcome.state,
            MutationState::Result {
                answer: Box::new(Answer::Done)
            }
        );
        assert_eq!(effects.load(Ordering::SeqCst), 1);
        assert_eq!(queries.load(Ordering::SeqCst), 2);
        assert_eq!(client.journal.records().unwrap().len(), 1);
        client.journal.acknowledge(&stamp).unwrap();
        assert!(client.journal.records().unwrap().is_empty());
        server.abort();
        let _ = server.await;
    }
}
