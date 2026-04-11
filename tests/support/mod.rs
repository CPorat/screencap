use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::fd::AsRawFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};

const STUB_ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);
const STUB_READ_POLL_INTERVAL: Duration = Duration::from_millis(50);
const STUB_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const STUB_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

pub struct IntegrationTestLock {
    file: File,
}

impl IntegrationTestLock {
    pub fn acquire() -> Result<Self> {
        let path = std::env::temp_dir().join("screencap-integration-tests.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| {
                format!("failed to open integration test lock at {}", path.display())
            })?;

        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
        if result != 0 {
            return Err(std::io::Error::last_os_error()).with_context(|| {
                format!(
                    "failed to acquire integration test lock at {}",
                    path.display()
                )
            });
        }

        Ok(Self { file })
    }
}

impl Drop for IntegrationTestLock {
    fn drop(&mut self) {
        let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}

pub struct StubHttpAction {
    pub response: String,
    pub keep_running: bool,
}

impl StubHttpAction {
    fn response(&self) -> &str {
        &self.response
    }

    fn should_exit(&self) -> bool {
        !self.keep_running
    }
}

pub struct StubHttpServer {
    label: &'static str,
    address: SocketAddr,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<Result<()>>>,
}

impl StubHttpServer {
    pub fn spawn<F>(label: &'static str, mut handler: F) -> Self
    where
        F: FnMut(String) -> Result<StubHttpAction> + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .unwrap_or_else(|error| panic!("bind {label} stub listener: {error}"));
        listener
            .set_nonblocking(true)
            .unwrap_or_else(|error| panic!("set {label} stub listener nonblocking: {error}"));
        let address = listener
            .local_addr()
            .unwrap_or_else(|error| panic!("read {label} stub listener address: {error}"));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);

        let handle = thread::spawn(move || -> Result<()> {
            loop {
                if thread_shutdown.load(Ordering::Relaxed) {
                    return Ok(());
                }

                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(STUB_ACCEPT_POLL_INTERVAL);
                        continue;
                    }
                    Err(error) => {
                        return Err(error).with_context(|| format!("accept {label} stub request"));
                    }
                };

                if thread_shutdown.load(Ordering::Relaxed) {
                    return Ok(());
                }

                let Some(request) = read_http_request(&mut stream, &thread_shutdown, label)? else {
                    return Ok(());
                };
                let action =
                    handler(request).with_context(|| format!("handle {label} stub request"))?;
                stream
                    .write_all(action.response().as_bytes())
                    .with_context(|| format!("write {label} stub response"))?;
                stream
                    .flush()
                    .with_context(|| format!("flush {label} stub response"))?;

                if action.should_exit() {
                    return Ok(());
                }
            }
        });

        Self {
            label,
            address,
            shutdown,
            handle: Some(handle),
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }
}

impl Drop for StubHttpServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.address);

        if let Some(handle) = self.handle.take() {
            let deadline = Instant::now() + STUB_SHUTDOWN_TIMEOUT;
            while !handle.is_finished() {
                if Instant::now() >= deadline {
                    panic!(
                        "{} stub server did not exit within {:?}; probable deadlock in test teardown",
                        self.label, STUB_SHUTDOWN_TIMEOUT
                    );
                }
                thread::sleep(STUB_ACCEPT_POLL_INTERVAL);
            }

            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => panic!(
                    "{} stub server failed during shutdown: {error:#}",
                    self.label
                ),
                Err(panic) => panic!(
                    "{} stub server panicked during shutdown: {panic:?}",
                    self.label
                ),
            }
        }
    }
}

pub fn json_http_response(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn read_http_request(
    stream: &mut TcpStream,
    shutdown: &AtomicBool,
    label: &str,
) -> Result<Option<String>> {
    stream
        .set_read_timeout(Some(STUB_READ_POLL_INTERVAL))
        .with_context(|| format!("set {label} stub read timeout"))?;

    let deadline = Instant::now() + STUB_REQUEST_TIMEOUT;
    let mut buffer = Vec::new();

    loop {
        let mut chunk = [0_u8; 8192];
        match stream.read(&mut chunk) {
            Ok(0) => {
                if buffer.is_empty() && shutdown.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                if buffer.is_empty() {
                    return Err(anyhow!(
                        "{label} stub client closed the connection before sending an HTTP request"
                    ));
                }
                if http_request_complete(&buffer)? {
                    break;
                }
                return Err(anyhow!(
                    "{label} stub client closed the connection before sending a complete HTTP request"
                ));
            }
            Ok(bytes_read) => {
                buffer.extend_from_slice(&chunk[..bytes_read]);
                if http_request_complete(&buffer)? {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if shutdown.load(Ordering::Relaxed) && buffer.is_empty() {
                    return Ok(None);
                }
                if http_request_complete(&buffer)? {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(anyhow!(
                        "{label} stub timed out waiting for a complete HTTP request ({} bytes received)",
                        buffer.len()
                    ));
                }
            }
            Err(error) => {
                return Err(error).with_context(|| format!("read {label} stub request"));
            }
        }
    }

    String::from_utf8(buffer)
        .map(Some)
        .with_context(|| format!("decode {label} stub request as UTF-8"))
}

fn http_request_complete(buffer: &[u8]) -> Result<bool> {
    let Some(headers_end) = find_header_end(buffer) else {
        return Ok(false);
    };

    let headers = std::str::from_utf8(&buffer[..headers_end])
        .context("decode stub request headers as UTF-8")?;
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then_some(value.trim().parse::<usize>())
        })
        .transpose()
        .context("parse stub request Content-Length header")?
        .unwrap_or(0);

    Ok(buffer.len() >= headers_end + 4 + content_length)
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}
