//! Private Unix-domain socket transport with 4-byte big-endian length framing.

use crate::protocol::{ErrorBody, Request, Response};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const MAX_REQUEST: usize = 1 << 20;
pub const MAX_RESPONSE: usize = 32 << 20;

/// `$TMPDIR/tracker-rs-<uid>/control.sock` unless `TRACKER_CONTROL_SOCKET` is set.
pub fn default_path() -> PathBuf {
    if let Some(p) = std::env::var_os("TRACKER_CONTROL_SOCKET") {
        return PathBuf::from(p);
    }
    std::env::temp_dir().join(format!("tracker-rs-{}", unsafe { libc::getuid() })).join("control.sock")
}

fn err(code: &str, e: impl std::fmt::Display) -> ErrorBody {
    ErrorBody::new(code, e.to_string())
}

fn peer_is_same_user(stream: &UnixStream) -> bool {
    let (mut uid, mut gid) = (0, 0);
    unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) == 0 && uid == libc::getuid() }
}

pub fn write_frame(stream: &mut UnixStream, data: &[u8], limit: usize) -> Result<(), ErrorBody> {
    if data.len() > limit {
        return Err(ErrorBody::new("message_too_large", "Control message exceeds the size limit."));
    }
    stream.write_all(&(data.len() as u32).to_be_bytes()).and_then(|_| stream.write_all(data)).map_err(|e| err("connection_error", e))
}

pub fn read_frame(stream: &mut UnixStream, limit: usize) -> Result<Vec<u8>, ErrorBody> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len).map_err(|e| err("connection_error", e))?;
    let n = u32::from_be_bytes(len) as usize;
    if n == 0 || n > limit {
        return Err(ErrorBody::new("message_too_large", "Invalid control message length."));
    }
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf).map_err(|e| err("connection_error", e))?;
    Ok(buf)
}

/// Sends one request on a fresh connection and waits up to `timeout` for the response.
pub fn call(path: &Path, request: &Request, timeout: Duration) -> Result<serde_json::Value, ErrorBody> {
    let mut stream = UnixStream::connect(path).map_err(|e| err("connection_error", format!("Could not connect to Tracker: {e}")))?;
    if !peer_is_same_user(&stream) {
        return Err(ErrorBody::new("unauthorized", "The control socket belongs to another user."));
    }
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(Duration::from_secs(30))).ok();
    write_frame(&mut stream, &serde_json::to_vec(request).map_err(|e| err("invalid_request", e))?, MAX_REQUEST)?;
    let data = read_frame(&mut stream, MAX_RESPONSE)?;
    let response: Response = serde_json::from_slice(&data).map_err(|e| err("invalid_response", e))?;
    if response.version != crate::PROTOCOL_VERSION {
        return Err(ErrorBody::new("incompatible_protocol", "Tracker speaks a different control protocol version."));
    }
    response.into_result()
}

/// Owns the socket path while alive; removes the socket when dropped.
pub struct Server {
    path: PathBuf,
    _lock: std::fs::File,
}

impl Server {
    /// Binds the socket and serves each connection on its own thread with `handler`.
    pub fn start(path: PathBuf, handler: impl Fn(Request) -> Response + Send + Sync + 'static) -> Result<Self, ErrorBody> {
        let dir = path.parent().ok_or_else(|| ErrorBody::new("invalid_endpoint", "Socket path has no folder."))?;
        if !dir.exists() {
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(|e| err("unsafe_endpoint", e))?;
        }
        let meta = std::fs::symlink_metadata(dir).map_err(|e| err("unsafe_endpoint", e))?;
        if !meta.is_dir() || meta.uid() != unsafe { libc::getuid() } || meta.mode() & 0o077 != 0 {
            return Err(ErrorBody::new("unsafe_endpoint", format!("{} must be a private folder owned by you.", dir.display())));
        }
        let lock_path = path.with_extension("lock");
        let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&lock_path).map_err(|e| err("unsafe_endpoint", e))?;
        std::fs::set_permissions(&lock_path, std::fs::Permissions::from_mode(0o600)).ok();
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(ErrorBody::new("already_running", "Another Tracker instance owns the control socket."));
        }
        if let Ok(m) = std::fs::symlink_metadata(&path) {
            use std::os::unix::fs::FileTypeExt;
            if !m.file_type().is_socket() {
                return Err(ErrorBody::new("unsafe_endpoint", "Unexpected file at the control socket path."));
            }
            std::fs::remove_file(&path).map_err(|e| err("unsafe_endpoint", e))?;
        }
        let listener = UnixListener::bind(&path).map_err(|e| err("bind_failed", e))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(|e| err("bind_failed", e))?;
        let handler = std::sync::Arc::new(handler);
        let active = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        std::thread::Builder::new()
            .name("tracker-control".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    // Bound concurrent connections; extra clients get a busy error immediately.
                    if active.load(std::sync::atomic::Ordering::SeqCst) >= 16 {
                        let _ = write_frame(&mut stream, &serde_json::to_vec(&Response::err(ErrorBody::new("busy", "Too many concurrent requests."))).unwrap(), MAX_RESPONSE);
                        continue;
                    }
                    let (handler, active) = (handler.clone(), active.clone());
                    active.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    std::thread::spawn(move || {
                        stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
                        stream.set_write_timeout(Some(Duration::from_secs(60))).ok();
                        let response = if !peer_is_same_user(&stream) {
                            Response::err(ErrorBody::new("unauthorized", "Peer is not the current user."))
                        } else {
                            match read_frame(&mut stream, MAX_REQUEST).and_then(|d| serde_json::from_slice::<Request>(&d).map_err(|e| err("invalid_request", e))) {
                                Ok(req) if req.version != crate::PROTOCOL_VERSION => Response::err(ErrorBody::new("incompatible_protocol", "Unsupported protocol version.")),
                                Ok(req) => handler(req),
                                Err(e) => Response::err(e),
                            }
                        };
                        let data = serde_json::to_vec(&response).unwrap_or_default();
                        let _ = write_frame(&mut stream, &data, MAX_RESPONSE);
                        active.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    });
                }
            })
            .map_err(|e| err("bind_failed", e))?;
        Ok(Self { path, _lock: lock })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trip_exclusive_and_private() {
        let dir = tempfile::Builder::new().prefix("trk").tempdir_in("/tmp").unwrap();
        let sock_dir = dir.path().join("ctl");
        let path = sock_dir.join("c.sock");
        let server = Server::start(path.clone(), |req| Response::ok(json!({"echo": req.command}))).unwrap();
        assert_eq!(std::fs::metadata(&sock_dir).unwrap().mode() & 0o777, 0o700);
        assert_eq!(std::fs::symlink_metadata(&path).unwrap().mode() & 0o777, 0o600);
        let req = Request { version: 1, client_id: uuid::Uuid::new_v4(), client_name: None, command: "ping".into(), arguments: Default::default() };
        assert_eq!(call(&path, &req, Duration::from_secs(5)).unwrap(), json!({"echo": "ping"}));
        assert_eq!(Server::start(path.clone(), |_| Response::ok(json!(null))).err().unwrap().code, "already_running");
        let bad = Request { version: 9, ..req };
        assert_eq!(call(&path, &bad, Duration::from_secs(5)).unwrap_err().code, "incompatible_protocol");
        drop(server);
        assert!(!path.exists());
    }
}
