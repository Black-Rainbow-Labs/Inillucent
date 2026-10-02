//! Fetching a URL through the `curl` program, the download path on macOS.
//!
//! Invariant: **curl only moves bytes.** The caller in `http.rs` still writes
//! them to a `.part` file, digests them, checks the length and renames the file
//! only when the digest matches. Nothing about what counts as installed changes
//! with the transport.
//!
//! ## Why macOS does not use the built in client
//!
//! The built in client gets TLS from the system OpenSSL, loaded with `dlopen`
//! (see `tls/unix.rs`). macOS has no public OpenSSL. Its `/usr/lib/libssl.dylib`
//! is a private copy, and macOS aborts any program that loads it by that name,
//! printing only `... is loading libcrypto in an unsafe way`. That is how
//! `inillucent setup-embeddings` ended on macOS in 2.0.3, before it had fetched
//! a byte. `/usr/bin/curl` is part of every macOS install, uses the system's
//! own TLS and trust store, and is kept current by Apple's updates. Using it
//! adds no dependency and no TLS code to this workspace.
//!
//! `INILLUCENT_HTTP=curl` selects this path on any system, and
//! `INILLUCENT_HTTP=builtin` selects the built in client on macOS. The tests
//! use the first to run this path on Windows and Linux, where `curl` is also
//! present.

use std::io::Read;
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};

use inillucent_base::error::refusal;
use inillucent_base::DbResult;

/// The environment variable that chooses the transport.
pub const TRANSPORT_VARIABLE: &str = "INILLUCENT_HTTP";

/// The most redirects curl follows, the same bound as the built in client.
const MAX_REDIRECTS: &str = "8";

/// How long curl may take to connect, in seconds.
const CONNECT_TIMEOUT: &str = "60";

/// The lowest speed, in bytes a second, below which a transfer counts as stalled.
const STALL_BYTES: &str = "1";

/// How long a transfer may stay below that speed, in seconds, before curl gives up.
const STALL_SECONDS: &str = "60";

/// Whether downloads go through curl on this system.
///
/// macOS by default, for the reason in the module comment, and anywhere
/// `INILLUCENT_HTTP=curl` is set.
pub fn selected() -> bool {
    match std::env::var(TRANSPORT_VARIABLE) {
        Ok(value) if value.trim().eq_ignore_ascii_case("curl") => true,
        Ok(value) if value.trim().eq_ignore_ascii_case("builtin") => false,
        _ => cfg!(target_os = "macos"),
    }
}

/// The curl program: the system's own on macOS, and the first on `PATH` elsewhere.
fn program() -> &'static str {
    if cfg!(target_os = "macos") {
        "/usr/bin/curl"
    } else {
        "curl"
    }
}

/// The arguments every request shares.
///
/// `--fail` turns an HTTP error into a non zero exit, so a 404 page is never
/// digested as if it were the file. An `https` URL may only redirect to another
/// `https` one, which is the rule the built in client applies.
/// @param url - what to fetch
fn common_arguments(url: &str) -> Vec<String> {
    let mut arguments: Vec<String> = [
        "--silent",
        "--show-error",
        "--fail",
        "--location",
        "--max-redirs",
        MAX_REDIRECTS,
        "--connect-timeout",
        CONNECT_TIMEOUT,
        "--speed-limit",
        STALL_BYTES,
        "--speed-time",
        STALL_SECONDS,
        "--user-agent",
        super::http::USER_AGENT,
        "--header",
        "Accept-Encoding: identity",
    ]
    .iter()
    .map(|argument| argument.to_string())
    .collect();
    if url.starts_with("https://") {
        arguments.push("--proto-redir".to_string());
        arguments.push("=https".to_string());
    }
    arguments
}

/// A response body arriving on curl's standard output.
pub struct Body {
    child: Child,
    stdout: ChildStdout,
    stderr: Option<ChildStderr>,
    url: String,
}

impl Body {
    /// Reads the next bytes of the body. Zero means curl has finished writing.
    ///
    /// @param out - where the bytes go
    pub fn read_body(&mut self, out: &mut [u8]) -> DbResult<usize> {
        self.stdout
            .read(out)
            .map_err(|error| refusal(format!("{}: reading from curl failed: {error}", self.url)))
    }

    /// Waits for curl and reports a failure it ended with.
    ///
    /// Called after the body has been read to its end. A transfer curl gave up
    /// on ends its output early, and its exit status is the only thing that
    /// says so when the server announced no length.
    pub fn finish(mut self) -> DbResult<()> {
        let mut message = String::new();
        if let Some(mut stderr) = self.stderr.take() {
            let _ = stderr.read_to_string(&mut message);
        }
        let status = self
            .child
            .wait()
            .map_err(|error| refusal(format!("{}: waiting for curl failed: {error}", self.url)))?;
        if status.success() {
            return Ok(());
        }
        let code = status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "a signal".to_string());
        Err(refusal(format!(
            "{} could not be fetched: curl ended with {code}: {}. Nothing was installed",
            self.url,
            message.trim()
        )))
    }

    /// Stops curl without waiting for the rest of the body.
    pub fn abandon(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts fetching a URL, with its body arriving through [`Body`].
///
/// @param url - what to fetch
pub fn open(url: &str) -> DbResult<Body> {
    let mut command = Command::new(program());
    command
        .args(common_arguments(url))
        .arg("--output")
        .arg("-")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        refusal(format!(
            "{url} could not be fetched: {} would not start ({error}). This system downloads \
             through curl; install it, or set {TRANSPORT_VARIABLE}=builtin",
            program()
        ))
    })?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return Err(refusal(format!("{url}: curl's output could not be read")));
    };
    let stderr = child.stderr.take();
    Ok(Body {
        child,
        stdout,
        stderr,
        url: url.to_string(),
    })
}

/// Asks the server how large the file is, without fetching it.
///
/// Only for the progress display and the length check. `None` when the server
/// did not say, or the request failed, in which case the digest is still the
/// check that matters.
/// @param url - what will be fetched
pub fn length(url: &str) -> Option<u64> {
    let output = Command::new(program())
        .args(common_arguments(url))
        .arg("--head")
        .arg(url)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // With `--location` every hop's headers are printed, so the last block is
    // the file's own.
    let last = text
        .rsplit("\r\n\r\n")
        .find(|block| !block.trim().is_empty())?;
    last.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            value.trim().parse::<u64>().ok()
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The redirect rule follows the scheme of the URL asked for.
    #[test]
    fn an_https_url_may_only_redirect_to_https() {
        let secure = common_arguments("https://example.com/file");
        assert!(secure
            .windows(2)
            .any(|pair| pair == ["--proto-redir", "=https"]));
        let plain = common_arguments("http://127.0.0.1:1/file");
        assert!(!plain.iter().any(|argument| argument == "--proto-redir"));
        assert!(secure.iter().any(|argument| argument == "--fail"));
    }
}
