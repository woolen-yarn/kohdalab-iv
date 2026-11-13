//! Versioned, bounded local IPC contract for KohdaLab instrument services.
//!
//! This crate deliberately contains no device or backend code.  A client only
//! connects to an already-selected loopback endpoint and never retries a
//! transfer whose completion could be unknown.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fmt;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 2;
pub const MAX_FRAME: usize = 64 * 1024;
pub const MAX_PAYLOAD: usize = 16 * 1024;
pub const MAX_TRANSFER_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Endpoint {
    pub protocol: u32,
    pub address: SocketAddr,
    pub token: String,
}

impl fmt::Debug for Endpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Endpoint")
            .field("protocol", &self.protocol)
            .field("address", &self.address)
            .field("token", &"[redacted]")
            .finish()
    }
}

impl Endpoint {
    pub fn validate(&self) -> Result<(), HubError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(HubError::new(
                ErrorCode::IncompatibleVersion,
                format!(
                    "Instrument service protocol {} is unsupported (client requires {}).",
                    self.protocol, PROTOCOL_VERSION
                ),
            ));
        }
        if !self.address.ip().is_loopback() || self.address.port() == 0 {
            return Err(HubError::new(
                ErrorCode::InvalidRequest,
                "Instrument service endpoint must be a loopback address with a nonzero port.",
            ));
        }
        if self.token.trim().is_empty()
            || self.token.len() > 512
            || self.token.contains(['\r', '\n'])
        {
            return Err(HubError::new(
                ErrorCode::InvalidRequest,
                "Instrument service endpoint token is invalid.",
            ));
        }
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, HubError> {
        let path = path.as_ref();
        let metadata = fs::metadata(path).map_err(|error| {
            HubError::new(
                ErrorCode::ConnectionLost,
                format!("Read instrument endpoint: {error}"),
            )
        })?;
        if metadata.len() > MAX_FRAME as u64 {
            return Err(HubError::new(
                ErrorCode::InvalidRequest,
                "Instrument endpoint file exceeds the protocol frame limit.",
            ));
        }
        let bytes = fs::read(path).map_err(|error| {
            HubError::new(
                ErrorCode::ConnectionLost,
                format!("Read instrument endpoint: {error}"),
            )
        })?;
        if bytes.len() > MAX_FRAME {
            return Err(HubError::new(
                ErrorCode::InvalidRequest,
                "Instrument endpoint file exceeds the protocol frame limit.",
            ));
        }
        let endpoint: Self = serde_json::from_slice(&bytes).map_err(|error| {
            HubError::new(
                ErrorCode::InvalidRequest,
                format!("Parse instrument endpoint: {error}"),
            )
        })?;
        endpoint.validate()?;
        Ok(endpoint)
    }
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Hello {
    pub protocol: u32,
    pub token: String,
    pub app: String,
}

