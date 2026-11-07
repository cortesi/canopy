//! The control protocol of an app that canopyctl hosts in a terminal.
//!
//! [`canopy_term::TermHost`] runs the app and acknowledges its input and
//! frames. A detached host serves a control socket beside the app's socket,
//! at [`control_path`], speaking one JSON request and one JSON response per
//! line.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    result,
};

use anyhow::{Context, Result, bail};
use canopy_term::TermHost;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    net::{
        UnixListener, UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
};

/// Return the control socket path beside an app's automation socket.
pub fn control_path(socket: &Path) -> PathBuf {
    let mut path = socket.as_os_str().to_owned();
    path.push(".term");
    PathBuf::from(path)
}

/// Return the log path of a detached host serving the app at `socket`.
pub fn log_path(socket: &Path) -> PathBuf {
    socket.with_extension("log")
}

/// One request to a hosted terminal.
///
/// A detached host outlives the canopyctl that started it, so a later
/// canopyctl may talk to an older host: the JSON form of a request is
/// [`WireRequest`], and serde maps it to and from this enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "WireRequest", into = "WireRequest")]
pub enum TermRequest {
    /// Stop the app and the host.
    Stop,
    /// Press keys, then read the screen if asked.
    Drive {
        /// Input in order. Each entry is a key with optional modifiers, such
        /// as `i`, `enter`, or `ctrl+c`, or `:` and text to type as it
        /// stands, one key per character. A lone `:` presses the colon key.
        keys: Vec<String>,
        /// What to read once the app has taken the input.
        read: Read,
    },
}

/// What a [`TermRequest::Drive`] reads once the app has taken its input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read {
    /// Nothing: the response holds no screen.
    None,
    /// The visible screen, once the app has drawn the input.
    Now,
    /// The visible screen, once it shows this text, for a result that
    /// arrives after the input, such as a worker's.
    Until(String),
}

impl Read {
    /// Read the screen once it shows `until`, if given, or else at once if
    /// `screen` asks for it.
    pub fn new(screen: bool, until: Option<String>) -> Self {
        match until {
            Some(text) => Self::Until(text),
            None if screen => Self::Now,
            None => Self::None,
        }
    }
}

/// The JSON form of a [`TermRequest`], as every host version reads it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct WireRequest {
    /// Input in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    keys: Vec<String>,
    /// Return the visible screen once the app has drawn the input.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    screen: bool,
    /// Before returning the screen, wait until it shows this text. It
    /// implies `screen`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    until: Option<String>,
    /// Stop the app and the host.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    stop: bool,
}

impl TryFrom<WireRequest> for TermRequest {
    type Error = String;

    fn try_from(wire: WireRequest) -> result::Result<Self, String> {
        let WireRequest {
            keys,
            screen,
            until,
            stop,
        } = wire;
        if stop {
            if !keys.is_empty() || screen || until.is_some() {
                return Err("a stop request takes no keys and reads no screen".into());
            }
            return Ok(Self::Stop);
        }
        Ok(Self::Drive {
            keys,
            read: Read::new(screen, until),
        })
    }
}

impl From<TermRequest> for WireRequest {
    fn from(request: TermRequest) -> Self {
        match request {
            TermRequest::Stop => Self {
                stop: true,
                ..Self::default()
            },
            TermRequest::Drive { keys, read } => {
                let (screen, until) = match read {
                    Read::None => (false, None),
                    Read::Now => (true, None),
                    Read::Until(text) => (true, Some(text)),
                };
                Self {
                    keys,
                    screen,
                    until,
                    stop: false,
                }
            }
        }
    }
}

/// The response to one [`TermRequest`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TermResponse {
    /// The visible screen, one string per row, when the request asked for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<Vec<String>>,
    /// Why the request failed, if it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl TermResponse {
    /// A response that reports `error`.
    fn failed(error: String) -> Self {
        Self {
            screen: None,
            error: Some(error),
        }
    }
}

