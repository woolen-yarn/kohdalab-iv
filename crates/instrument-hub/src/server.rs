//! Bounded authenticated loopback server; one worker owns all bus operations.
use crate::core::{Controller, Core, error};
use instrument_protocol::{
    Endpoint, Envelope, ErrorCode, Hello, HelloReply, PROTOCOL_VERSION, Reply, Request, read_json,
    write_json,
};
use std::collections::{BTreeMap, VecDeque};
use std::io::BufReader;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

struct Session {
    app: String,
    alive: Arc<AtomicBool>,
}
enum Event {
    Register {
        id: String,
        app: String,
        alive: Arc<AtomicBool>,
    },
    Command {
        session: String,
        envelope: Envelope,
        received: Instant,
        reply: mpsc::SyncSender<Reply>,
    },
}

/// Test shutdown is graceful. A production service must be kept running while
/// applications own outputs; crash recovery is fenced by the source journal.
pub fn run<B: Controller + 'static>(
    listener: TcpListener,
    endpoint: Endpoint,
    mut core: Core<B>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let (events, inbox) = mpsc::sync_channel::<Event>(64);
    let worker_stop = stop.clone();
    let worker = thread::spawn(move || {
        let mut sessions = BTreeMap::<String, Session>::new();
        let mut pending = VecDeque::new();
        while !worker_stop.load(Ordering::Acquire) {
            let lost: Vec<_> = sessions
                .iter()
                .filter(|(_, s)| !s.alive.load(Ordering::Acquire))
                .map(|(id, _)| id.clone())
                .collect();
            for id in lost {
                let _ = core.disconnect(&id, true);
                sessions.remove(&id);
            }
            if pending.is_empty() {
                match inbox.recv_timeout(Duration::from_millis(50)) {
                    Ok(event) => pending.push_back(event),
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                }
            }
            // At most one outstanding transaction per connection. FIFO is
            // fair for normal work; a verified-form OFF can pass queued polls.
            while pending.len() < 64 {
                match inbox.try_recv() {
                    Ok(event) => pending.push_back(event),
                    Err(_) => break,
                }
            }
            let safety = pending.iter().position(|event| match event {
                Event::Command { session, envelope, .. } if sessions.contains_key(session) => matches!(&envelope.request,
                    Request::Transfer { operation, payload, .. } if operation == "WRITE" && matches!(payload.as_slice(), b":OUTP OFF\n" | b"O0;E\n")),
                _ => false,
            });
            let Some(event) = pending.remove(safety.unwrap_or(0)) else {
                continue;
            };
            match event {
                Event::Register { id, app, alive } => {
                    sessions.insert(id, Session { app, alive });
                }
                Event::Command {
                    session,
                    envelope,
                    received,
                    reply,
                } => {
                    let deadline = received + Duration::from_millis(envelope.timeout_ms);
                    let result = if let Some(client) = sessions.get(&session) {
                        if !client.alive.load(Ordering::Acquire) {
                            Err(error(
                                ErrorCode::Cancelled,
                                "Client disconnected before transaction dispatch",
                            ))
                        } else if Instant::now() >= deadline {
                            Err(error(
                                ErrorCode::Deadline,
                                "Transaction expired while waiting for the GPIB bus",
                            ))
                        } else {
                            core.request(
                                &session,
                                &client.app,
                                envelope.request,
                                deadline.saturating_duration_since(Instant::now()),
                            )
                        }
                    } else {
                        Err(error(
                            ErrorCode::ConnectionLost,
                            "Instrument session is closed",
                        ))
                    };
                    let response = match result {
                        Ok(payload) => Reply {
                            id: envelope.id,
                            payload,
                            error: None,
                            request_nonce: String::new(),
                        },
                        Err(e) => Reply {
                            id: envelope.id,
                            payload: String::new(),
                            error: Some(e),
                            request_nonce: String::new(),
                        },
                    };
                    // A dead/slow writer cannot block the controller worker.
                    let _ = reply.try_send(response);
                }
            }
        }
        for (id, session) in sessions {
            session.alive.store(false, Ordering::Release);
            let _ = core.disconnect(&id, true);
        }
    });
    let clients = Arc::new(AtomicUsize::new(0));
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, peer)) if peer.ip().is_loopback() => {
                if clients.fetch_add(1, Ordering::AcqRel) >= 32 {
                    clients.fetch_sub(1, Ordering::AcqRel);
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                }
                let endpoint = endpoint.clone();
                let events = events.clone();
                let clients = clients.clone();
                thread::spawn(move || {
                    serve_client(stream, &endpoint, events);
                    clients.fetch_sub(1, Ordering::AcqRel);
                });
            }
            Ok((stream, _)) => {
                let _ = stream.shutdown(Shutdown::Both);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(e) => {
                stop.store(true, Ordering::Release);
                let _ = worker.join();
                return Err(e.to_string());
            }
        }
    }
    drop(events);
    worker
        .join()
        .map_err(|_| "Instrument worker panicked".to_owned())
}

