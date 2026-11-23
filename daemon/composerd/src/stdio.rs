use crate::hub::Hub;
use crate::rpc::{error_line, handle_line, RpcError};
use std::io::{BufRead, Read, Write};

pub(crate) enum Line {
    Eof,
    Text(String),
    TooLong,
}

/// Reads one line of at most `max` bytes; an oversized line is skipped to its end.
pub(crate) fn read_bounded(r: &mut impl BufRead, max: usize) -> std::io::Result<Line> {
    let mut buf = Vec::new();
    let limit = max as u64 + 1;
    r.by_ref().take(limit).read_until(b'\n', &mut buf)?;
    if buf.is_empty() {
        return Ok(Line::Eof);
    }
    let ended = buf.last() == Some(&b'\n');
    if ended {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
    }
    if buf.len() <= max && (ended || buf.len() < max + 1) {
        return Ok(Line::Text(String::from_utf8_lossy(&buf).into_owned()));
    }
    if ended {
        return Ok(Line::TooLong);
    }
    // Over the limit without a newline: drop the rest of the line.
    loop {
        buf.clear();
        r.by_ref().take(limit).read_until(b'\n', &mut buf)?;
        if buf.is_empty() || buf.last() == Some(&b'\n') {
            return Ok(Line::TooLong);
        }
    }
}

/// Serves one stdio client until the reader ends: one JSON message per line.
pub fn serve_stdio(
    hub: &Hub,
    max_message_bytes: usize,
    mut reader: impl BufRead,
    mut writer: impl Write + Send,
) -> std::io::Result<()> {
    let (client, rx) = hub
        .connect()
        .ok_or_else(|| std::io::Error::other("client limit reached"))?;
    std::thread::scope(|s| {
        let w = s.spawn(move || -> std::io::Result<()> {
            for msg in rx {
                writeln!(writer, "{msg}")?;
                writer.flush()?;
            }
            Ok(())
        });
        let result = (|| loop {
            match read_bounded(&mut reader, max_message_bytes)? {
                Line::Eof => return Ok(()),
                Line::Text(t) if t.trim().is_empty() => {}
                Line::Text(t) => handle_line(hub, &client, &t),
                Line::TooLong => {
                    let _ = client.tx.send(error_line(
                        serde_json::Value::Null,
                        &RpcError::invalid_request("message too large"),
                    ));
                }
            }
        })();
        hub.disconnect(&client);
        drop(client);
        let wrote = w
            .join()
            .unwrap_or_else(|_| Err(std::io::Error::other("writer panicked")));
        result.and(wrote)
    })
}