/// The hosted app, as the control loop drives it.
trait Host {
    /// Press `keys`, then return what `read` asks for.
    async fn drive(&mut self, keys: &[String], read: &Read) -> Result<Option<Vec<String>>>;
    /// Wait until the app exits.
    async fn exited(&self);
    /// Stop the app and remove its socket.
    async fn stop(self) -> Result<()>;
}

/// Press `keys` on `host`, then return the screen if `read` asks for it.
pub async fn drive(
    host: &mut TermHost,
    keys: &[String],
    read: &Read,
) -> Result<Option<Vec<String>>> {
    host.keys(keys).await?;
    Ok(match read {
        Read::None => None,
        Read::Now => Some(host.screen().await?),
        Read::Until(text) => Some(host.screen_until(text).await?),
    })
}

impl Host for TermHost {
    async fn drive(&mut self, keys: &[String], read: &Read) -> Result<Option<Vec<String>>> {
        drive(self, keys, read).await
    }

    async fn exited(&self) {
        Self::exited(self).await;
    }

    async fn stop(self) -> Result<()> {
        Self::stop(self).await
    }
}

/// Bind the control socket beside the app's socket, so clients can connect
/// before [`serve_control`] runs.
pub fn bind_control(host: &TermHost) -> Result<UnixListener> {
    let path = control_path(host.socket());
    UnixListener::bind(&path).with_context(|| format!("bind {}", path.display()))
}

/// Serve requests on the control socket until a request stops the host or
/// the app exits, then stop the app and remove both sockets.
pub async fn serve_control(host: TermHost, listener: UnixListener) -> Result<()> {
    let path = control_path(host.socket());
    serve_at(host, listener, &path).await
}

/// Serve `host` on `listener`, bound at `path`, and remove `path` at the end.
async fn serve_at(host: impl Host, listener: UnixListener, path: &Path) -> Result<()> {
    let result = accept(host, &listener).await;
    drop(fs::remove_file(path));
    result
}

/// What the control loop sees next.
enum HostEvent {
    /// A client sent a request line, which may not parse.
    Request(result::Result<TermRequest, String>),
    /// The client closed its connection, or it failed.
    ConnectionLost,
    /// The app exited.
    AppExited,
}

/// One client connection's lines and its writer.
struct Connection {
    /// The request lines.
    lines: Lines<BufReader<OwnedReadHalf>>,
    /// The response stream.
    writer: OwnedWriteHalf,
}

impl Connection {
    /// Split `stream` into its lines and its writer.
    fn new(stream: UnixStream) -> Self {
        let (reader, writer) = stream.into_split();
        Self {
            lines: BufReader::new(reader).lines(),
            writer,
        }
    }

    /// Write `response` as one line.
    async fn respond(&mut self, response: &TermResponse) -> Result<()> {
        let mut encoded = serde_json::to_vec(response)?;
        encoded.push(b'\n');
        self.writer.write_all(&encoded).await?;
        Ok(())
    }
}

/// Accept control connections until one stops the host or the app exits,
/// and stop the host on every path.
async fn accept(mut host: impl Host, listener: &UnixListener) -> Result<()> {
    let result = run(&mut host, listener).await;
    let stopped = host.stop().await;
    result.and(stopped)
}

