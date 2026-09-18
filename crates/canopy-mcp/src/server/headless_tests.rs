//! Blocking-worker ownership, responsiveness, and cancellation over real MCP.

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use canopy::{CanopyBuilder, Widget, Work, derive_commands, geom::Size};
use tmcp::schema::ClientRequest;
use tokio::{
    io::{duplex, split},
    sync::mpsc as async_mpsc,
    task::JoinHandle,
    time::timeout,
};

use super::*;
use crate::metadata::test_app_factory;

const DEADLINE: Duration = Duration::from_secs(3);

struct Probe {
    id: usize,
    owner: thread::ThreadId,
    started: async_mpsc::UnboundedSender<usize>,
    dropped: async_mpsc::UnboundedSender<usize>,
}

#[derive_commands]
impl Probe {
    #[command]
    fn started(&self) {
        assert_eq!(thread::current().id(), self.owner);
        let _closed = self.started.send(self.id);
    }
}

impl Widget for Probe {}

impl Drop for Probe {
    fn drop(&mut self) {
        assert_eq!(thread::current().id(), self.owner);
        let _closed = self.dropped.send(self.id);
    }
}

struct Fixture {
    server: CanopyMcpServer,
    built: Arc<AtomicUsize>,
    started: async_mpsc::UnboundedReceiver<usize>,
    dropped: async_mpsc::UnboundedReceiver<usize>,
}

fn fixture(workers: usize) -> Fixture {
    let caller = thread::current().id();
    let built = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&built);
    let (started_tx, started) = async_mpsc::unbounded_channel();
    let (dropped_tx, dropped) = async_mpsc::unbounded_channel();
    let factory = test_app_factory(move || {
        let owner = thread::current().id();
        assert_ne!(owner, caller, "construction must leave the async caller");
        let probe = Probe {
            id: count.fetch_add(1, Ordering::SeqCst),
            owner,
            started: started_tx.clone(),
            dropped: dropped_tx.clone(),
        };
        Ok(CanopyBuilder::new()
            .configure(|canopy| {
                canopy.add_commands::<Probe>()?;
                canopy.register_startup_script(
                    "startup",
                    "function setup() canopy.set_mode('ready') end",
                )
            })
            .bindings("bindings", "canopy.set_mode('building')")
            .assemble(move |canopy| canopy.replace_root(probe).map(|_| ()))
            .build()?)
    });
    let mut server = CanopyMcpServer::new(factory);
    server.workers = Arc::new(Semaphore::new(workers));
    Fixture {
        server,
        built,
        started,
        dropped,
    }
}

async fn connect(
    server: impl tmcp::ServerHandler + Clone + 'static,
) -> (tmcp::Client<()>, JoinHandle<tmcp::Result<()>>) {
    let (client_io, server_io) = duplex(64 * 1024);
    let (reader, writer) = split(server_io);
    let transport = tokio::spawn(Server::new(move || server.clone()).serve_stream(reader, writer));
    let (reader, writer) = split(client_io);
    let mut client = tmcp::Client::new("headless-test", "1").with_request_timeout(DEADLINE);
    client
        .connect_stream(reader, writer)
        .await
        .expect("connect MCP");
    (client, transport)
}

fn request(source: &str) -> ClientRequest {
    ClientRequest::call_tool(
        "script_eval",
        Some(
            tmcp::Arguments::from_struct(ScriptEvalRequest {
                timeout_ms: Some(10_000),
                ..ScriptEvalRequest::new(source)
            })
            .expect("request arguments"),
        ),
        None,
    )
}

async fn receive(receiver: &mut async_mpsc::UnboundedReceiver<usize>) -> usize {
    timeout(DEADLINE, receiver.recv())
        .await
        .expect("worker makes progress")
        .expect("worker signal")
}

async fn wait_for_drop(receiver: &mut async_mpsc::UnboundedReceiver<usize>, id: usize) {
    timeout(DEADLINE, async {
        while receiver.recv().await.expect("worker signal") != id {}
    })
    .await
    .expect("cancelled application is dropped");
}

async fn close(mut client: tmcp::Client<()>, transport: JoinHandle<tmcp::Result<()>>) {
    client.disconnect().await;
    timeout(DEADLINE, transport)
        .await
        .expect("server drains")
        .expect("server joins")
        .expect("server exits");
}