fn serve_client(mut stream: TcpStream, endpoint: &Endpoint, events: mpsc::SyncSender<Event>) {
    // macOS accepts may inherit the listener's O_NONBLOCK flag.
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
    let Ok(read_stream) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(read_stream);
    let Ok(hello) = read_json::<Hello>(&mut reader) else {
        return;
    };
    let problem = if hello.protocol != PROTOCOL_VERSION {
        Some(error(
            ErrorCode::IncompatibleVersion,
            "Instrument service protocol mismatch",
        ))
    } else if hello.token != endpoint.token
        || hello.app.trim().is_empty()
        || hello.app.len() > 128
        || hello.app.contains(['\r', '\n'])
    {
        Some(error(
            ErrorCode::InvalidRequest,
            "Invalid instrument service credentials or application identity",
        ))
    } else {
        None
    };
    if let Some(e) = problem {
        let _ = write_json(
            &mut stream,
            &HelloReply {
                protocol: PROTOCOL_VERSION,
                session_id: String::new(),
                error: Some(e),
                request_nonce: String::new(),
            },
        );
        return;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let first_nonce = uuid::Uuid::new_v4().to_string();
    let alive = Arc::new(AtomicBool::new(true));
    if events
        .try_send(Event::Register {
            id: id.clone(),
            app: hello.app,
            alive: alive.clone(),
        })
        .is_err()
    {
        return;
    }
    if write_json(
        &mut stream,
        &HelloReply {
            protocol: PROTOCOL_VERSION,
            session_id: id.clone(),
            error: None,
            request_nonce: first_nonce.clone(),
        },
    )
    .is_err()
    {
        alive.store(false, Ordering::Release);
        return;
    }
    let _ = reader.get_ref().set_read_timeout(None);
    let (response_tx, responses) = mpsc::sync_channel::<Reply>(1);
    let expected_nonce = Arc::new(Mutex::new(Some(first_nonce)));
    let writer_alive = alive.clone();
    let writer_nonce = expected_nonce.clone();
    let Ok(mut writer_stream) = stream.try_clone() else {
        alive.store(false, Ordering::Release);
        return;
    };
    let writer = thread::spawn(move || {
        while writer_alive.load(Ordering::Acquire) {
            match responses.recv_timeout(Duration::from_millis(100)) {
                Ok(mut reply) => {
                    // An unpredictable turn token is disclosed only in the
                    // response. A peer cannot pre-pipeline another operation
                    // before receiving it; legitimate fast SDKs never need a
                    // timing-dependent wait or a write retry.
                    reply.request_nonce = uuid::Uuid::new_v4().to_string();
                    *writer_nonce.lock().expect("nonce lock") = Some(reply.request_nonce.clone());
                    if write_json(&mut writer_stream, &reply).is_err() {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            }
        }
        writer_alive.store(false, Ordering::Release);
        let _ = writer_stream.shutdown(Shutdown::Both);
    });
    let mut last_id = 0;
    // The reader keeps observing EOF while the worker is busy. Therefore
    // killing a proxy cancels queued output work before it is dispatched.
    while alive.load(Ordering::Acquire) {
        let Ok(envelope) = read_json::<Envelope>(&mut reader) else {
            break;
        };
        if envelope.validate().is_err() || envelope.id <= last_id {
            break;
        }
        let mut turn = expected_nonce.lock().expect("nonce lock");
        if turn.as_deref() != Some(envelope.request_nonce.as_str())
            || !alive.load(Ordering::Acquire)
        {
            break;
        }
        *turn = None;
        drop(turn);
        last_id = envelope.id;
        if events
            .try_send(Event::Command {
                session: id.clone(),
                envelope,
                received: Instant::now(),
                reply: response_tx.clone(),
            })
            .is_err()
        {
            break;
        }
    }
    alive.store(false, Ordering::Release);
    let _ = stream.shutdown(Shutdown::Both);
    drop(response_tx);
    let _ = writer.join();
}