/// Serve connections one at a time, watching the app's exit throughout.
/// A failed connection is logged and the loop accepts the next one.
async fn run(host: &mut impl Host, listener: &UnixListener) -> Result<()> {
    let mut connection: Option<Connection> = None;
    loop {
        let event = match &mut connection {
            None => {
                tokio::select! {
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, _)) => connection = Some(Connection::new(stream)),
                            // A client that gave up before the accept ends
                            // only its own connection.
                            Err(error) if matches!(
                                error.kind(),
                                ErrorKind::ConnectionAborted | ErrorKind::Interrupted
                            ) => eprintln!("control connection: accept: {error}"),
                            Err(error) => return Err(error.into()),
                        }
                        continue;
                    }
                    () = host.exited() => HostEvent::AppExited,
                }
            }
            Some(open) => tokio::select! {
                line = open.lines.next_line() => match line {
                    Ok(Some(line)) => HostEvent::Request(
                        serde_json::from_str(&line).map_err(|error| format!("invalid request: {error}")),
                    ),
                    Ok(None) => HostEvent::ConnectionLost,
                    Err(error) => {
                        eprintln!("control connection: read: {error}");
                        HostEvent::ConnectionLost
                    }
                },
                () = host.exited() => HostEvent::AppExited,
            },
        };
        let (response, stop) = match event {
            HostEvent::AppExited => return Ok(()),
            HostEvent::ConnectionLost => {
                connection = None;
                continue;
            }
            HostEvent::Request(Err(error)) => (TermResponse::failed(error), false),
            HostEvent::Request(Ok(TermRequest::Stop)) => (TermResponse::default(), true),
            HostEvent::Request(Ok(TermRequest::Drive { keys, read })) => {
                let response = match host.drive(&keys, &read).await {
                    Ok(screen) => TermResponse {
                        screen,
                        error: None,
                    },
                    Err(error) => TermResponse::failed(format!("{error:#}")),
                };
                (response, false)
            }
        };
        if let Some(open) = &mut connection
            && let Err(error) = open.respond(&response).await
        {
            eprintln!("control connection: write: {error:#}");
            connection = None;
        }
        if stop {
            return Ok(());
        }
    }
}