async fn responsive_transport() {
    let mut fixture = fixture(2);
    let (client, transport) = connect(fixture.server.clone()).await;
    for source in [
        "probe.started(); canopy.wait_for(function() return false end)",
        "probe.started(); while true do end",
    ] {
        let (request_id, pending) = client
            .request::<CallToolResult>(request(source))
            .await
            .expect("start eval");
        let id = receive(&mut fixture.started).await;
        timeout(DEADLINE, client.ping())
            .await
            .expect("ping remains responsive")
            .expect("ping");
        let quick = client
            .call_tool("script_eval", ScriptEvalRequest::new("return 42"))
            .await
            .expect("concurrent evaluation");
        assert_eq!(quick.structured_content.expect("payload")["value"], 42);
        client.cancel(request_id).await.expect("cancel through MCP");
        drop(pending);
        wait_for_drop(&mut fixture.dropped, id).await;
        let all_workers = timeout(DEADLINE, fixture.server.workers.acquire_many(2))
            .await
            .expect("workers released")
            .expect("semaphore open");
        drop(all_workers);
    }
    close(client, transport).await;
}

#[tokio::test]
async fn current_thread_transport_remains_responsive_and_cancels_work() {
    responsive_transport().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn single_worker_transport_remains_responsive_and_cancels_work() {
    responsive_transport().await;
}

#[tokio::test]
async fn disconnect_cancels_an_unbounded_evaluation_and_releases_its_worker() {
    let mut fixture = fixture(1);
    let (mut client, transport) = connect(fixture.server.clone()).await;
    let (_, pending) = client
        .request::<CallToolResult>(ClientRequest::call_tool(
            "script_eval",
            Some(
                tmcp::Arguments::from_struct(ScriptEvalRequest::new(
                    "probe.started(); canopy.wait_for(function() return false end)",
                ))
                .expect("arguments"),
            ),
            None,
        ))
        .await
        .expect("unbounded evaluation");
    let id = receive(&mut fixture.started).await;
    client.disconnect().await;
    drop(pending);
    wait_for_drop(&mut fixture.dropped, id).await;
    let _permit = timeout(DEADLINE, fixture.server.workers.acquire())
        .await
        .expect("worker freed")
        .expect("semaphore open");
    timeout(DEADLINE, transport)
        .await
        .expect("transport exits")
        .expect("transport task")
        .expect("transport result");
}

#[tokio::test]
async fn cancelled_queued_request_never_constructs_an_application() {
    let mut fixture = fixture(1);
    let (client, transport) = connect(fixture.server.clone()).await;
    let (active, active_result) = client
        .request::<CallToolResult>(request(
            "probe.started(); canopy.wait_for(function() return false end)",
        ))
        .await
        .expect("active request");
    let id = receive(&mut fixture.started).await;
    let (queued, queued_result) = client
        .request::<CallToolResult>(request("return 99"))
        .await
        .expect("queued request");
    client
        .ping()
        .await
        .expect("transport processes queued requests");
    client.cancel(queued).await.expect("cancel queued request");
    client.cancel(active).await.expect("cancel active request");
    drop((active_result, queued_result));
    wait_for_drop(&mut fixture.dropped, id).await;
    let permit = timeout(DEADLINE, fixture.server.workers.acquire())
        .await
        .expect("worker freed")
        .expect("semaphore open");
    assert_eq!(fixture.built.load(Ordering::SeqCst), 1);
    drop(permit);
    close(client, transport).await;
}

#[tokio::test]
async fn dropping_headless_call_cancels_its_worker() {
    let mut fixture = fixture(1);
    let context = ServerCtx::notification_only(async_mpsc::channel(1).0);
    let request =
        ScriptEvalRequest::new("probe.started(); canopy.wait_for(function() return false end)");
    let mut eval = Box::pin(fixture.server.script_eval(&context, request));
    let id = tokio::select! {
        id = receive(&mut fixture.started) => id,
        result = &mut eval => panic!("evaluation must wait: {result:?}"),
    };
    drop(eval);
    wait_for_drop(&mut fixture.dropped, id).await;
    let _permit = timeout(DEADLINE, fixture.server.workers.acquire())
        .await
        .expect("worker freed")
        .expect("semaphore open");
}

#[tokio::test]
async fn cancelled_native_work_retains_its_permit_until_it_exits() {
    let fixture = fixture(1);
    let context = ServerCtx::notification_only(async_mpsc::channel(1).0);
    let (started, ready) = oneshot::channel();
    let (release, wait) = mpsc::channel();
    let mut call = Box::pin(fixture.server.run_headless(&context, move |_, _cancelled| {
        started.send(()).expect("start observer");
        wait.recv_timeout(DEADLINE).expect("native work released");
        Ok(())
    }));
    tokio::select! {
        result = ready => result.expect("worker starts"),
        result = &mut call => panic!("native work must wait: {result:?}"),
    }
    drop(call);
    assert!(fixture.server.workers.try_acquire().is_err());
    release.send(()).expect("release worker");
    let _permit = timeout(DEADLINE, fixture.server.workers.acquire())
        .await
        .expect("worker freed")
        .expect("semaphore open");
}

#[tokio::test]
async fn live_transport_cancellation_and_disconnect_wake_the_ui_driver() {
    use std::sync::atomic::AtomicBool;

    use futures::StreamExt;

    for disconnect in [false, true] {
        const SOURCE: &str = "probe.started(); canopy.wait_for(function() return false end)";
        let (ready_tx, ready_rx) = oneshot::channel();
        let (started_tx, mut started) = async_mpsc::unbounded_channel();
        let (cancelled_tx, mut cancelled) = async_mpsc::unbounded_channel();
        let stop = Arc::new(AtomicBool::new(false));
        let ui_stop = Arc::clone(&stop);
        let ui = thread::spawn(move || {
            let runtime = Builder::new_current_thread().enable_all().build().unwrap();
            let _entered = runtime.enter();
            let probe = Probe {
                id: 0,
                owner: thread::current().id(),
                started: started_tx,
                dropped: async_mpsc::unbounded_channel().0,
            };
            let mut canopy = CanopyBuilder::new()
                .configure(|canopy| canopy.add_commands::<Probe>())
                .assemble(move |canopy| canopy.replace_root(probe).map(|_| ()))
                .build()
                .expect("live app");
            canopy.set_root_size(Size::new(20, 5)).unwrap();
            canopy.turn(Work::Prepare).unwrap();
            ready_tx
                .send(canopy.automation_handle())
                .ok()
                .expect("live handle");
            let mut events = canopy.take_event_receiver().expect("UI events");
            let mut reported = false;
            while !ui_stop.load(Ordering::SeqCst) {
                runtime
                    .block_on(async { timeout(DEADLINE, events.next()).await })
                    .expect("UI receives a wake")
                    .expect("UI event");
                canopy.turn(Work::Wake).expect("UI turn");
                if !reported
                    && canopy.script_journal().iter().any(|entry| {
                        entry.source == SOURCE
                            && entry
                                .error
                                .as_ref()
                                .is_some_and(|error| error.contains("cancel"))
                    })
                {
                    reported = true;
                    cancelled_tx.send(0).expect("cancellation observer");
                }
            }
        });
        let automation = timeout(DEADLINE, ready_rx).await.unwrap().unwrap();
        let server = LiveCanopyMcpServer {
            automation: automation.clone(),
            context: LiveContext::new(AppMetadata::test()),
        };
        let (mut client, transport) = connect(server.clone()).await;
        let (id, pending) = client
            .request::<CallToolResult>(request(SOURCE))
            .await
            .expect("live request");
        receive(&mut started).await;
        if disconnect {
            client.disconnect().await;
        } else {
            client.cancel(id).await.expect("cancel live request");
        }
        drop(pending);
        receive(&mut cancelled).await;
        close(client, transport).await;
        let (client, transport) = connect(server).await;
        let response = timeout(
            DEADLINE,
            client.call_tool("script_eval", ScriptEvalRequest::new("return 42")),
        )
        .await
        .expect("live app reusable")
        .expect("second live evaluation");
        assert_eq!(response.structured_content.expect("payload")["value"], 42);
        close(client, transport).await;
        automation
            .submit(Box::new(move |_| stop.store(true, Ordering::SeqCst)))
            .expect("stop UI");
        spawn_blocking(move || ui.join().expect("UI exits"))
            .await
            .expect("join UI");
    }
}
