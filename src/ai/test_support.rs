use std::{
    env,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    thread,
};

pub(crate) struct EnvGuard {
    key: String,
    previous: Option<String>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    pub(crate) fn set(key: &str, value: &str) -> Self {
        static ENV_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        let lock = ENV_LOCK
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("lock env");
        let previous = env::var(key).ok();
        env::set_var(key, value);
        Self {
            key: key.to_owned(),
            previous,
            _lock: lock,
        }
    }

    pub(crate) fn unset(key: &str) -> Self {
        static ENV_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        let lock = ENV_LOCK
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .expect("lock env");
        let previous = env::var(key).ok();
        env::remove_var(key);
        Self {
            key: key.to_owned(),
            previous,
            _lock: lock,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            env::set_var(&self.key, previous);
        } else {
            env::remove_var(&self.key);
        }
    }
}

pub(crate) struct TestServer {
    address: SocketAddr,
    handle: Option<thread::JoinHandle<()>>,
}

impl TestServer {
    pub(crate) fn spawn(status: u16, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener addr");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("read request");
            let response = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });

        Self {
            address,
            handle: Some(handle),
        }
    }

    pub(crate) fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().expect("join server thread");
        }
    }
}