/// Send one request to the host serving the app at `socket`.
pub async fn send(socket: &Path, request: &TermRequest) -> Result<TermResponse> {
    let path = control_path(socket);
    let stream = UnixStream::connect(&path).await.with_context(|| {
        format!(
            "connect to {} (is the app hosted with --pty?)",
            path.display()
        )
    })?;
    let (reader, mut writer) = stream.into_split();
    let mut encoded = serde_json::to_vec(request)?;
    encoded.push(b'\n');
    writer.write_all(&encoded).await?;
    let mut lines = BufReader::new(reader).lines();
    let line = lines
        .next_line()
        .await?
        .context("the host closed the connection")?;
    let response: TermResponse = serde_json::from_str(&line)?;
    if let Some(error) = &response.error {
        bail!("{error}");
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
        time::Duration,
    };

    use tokio::{
        io::AsyncReadExt,
        runtime,
        sync::{Notify, oneshot},
        task::{self, JoinHandle, LocalSet},
        time::timeout,
    };

    use super::*;

    /// Bounds a step that the fixed loop finishes at once, so a regression
    /// fails instead of hanging.
    const BOUND: Duration = Duration::from_secs(5);

    /// A host that records what the loop asks of it.
    #[derive(Clone, Default)]
    struct FakeHost {
        /// The keys of each drive, in order.
        drives: Rc<RefCell<Vec<Vec<String>>>>,
        /// A gate the next drive waits on before it returns, if set.
        gate: Rc<RefCell<Option<oneshot::Receiver<()>>>>,
        /// Signalled when a drive starts.
        driving: Rc<Notify>,
        /// Notified when the app exits.
        exit: Rc<Notify>,
        /// Whether the app has exited.
        exited: Rc<Cell<bool>>,
        /// How many times the loop stopped the host.
        stops: Rc<Cell<usize>>,
    }

    impl FakeHost {
        /// Make the app exit.
        fn exit(&self) {
            self.exited.set(true);
            self.exit.notify_waiters();
        }
    }

    impl Host for FakeHost {
        async fn drive(&mut self, keys: &[String], read: &Read) -> Result<Option<Vec<String>>> {
            self.drives.borrow_mut().push(keys.to_vec());
            self.driving.notify_one();
            let gate = self.gate.borrow_mut().take();
            if let Some(gate) = gate {
                drop(gate.await);
            }
            Ok(match read {
                Read::None => None,
                Read::Now | Read::Until(_) => Some(vec![keys.join(" ")]),
            })
        }

        async fn exited(&self) {
            loop {
                let notified = self.exit.notified();
                if self.exited.get() {
                    return;
                }
                notified.await;
            }
        }

        async fn stop(self) -> Result<()> {
            self.stops.set(self.stops.get() + 1);
            Ok(())
        }
    }

    /// A control socket in a temporary directory, served by a fake host.
    struct Served {
        /// Holds the socket's directory.
        _dir: tempfile::TempDir,
        /// The control socket.
        path: PathBuf,
        /// The host the loop drives.
        host: FakeHost,
        /// The serving task.
        task: JoinHandle<Result<()>>,
    }

    impl Served {
        /// Bind a control socket and serve a fake host on it, on the
        /// current local set.
        fn start() -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("app.sock.term");
            let listener = UnixListener::bind(&path)?;
            let host = FakeHost::default();
            let served = host.clone();
            let at = path.clone();
            let task = task::spawn_local(async move { serve_at(served, listener, &at).await });
            Ok(Self {
                _dir: dir,
                path,
                host,
                task,
            })
        }

        /// Send `request` on a new connection and return the response.
        async fn send(&self, request: &TermRequest) -> Result<TermResponse> {
            let stream = UnixStream::connect(&self.path).await?;
            let (reader, mut writer) = stream.into_split();
            let mut encoded = serde_json::to_vec(request)?;
            encoded.push(b'\n');
            writer.write_all(&encoded).await?;
            let line = BufReader::new(reader)
                .lines()
                .next_line()
                .await?
                .context("the host closed the connection")?;
            Ok(serde_json::from_str(&line)?)
        }

        /// Stop the host through the socket, and wait until the loop ends.
        async fn stop(self) -> Result<FakeHost> {
            let response = self.send(&TermRequest::Stop).await?;
            assert_eq!(response, TermResponse::default());
            timeout(BOUND, self.task).await???;
            Ok(self.host)
        }
    }

    /// A drive request that reads the screen.
    fn drive(keys: &[&str]) -> TermRequest {
        TermRequest::Drive {
            keys: keys.iter().map(|key| (*key).to_owned()).collect(),
            read: Read::Now,
        }
    }

    /// Run `test` on a local set of a current-thread runtime.
    fn local(test: impl AsyncFnOnce() -> Result<()>) -> Result<()> {
        let runtime = runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        LocalSet::new().block_on(&runtime, test())
    }

    #[test]
    fn the_wire_form_maps_to_each_request() -> Result<()> {
        let cases = [
            (r#"{"stop":true}"#, TermRequest::Stop),
            (
                r#"{"keys":["a"]}"#,
                TermRequest::Drive {
                    keys: vec!["a".into()],
                    read: Read::None,
                },
            ),
            (
                r#"{"screen":true}"#,
                TermRequest::Drive {
                    keys: Vec::new(),
                    read: Read::Now,
                },
            ),
            (
                r#"{"screen":true,"until":"x"}"#,
                TermRequest::Drive {
                    keys: Vec::new(),
                    read: Read::Until("x".into()),
                },
            ),
        ];
        for (wire, request) in cases {
            assert_eq!(
                serde_json::from_str::<TermRequest>(wire)?,
                request,
                "{wire}"
            );
            assert_eq!(serde_json::to_string(&request)?, wire);
        }
        // An older client's `until` alone implies the screen.
        assert_eq!(
            serde_json::from_str::<TermRequest>(r#"{"until":"x"}"#)?,
            TermRequest::Drive {
                keys: Vec::new(),
                read: Read::Until("x".into()),
            }
        );
        // A stop that carries keys would drop them, so it does not parse.
        assert!(serde_json::from_str::<TermRequest>(r#"{"stop":true,"keys":["a"]}"#).is_err());
        Ok(())
    }

    #[test]
    fn regression_a_request_that_is_not_utf8_leaves_the_host_serving() -> Result<()> {
        local(async || {
            let served = Served::start()?;
            let mut stream = UnixStream::connect(&served.path).await?;
            stream.write_all(b"\xff\xfe\n").await?;
            // The host drops the connection without a response.
            let mut rest = Vec::new();
            timeout(BOUND, stream.read_to_end(&mut rest)).await??;
            assert!(rest.is_empty());
            let response = served.send(&drive(&["a"])).await?;
            assert_eq!(response.screen, Some(vec!["a".to_owned()]));
            let host = served.stop().await?;
            assert_eq!(host.stops.get(), 1);
            Ok(())
        })
    }

    #[test]
    fn regression_a_client_that_leaves_before_its_response_leaves_the_host_serving() -> Result<()> {
        local(async || {
            let served = Served::start()?;
            let (release, gate) = oneshot::channel();
            *served.host.gate.borrow_mut() = Some(gate);
            let mut stream = UnixStream::connect(&served.path).await?;
            stream.write_all(br#"{"keys":["a"],"screen":true}"#).await?;
            stream.write_all(b"\n").await?;
            timeout(BOUND, served.host.driving.notified()).await?;
            drop(stream);
            release.send(()).expect("the drive waits");
            let response = served.send(&drive(&["b"])).await?;
            assert_eq!(response.screen, Some(vec!["b".to_owned()]));
            assert_eq!(*served.host.drives.borrow(), [vec!["a"], vec!["b"]]);
            let host = served.stop().await?;
            assert_eq!(host.stops.get(), 1);
            Ok(())
        })
    }

    #[test]
    fn regression_a_client_holding_a_partial_request_does_not_hide_the_exit() -> Result<()> {
        local(async || {
            let served = Served::start()?;
            let stream = UnixStream::connect(&served.path).await?;
            let (reader, mut writer) = stream.into_split();
            // A first request shows that the loop reads this connection.
            writer.write_all(b"{\"keys\":[\"a\"]}\n").await?;
            let mut lines = BufReader::new(reader).lines();
            lines.next_line().await?.context("a response")?;
            writer.write_all(br#"{"keys":"#).await?;
            served.host.exit();
            timeout(BOUND, served.task).await???;
            assert_eq!(served.host.stops.get(), 1);
            assert!(!served.path.exists());
            drop((lines, writer));
            Ok(())
        })
    }

    #[test]
    fn a_stop_stops_the_host_and_removes_the_control_socket() -> Result<()> {
        local(async || {
            let served = Served::start()?;
            let path = served.path.clone();
            assert!(path.exists());
            let host = served.stop().await?;
            assert_eq!(host.stops.get(), 1);
            assert!(!path.exists());
            Ok(())
        })
    }

    #[test]
    fn an_invalid_request_gets_an_error_and_the_connection_stays() -> Result<()> {
        local(async || {
            let served = Served::start()?;
            let stream = UnixStream::connect(&served.path).await?;
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            writer
                .write_all(b"{\"stop\":true,\"keys\":[\"a\"]}\n")
                .await?;
            let line = lines.next_line().await?.context("a response")?;
            let response: TermResponse = serde_json::from_str(&line)?;
            assert!(
                response
                    .error
                    .is_some_and(|error| error.contains("invalid request"))
            );
            writer
                .write_all(b"{\"keys\":[\"a\"],\"screen\":true}\n")
                .await?;
            let line = lines.next_line().await?.context("a response")?;
            let response: TermResponse = serde_json::from_str(&line)?;
            assert_eq!(response.screen, Some(vec!["a".to_owned()]));
            drop((lines, writer));
            served.stop().await?;
            Ok(())
        })
    }
}