impl fmt::Debug for Hello {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Hello")
            .field("protocol", &self.protocol)
            .field("token", &"[redacted]")
            .field("app", &self.app)
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct HelloReply {
    pub protocol: u32,
    pub session_id: String,
    #[serde(default)]
    pub request_nonce: String,
    pub error: Option<HubError>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct HubError {
    pub code: ErrorCode,
    pub message: String,
}

impl HubError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for HubError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for HubError {}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ResourceInUse,
    Deadline,
    Cancelled,
    ConnectionLost,
    StateUnknown,
    IncompatibleVersion,
    InvalidRequest,
    ControllerFault,
    Internal,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SafetyPolicy {
    Instrument,
    Source { model: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Claim {
        address: u8,
        policy: SafetyPolicy,
    },
    Transfer {
        operation: String,
        address: u8,
        timeout_ms: u64,
        payload: Vec<u8>,
    },
    Release {
        address: u8,
    },
    List,
    Close,
    Recover {
        address: u8,
        model: String,
    },
    Status,
}

impl Request {
    pub fn validate(&self) -> Result<(), HubError> {
        let validate_address = |address| {
            if (1..=30).contains(&address) {
                Ok(())
            } else {
                Err(HubError::new(
                    ErrorCode::InvalidRequest,
                    "GPIB address must be 1–30.",
                ))
            }
        };
        match self {
            Self::Claim { address, policy } => {
                validate_address(*address)?;
                if let SafetyPolicy::Source { model } = policy {
                    validate_model(model)?;
                }
            }
            Self::Transfer {
                operation,
                address,
                timeout_ms,
                payload,
            } => {
                validate_address(*address)?;
                if !(1..=MAX_TRANSFER_TIMEOUT.as_millis() as u64).contains(timeout_ms) {
                    return Err(HubError::new(
                        ErrorCode::InvalidRequest,
                        "Transfer timeout must be 1–30000 ms.",
                    ));
                }
                if payload.len() > MAX_PAYLOAD {
                    return Err(HubError::new(
                        ErrorCode::InvalidRequest,
                        "Transfer payload exceeds 16 KiB.",
                    ));
                }
                if !matches!(
                    operation.as_str(),
                    "WRITE" | "QUERY" | "READ" | "CLEAR" | "LOCAL" | "REN"
                ) {
                    return Err(HubError::new(
                        ErrorCode::InvalidRequest,
                        "Unsupported instrument transfer operation.",
                    ));
                }
            }
            Self::Release { address } => validate_address(*address)?,
            Self::Recover { address, model } => {
                validate_address(*address)?;
                validate_model(model)?;
            }
            Self::List | Self::Close | Self::Status => {}
        }
        Ok(())
    }
}

fn validate_model(model: &str) -> Result<(), HubError> {
    if model.trim().is_empty() || model.len() > 128 || model.contains(['\r', '\n']) {
        Err(HubError::new(
            ErrorCode::InvalidRequest,
            "Instrument model is invalid.",
        ))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Envelope {
    pub id: u64,
    pub timeout_ms: u64,
    #[serde(default)]
    pub request_nonce: String,
    pub request: Request,
}

impl Envelope {
    pub fn validate(&self) -> Result<(), HubError> {
        if !(1..=MAX_TRANSFER_TIMEOUT.as_millis() as u64).contains(&self.timeout_ms) {
            return Err(HubError::new(
                ErrorCode::InvalidRequest,
                "Request timeout must be 1–30000 ms.",
            ));
        }
        validate_request_nonce(&self.request_nonce)?;
        self.request.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Reply {
    pub id: u64,
    pub payload: String,
    #[serde(default)]
    pub request_nonce: String,
    pub error: Option<HubError>,
}

fn validate_request_nonce(nonce: &str) -> Result<(), HubError> {
    if nonce.is_empty() || nonce.len() > 128 || nonce.contains(['\r', '\n']) {
        return Err(HubError::new(
            ErrorCode::InvalidRequest,
            "Instrument service request nonce is invalid.",
        ));
    }
    Ok(())
}

pub fn read_json<T: DeserializeOwned>(reader: &mut impl BufRead) -> Result<T, HubError> {
    let mut bytes = Vec::with_capacity(1024);
    let count = reader
        .by_ref()
        .take((MAX_FRAME + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(io_error)?;
    if count == 0 {
        return Err(HubError::new(
            ErrorCode::ConnectionLost,
            "Instrument service closed the connection.",
        ));
    }
    if count > MAX_FRAME || bytes.last() != Some(&b'\n') {
        return Err(HubError::new(
            ErrorCode::InvalidRequest,
            "Instrument service frame is malformed or exceeds 64 KiB.",
        ));
    }
    bytes.pop();
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        HubError::new(
            ErrorCode::InvalidRequest,
            format!("Malformed instrument service JSON: {error}"),
        )
    })
}

pub fn write_json<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<(), HubError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        HubError::new(
            ErrorCode::Internal,
            format!("Encode instrument service JSON: {error}"),
        )
    })?;
    if bytes.len() + 1 > MAX_FRAME {
        return Err(HubError::new(
            ErrorCode::InvalidRequest,
            "Instrument service frame exceeds 64 KiB.",
        ));
    }
    writer
        .write_all(&bytes)
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(io_error)
}

fn io_error(error: io::Error) -> HubError {
    let code = match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => ErrorCode::Deadline,
        io::ErrorKind::UnexpectedEof
        | io::ErrorKind::BrokenPipe
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::NotConnected => ErrorCode::ConnectionLost,
        _ => ErrorCode::ConnectionLost,
    };
    HubError::new(code, format!("Instrument service transport: {error}"))
}

pub struct Client {
    writer: Option<TcpStream>,
    reader: Option<BufReader<TcpStream>>,
    session_id: String,
    request_nonce: String,
    next_id: u64,
}

impl Client {
    pub fn connect(endpoint: &Endpoint, app: &str) -> Result<Self, HubError> {
        endpoint.validate()?;
        if app.trim().is_empty() || app.len() > 128 || app.contains(['\r', '\n']) {
            return Err(HubError::new(
                ErrorCode::InvalidRequest,
                "Application name is invalid.",
            ));
        }
        let mut writer =
            TcpStream::connect_timeout(&endpoint.address, CONNECT_TIMEOUT).map_err(io_error)?;
        writer
            .set_read_timeout(Some(CONNECT_TIMEOUT))
            .map_err(io_error)?;
        writer
            .set_write_timeout(Some(CONNECT_TIMEOUT))
            .map_err(io_error)?;
        let reader_stream = writer.try_clone().map_err(io_error)?;
        let mut reader = BufReader::new(reader_stream);
        write_json(
            &mut writer,
            &Hello {
                protocol: PROTOCOL_VERSION,
                token: endpoint.token.clone(),
                app: app.into(),
            },
        )?;
        let reply: HelloReply = read_json(&mut reader)?;
        if reply.protocol != PROTOCOL_VERSION {
            let _ = writer.shutdown(Shutdown::Both);
            return Err(HubError::new(
                ErrorCode::IncompatibleVersion,
                "Instrument service protocol is incompatible.",
            ));
        }
        if let Some(error) = reply.error {
            let _ = writer.shutdown(Shutdown::Both);
            return Err(error);
        }
        if reply.session_id.is_empty() || reply.session_id.len() > 128 {
            let _ = writer.shutdown(Shutdown::Both);
            return Err(HubError::new(
                ErrorCode::InvalidRequest,
                "Instrument service returned an invalid session.",
            ));
        }
        if let Err(error) = validate_request_nonce(&reply.request_nonce) {
            let _ = writer.shutdown(Shutdown::Both);
            return Err(error);
        }
        Ok(Self {
            writer: Some(writer),
            reader: Some(reader),
            session_id: reply.session_id,
            request_nonce: reply.request_nonce,
            next_id: 1,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn request(&mut self, request: Request, timeout: Duration) -> Result<String, HubError> {
        if timeout.is_zero() || timeout > MAX_TRANSFER_TIMEOUT {
            return Err(HubError::new(
                ErrorCode::InvalidRequest,
                "Request timeout must be 1 ms–30 s.",
            ));
        }
        request.validate()?;
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or_else(|| {
            HubError::new(
                ErrorCode::Internal,
                "Instrument service request ID exhausted.",
            )
        })?;
        let timeout_ms = timeout
            .as_millis()
            .clamp(1, MAX_TRANSFER_TIMEOUT.as_millis()) as u64;
        let envelope = Envelope {
            id,
            timeout_ms,
            request_nonce: self.request_nonce.clone(),
            request,
        };
        let result = (|| {
            let writer = self.writer.as_mut().ok_or_else(|| {
                HubError::new(
                    ErrorCode::ConnectionLost,
                    "Instrument service session is closed.",
                )
            })?;
            writer.set_read_timeout(Some(timeout)).map_err(io_error)?;
            writer.set_write_timeout(Some(timeout)).map_err(io_error)?;
            write_json(writer, &envelope)?;
            let reader = self.reader.as_mut().ok_or_else(|| {
                HubError::new(
                    ErrorCode::ConnectionLost,
                    "Instrument service session is closed.",
                )
            })?;
            let reply: Reply = read_json(reader)?;
            if reply.id != id {
                return Err(HubError::new(
                    ErrorCode::ConnectionLost,
                    "Instrument service response ID did not match the request.",
                ));
            }
            Ok(reply)
        })();
        match result {
            Ok(reply) => {
                if let Err(error) = validate_request_nonce(&reply.request_nonce) {
                    self.close_transport();
                    return Err(error);
                }
                self.request_nonce = reply.request_nonce;
                if let Some(error) = reply.error {
                    if matches!(
                        error.code,
                        ErrorCode::ControllerFault
                            | ErrorCode::ConnectionLost
                            | ErrorCode::IncompatibleVersion
                    ) {
                        self.close_transport();
                    }
                    Err(error)
                } else {
                    Ok(reply.payload)
                }
            }
            Err(error) => {
                self.close_transport();
                Err(error)
            }
        }
    }

    fn close_transport(&mut self) {
        self.reader.take();
        if let Some(stream) = self.writer.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.close_transport();
    }
}

pub fn new_session_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::net::TcpListener;
    use std::thread;

    fn endpoint(address: SocketAddr) -> Endpoint {
        Endpoint {
            protocol: PROTOCOL_VERSION,
            address,
            token: "test-token".into(),
        }
    }

    fn mock_server(reply_id: u64) -> (Endpoint, thread::JoinHandle<()>) {
        mock_server_with_reply_nonce(reply_id, "nonce-2")
    }

    fn mock_server_with_reply_nonce(
        reply_id: u64,
        reply_nonce: impl Into<String>,
    ) -> (Endpoint, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = endpoint(listener.local_addr().unwrap());
        let reply_nonce = reply_nonce.into();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let cloned = stream.try_clone().unwrap();
            let mut input = BufReader::new(cloned);
            let hello: Hello = read_json(&mut input).unwrap();
            assert_eq!(hello.token, "test-token");
            write_json(
                &mut stream,
                &HelloReply {
                    protocol: PROTOCOL_VERSION,
                    session_id: "session".into(),
                    request_nonce: "nonce-1".into(),
                    error: None,
                },
            )
            .unwrap();
            let request: Envelope = read_json(&mut input).unwrap();
            assert_eq!(request.request_nonce, "nonce-1");
            write_json(
                &mut stream,
                &Reply {
                    id: reply_id,
                    payload: "ok".into(),
                    request_nonce: reply_nonce,
                    error: None,
                },
            )
            .unwrap();
        });
        (endpoint, handle)
    }

    #[test]
    fn client_round_trip_is_bounded_and_versioned() {
        let (endpoint, server) = mock_server(1);
        let mut client = Client::connect(&endpoint, "iv").unwrap();
        assert_eq!(client.session_id(), "session");
        assert_eq!(
            client
                .request(Request::Status, Duration::from_secs(1))
                .unwrap(),
            "ok"
        );
        server.join().unwrap();
    }

    #[test]
    fn mismatched_reply_closes_client_without_retry() {
        let (endpoint, server) = mock_server(2);
        let mut client = Client::connect(&endpoint, "iv").unwrap();
        let error = client
            .request(Request::Status, Duration::from_secs(1))
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ConnectionLost);
        assert_eq!(
            client
                .request(Request::Status, Duration::from_secs(1))
                .unwrap_err()
                .code,
            ErrorCode::ConnectionLost
        );
        server.join().unwrap();
    }

    #[test]
    fn malformed_reply_nonce_closes_client_without_retry() {
        let (endpoint, server) = mock_server_with_reply_nonce(1, "");
        let mut client = Client::connect(&endpoint, "iv").unwrap();
        let error = client
            .request(Request::Status, Duration::from_secs(1))
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        assert_eq!(
            client
                .request(Request::Status, Duration::from_secs(1))
                .unwrap_err()
                .code,
            ErrorCode::ConnectionLost
        );
        server.join().unwrap();
    }

    #[test]
    fn missing_handshake_nonce_is_rejected_before_any_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = endpoint(listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let cloned = stream.try_clone().unwrap();
            let mut input = BufReader::new(cloned);
            let hello: Hello = read_json(&mut input).unwrap();
            assert_eq!(hello.token, "test-token");
            write_json(
                &mut stream,
                &HelloReply {
                    protocol: PROTOCOL_VERSION,
                    session_id: "session".into(),
                    request_nonce: String::new(),
                    error: None,
                },
            )
            .unwrap();
        });
        let error = match Client::connect(&endpoint, "iv") {
            Ok(_) => panic!("missing handshake nonce unexpectedly authenticated"),
            Err(error) => error,
        };
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        server.join().unwrap();
    }

    #[test]
    fn endpoint_rejects_non_loopback_and_incompatible_protocol() {
        let invalid = Endpoint {
            protocol: PROTOCOL_VERSION,
            address: "192.0.2.1:1".parse().unwrap(),
            token: "x".into(),
        };
        assert_eq!(
            invalid.validate().unwrap_err().code,
            ErrorCode::InvalidRequest
        );
        let incompatible = Endpoint {
            protocol: PROTOCOL_VERSION + 1,
            address: "127.0.0.1:1".parse().unwrap(),
            token: "x".into(),
        };
        assert_eq!(
            incompatible.validate().unwrap_err().code,
            ErrorCode::IncompatibleVersion
        );
    }

    #[test]
    fn oversized_and_malformed_frames_are_rejected() {
        let large = vec![b'x'; MAX_FRAME + 1];
        assert_eq!(
            read_json::<Reply>(&mut Cursor::new(large))
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
        assert_eq!(
            read_json::<Reply>(&mut Cursor::new(b"not json\n".to_vec()))
                .unwrap_err()
                .code,
            ErrorCode::InvalidRequest
        );
    }
}
