#![cfg(unix)]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use uuid::Uuid;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_MESSAGE: usize = 1024 * 1024;

struct SocketPath(PathBuf);

impl SocketPath {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("kryptopass-browser-proxy-{}.sock", Uuid::new_v4())))
    }

    fn as_path(&self) -> &Path {
        &self.0
    }
}

impl Drop for SocketPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct ManagedChild(Child);

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn spawn_proxy(path: &Path) -> (ManagedChild, thread::JoinHandle<Vec<u8>>) {
    let mut child = ManagedChild(
        Command::new(env!("CARGO_BIN_EXE_kryptopass-browser-proxy"))
            .arg("--socket")
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start compiled browser proxy"),
    );
    let mut output = child.0.stdout.take().expect("proxy stdout pipe");
    let output_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        output
            .read_to_end(&mut bytes)
            .expect("read proxy stdout to EOF");
        bytes
    });
    (child, output_reader)
}

fn accept_with_timeout(listener: &UnixListener) -> UnixStream {
    listener
        .set_nonblocking(true)
        .expect("set listener nonblocking");
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "proxy did not connect to socket");
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("accept proxy connection: {error}"),
        }
    }
}

fn wait_for_exit(child: &mut ManagedChild) -> ExitStatus {
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        if let Some(status) = child.0.try_wait().expect("poll proxy process") {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "proxy did not exit before timeout"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn framed(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_ne_bytes());
    frame.extend_from_slice(payload);
    frame
}

fn read_frame(stream: &mut UnixStream) -> Vec<u8> {
    let mut length = [0; 4];
    stream
        .read_exact(&mut length)
        .expect("read relayed frame length");
    let length = u32::from_ne_bytes(length) as usize;
    assert!(
        length > 0 && length <= MAX_MESSAGE,
        "invalid relayed frame length"
    );
    let mut payload = vec![0; length];
    stream
        .read_exact(&mut payload)
        .expect("read relayed frame payload");
    payload
}

fn run_proxy_with_closed_input(input: &[u8]) -> (ExitStatus, Vec<u8>) {
    let socket_path = SocketPath::new();
    let listener = UnixListener::bind(socket_path.as_path()).expect("bind native socket");
    let (mut child, output_reader) = spawn_proxy(socket_path.as_path());
    let mut native = accept_with_timeout(&listener);
    native
        .set_read_timeout(Some(PROCESS_TIMEOUT))
        .expect("set native read timeout");

    let mut stdin = child.0.stdin.take().expect("proxy stdin pipe");
    stdin.write_all(input).expect("write browser input");
    drop(stdin);

    let status = wait_for_exit(&mut child);
    let mut forwarded = [0; 1];
    let bytes = native.read(&mut forwarded).expect("check native socket");
    assert_eq!(bytes, 0, "incomplete browser input reached native socket");
    let output = output_reader.join().expect("join proxy stdout reader");
    (status, output)
}

#[test]
fn proxy_preserves_fragmented_and_coalesced_native_frames() {
    let socket_path = SocketPath::new();
    let listener = UnixListener::bind(socket_path.as_path()).expect("bind native socket");
    let (mut child, output_reader) = spawn_proxy(socket_path.as_path());
    let mut native = accept_with_timeout(&listener);
    native
        .set_read_timeout(Some(PROCESS_TIMEOUT))
        .expect("set native read timeout");
    native
        .set_write_timeout(Some(PROCESS_TIMEOUT))
        .expect("set native write timeout");

    let browser_frames = [
        b"{\"action\":\"first\",\"payload\":\"browser\"}".as_slice(),
        b"{\"action\":\"second\",\"sequence\":2}".as_slice(),
    ];
    let mut browser_input = Vec::new();
    for payload in browser_frames {
        browser_input.extend_from_slice(&framed(payload));
    }
    child
        .0
        .stdin
        .as_mut()
        .expect("proxy stdin pipe")
        .write_all(&browser_input)
        .expect("write coalesced browser frames");

    for expected in browser_frames {
        assert_eq!(read_frame(&mut native), expected);
    }

    let native_frames = [
        b"{\"reply\":\"fragmented\",\"sequence\":1}".as_slice(),
        b"{\"reply\":\"coalesced\",\"sequence\":2}".as_slice(),
        b"{\"reply\":\"coalesced\",\"sequence\":3}".as_slice(),
    ];

    // Split both the first frame's header and payload across separate writes.
    let first = framed(native_frames[0]);
    for chunk in [
        &first[0..1],
        &first[1..3],
        &first[3..4],
        &first[4..9],
        &first[9..],
    ] {
        native
            .write_all(chunk)
            .expect("write fragmented native frame");
        thread::sleep(Duration::from_millis(10));
    }

    // One write carries two complete frames back-to-back on the socket.
    let mut coalesced = framed(native_frames[1]);
    coalesced.extend_from_slice(&framed(native_frames[2]));
    native
        .write_all(&coalesced)
        .expect("write coalesced native frames");
    drop(native);

    let status = wait_for_exit(&mut child);
    assert!(status.success(), "proxy exited with {status}");

    let actual_output = output_reader.join().expect("join proxy stdout reader");
    let mut expected_output = Vec::new();
    for payload in native_frames {
        expected_output.extend_from_slice(&framed(payload));
    }
    assert_eq!(actual_output, expected_output);
}

#[test]
fn proxy_rejects_oversized_browser_input_without_forwarding_it() {
    let socket_path = SocketPath::new();
    let listener = UnixListener::bind(socket_path.as_path()).expect("bind native socket");
    let (mut child, output_reader) = spawn_proxy(socket_path.as_path());
    let mut native = accept_with_timeout(&listener);
    native
        .set_read_timeout(Some(PROCESS_TIMEOUT))
        .expect("set native read timeout");

    child
        .0
        .stdin
        .as_mut()
        .expect("proxy stdin pipe")
        .write_all(&u32::MAX.to_ne_bytes())
        .expect("write oversized browser frame header");

    // Keep stdin open while waiting: a proxy that allocates from this length
    // before checking its bound would block waiting for a multi-gigabyte body.
    let status = wait_for_exit(&mut child);
    assert!(
        matches!(status.code(), Some(code) if code != 0),
        "oversized input should terminate the proxy with a nonzero status, got {status}"
    );

    let mut forwarded = [0; 1];
    let bytes = native.read(&mut forwarded).expect("check native socket");
    assert_eq!(bytes, 0, "oversized browser frame reached native socket");
    assert!(
        output_reader
            .join()
            .expect("join proxy stdout reader")
            .is_empty(),
        "oversized input produced browser output"
    );
}

#[test]
fn proxy_accepts_clean_browser_stdin_eof() {
    let (status, output) = run_proxy_with_closed_input(&[]);

    assert!(
        status.success(),
        "clean stdin EOF should succeed, got {status}"
    );
    assert!(output.is_empty(), "clean EOF produced browser output");
}

#[test]
fn proxy_rejects_truncated_browser_frame_header() {
    let (status, output) = run_proxy_with_closed_input(&[0x04, 0x00, 0x00]);

    assert!(
        matches!(status.code(), Some(code) if code != 0),
        "truncated frame header should fail, got {status}"
    );
    assert!(
        output.is_empty(),
        "truncated header produced browser output"
    );
}

#[test]
fn proxy_rejects_truncated_browser_frame_payload() {
    let mut input = 4_u32.to_ne_bytes().to_vec();
    input.extend_from_slice(b"{}");
    let (status, output) = run_proxy_with_closed_input(&input);

    assert!(
        matches!(status.code(), Some(code) if code != 0),
        "truncated frame payload should fail, got {status}"
    );
    assert!(
        output.is_empty(),
        "truncated payload produced browser output"
    );
}
