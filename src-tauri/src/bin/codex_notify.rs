//! `notify` shim for Codex CLI.
//!
//! Codex only supports a single `notify` program and appends the event JSON as
//! the last argument. This forwards that JSON to a running Signalpost and
//! then runs whatever `notify` program was configured before, so installing it
//! cannot take the slot away from something already using it.
//!
//! Usage, as written into `~/.codex/config.toml`:
//!
//! ```toml
//! notify = ["signalpost-codex.exe", "--token", "…", "--chain", "original.exe", "its-arg"]
//! ```

// No console window: Codex runs this on every turn, and a flash each time
// would be worse than the notification is useful.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const CHAIN_FLAG: &str = "--chain";
const TOKEN_FLAG: &str = "--token";
const DEFAULT_PORT: u16 = 8787;
/// The panel is either up on this machine or it is not; there is nothing to
/// wait for beyond a local connection.
const TIMEOUT: Duration = Duration::from_millis(700);

fn port() -> u16 {
    std::env::var("SIGNALPOST_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// A single fixed localhost POST does not justify pulling in an HTTP client;
/// the startup cost of one would be paid on every Codex turn.
fn post(path: &str, body: &str) -> std::io::Result<()> {
    let addr = format!("127.0.0.1:{}", port())
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::other("no address"))?;
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT)?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    stream.set_read_timeout(Some(TIMEOUT))?;

    let request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: 127.0.0.1\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    // Waiting for the reply is what makes the send reliable: this process
    // exits the moment `post` returns, and tearing the socket down before the
    // server has read the request loses it.
    let mut response = Vec::new();
    let _ = stream.take(512).read_to_end(&mut response);
    Ok(())
}

/// What a Codex command line breaks down into: the token to post with, the
/// program to run afterwards with its own arguments, and the event JSON.
type Invocation = (
    Option<String>,
    Option<(String, Vec<String>)>,
    Option<String>,
);

/// Splits `--token <secret>` and `--chain <program> [args…]` off the front,
/// leaving the event JSON Codex appended as the last argument.
fn split(args: Vec<String>) -> Invocation {
    let mut args = args;
    let json = args.pop();

    let mut token = None;
    if args.first().map(String::as_str) == Some(TOKEN_FLAG) && args.len() >= 2 {
        token = Some(args[1].clone());
        args.drain(..2);
    }

    if args.first().map(String::as_str) == Some(CHAIN_FLAG) && args.len() >= 2 {
        let program = args[1].clone();
        let rest = args[2..].to_vec();
        return (token, Some((program, rest)), json);
    }
    (token, None, json)
}

fn main() {
    let (token, chain, json) = split(std::env::args().skip(1).collect());

    if let Some(body) = &json {
        // Without a token the post is answered with 404, which is the same
        // outcome as the panel not running: the chained program still runs.
        let path = format!("/hook/{}/codex", token.unwrap_or_default());
        // A panel that is not running is the normal case, not an error.
        let _ = post(&path, body);
    }

    // Run the original program last, and pass the event through unchanged so
    // it sees exactly what Codex would have given it.
    if let Some((program, mut args)) = chain {
        if let Some(body) = json {
            args.push(body);
        }
        let _ = std::process::Command::new(program).args(args).status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_event_json_is_the_last_argument() {
        let (token, chain, json) = split(v(&["{\"type\":\"x\"}"]));
        assert!(token.is_none());
        assert!(chain.is_none());
        assert_eq!(json.as_deref(), Some("{\"type\":\"x\"}"));
    }

    #[test]
    fn a_chained_program_keeps_its_own_arguments() {
        let (_, chain, json) = split(v(&["--chain", "orig.exe", "turn-ended", "{}"]));
        let (program, args) = chain.unwrap();
        assert_eq!(program, "orig.exe");
        assert_eq!(args, v(&["turn-ended"]));
        assert_eq!(json.as_deref(), Some("{}"));
    }

    #[test]
    fn a_chain_flag_without_a_program_is_ignored_rather_than_panicking() {
        let (_, chain, json) = split(v(&["--chain", "{}"]));
        assert!(chain.is_none());
        assert_eq!(json.as_deref(), Some("{}"));
    }

    #[test]
    fn the_token_is_taken_off_the_front_and_the_chain_still_parses() {
        let (token, chain, json) = split(v(&[
            "--token",
            "abc123",
            "--chain",
            "orig.exe",
            "turn-ended",
            "{}",
        ]));
        assert_eq!(token.as_deref(), Some("abc123"));
        let (program, args) = chain.unwrap();
        assert_eq!(program, "orig.exe");
        assert_eq!(args, v(&["turn-ended"]));
        assert_eq!(json.as_deref(), Some("{}"));
    }

    /// An entry written by an older build has no token; it must still run
    /// whatever was chained behind it rather than fall over.
    #[test]
    fn a_registration_without_a_token_still_chains() {
        let (token, chain, json) = split(v(&["--chain", "orig.exe", "{}"]));
        assert!(token.is_none());
        assert!(chain.is_some());
        assert_eq!(json.as_deref(), Some("{}"));
    }
}
