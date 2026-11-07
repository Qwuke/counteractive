//! Server-sent events: a bounded splitter for reading, and an encoder.
//!
//! The gate reads SSE from the engine, and the wallet reads it from the
//! gate, so both read data a peer controls. The splitter therefore bounds
//! how many bytes one event may hold, including bytes that haven't yet
//! formed a full line, so a peer can't grow a buffer without limit.
//!
//! Lines end in `\n` or `\r\n`. A bare `\r` is a valid terminator in the
//! SSE standard, but no sender here emits one, so it isn't treated as a
//! line end: it stays in the line, and a stream using only `\r` runs into
//! the size bound instead of being misread.

/// The gate's receipt event (SPEC section 5).
pub const EVENT_RECEIPT: &str = "receipt";
/// The gate's final event, with `data: {}`.
pub const EVENT_DONE: &str = "done";
/// The data line an OpenAI-compatible engine ends its stream with.
pub const OPENAI_DONE: &str = "[DONE]";

/// One dispatched event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event:` field, if the event had one.
    pub event: Option<String>,
    /// The `data:` lines, joined with `\n`.
    pub data: String,
}

impl SseEvent {
    /// An unnamed event: the default `message` type.
    pub fn data(data: impl Into<String>) -> Self {
        Self { event: None, data: data.into() }
    }

    /// A named event.
    pub fn named(event: impl Into<String>, data: impl Into<String>) -> Self {
        Self { event: Some(event.into()), data: data.into() }
    }

    /// Whether this event has the given name.
    pub fn is(&self, name: &str) -> bool {
        self.event.as_deref() == Some(name)
    }

    /// The wire form, ending in the blank line that dispatches it.
    pub fn encode(&self) -> String {
        let mut out = String::with_capacity(self.data.len() + 32);
        if let Some(event) = &self.event {
            out.push_str("event: ");
            out.push_str(event);
            out.push('\n');
        }
        for line in self.data.split('\n') {
            out.push_str("data: ");
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
        out
    }
}

/// Why a stream couldn't be split.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SseError {
    /// One event, or an unfinished line, grew past the bound.
    #[error("SSE event exceeds {limit} bytes")]
    TooLarge {
        /// The bound in bytes.
        limit: usize,
    },
    /// A line wasn't UTF-8.
    #[error("SSE line is not UTF-8")]
    InvalidUtf8,
    /// The stream ended partway through an event.
    #[error("SSE stream ended mid-event")]
    Truncated,
}

/// Splits a byte stream into events, holding at most `max_event_bytes` per event.
#[derive(Debug)]
pub struct SseSplitter {
    max_event_bytes: usize,
    /// Bytes after the last line end.
    partial: Vec<u8>,
    /// Bytes of complete lines in the event being built.
    event_bytes: usize,
    event: Option<String>,
    data: Option<String>,
}

impl SseSplitter {
    /// A splitter that rejects any event larger than `max_event_bytes`.
    pub fn new(max_event_bytes: usize) -> Self {
        Self { max_event_bytes, partial: Vec::new(), event_bytes: 0, event: None, data: None }
    }

    /// Feeds bytes in, returning every event they complete.
    ///
    /// After an error the splitter's state is unspecified; drop the stream.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, SseError> {
        let mut events = Vec::new();
        let mut rest = bytes;
        while let Some(end) = rest.iter().position(|&byte| byte == b'\n') {
            let (head, tail) = rest.split_at(end);
            rest = &tail[1..];
            self.check_size(self.partial.len() + head.len())?;
            let line = if self.partial.is_empty() {
                Self::line_from(head)?
            } else {
                self.partial.extend_from_slice(head);
                let joined = std::mem::take(&mut self.partial);
                Self::line_from(&joined)?
            };
            if let Some(event) = self.take_line(&line) {
                events.push(event);
            }
        }
        self.check_size(self.partial.len() + rest.len())?;
        self.partial.extend_from_slice(rest);
        Ok(events)
    }

    /// Ends the stream. An unfinished event is an error, since a sender
    /// here always ends each event with a blank line.
    pub fn finish(self) -> Result<(), SseError> {
        if self.partial.is_empty() && self.event_bytes == 0 { Ok(()) } else { Err(SseError::Truncated) }
    }

    fn check_size(&self, pending_line: usize) -> Result<(), SseError> {
        if self.event_bytes + pending_line > self.max_event_bytes {
            Err(SseError::TooLarge { limit: self.max_event_bytes })
        } else {
            Ok(())
        }
    }

    fn line_from(raw: &[u8]) -> Result<String, SseError> {
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        String::from_utf8(raw.to_vec()).map_err(|_| SseError::InvalidUtf8)
    }

    /// Applies one line; a blank line dispatches the event built so far.
    fn take_line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            self.event_bytes = 0;
            let event = self.event.take();
            return self.data.take().map(|data| SseEvent { event, data });
        }
        self.event_bytes += line.len() + 1;
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_owned()),
            "data" => match &mut self.data {
                Some(data) => {
                    data.push('\n');
                    data.push_str(value);
                }
                None => self.data = Some(value.to_owned()),
            },
            // `id` and `retry` mean nothing here; other fields are ignored per the standard.
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split_all(input: &[u8], limit: usize) -> Result<Vec<SseEvent>, SseError> {
        let mut splitter = SseSplitter::new(limit);
        let events = splitter.push(input)?;
        splitter.finish()?;
        Ok(events)
    }

    #[test]
    fn splits_the_gate_stream() {
        let stream = concat!(
            "data: {\"choices\":[]}\n\n",
            "event: receipt\ndata: {\"turn\":1}\n\n",
            "event: done\ndata: {}\n\n",
        );
        let events = split_all(stream.as_bytes(), 1024).unwrap();
        assert_eq!(
            events,
            vec![
                SseEvent::data("{\"choices\":[]}"),
                SseEvent::named(EVENT_RECEIPT, "{\"turn\":1}"),
                SseEvent::named(EVENT_DONE, "{}"),
            ]
        );
    }

    #[test]
    fn handles_crlf_comments_and_multiline_data() {
        let stream = b": keepalive\r\ndata: a\r\ndata: b\r\nid: 7\r\n\r\n";
        assert_eq!(split_all(stream, 1024).unwrap(), vec![SseEvent::data("a\nb")]);
    }

    #[test]
    fn any_chunking_gives_the_same_events() {
        let stream = "event: receipt\ndata: {\"x\":1}\n\ndata: [DONE]\n\n".as_bytes();
        let whole = split_all(stream, 1024).unwrap();
        for size in 1..stream.len() {
            let mut splitter = SseSplitter::new(1024);
            let mut events = Vec::new();
            for chunk in stream.chunks(size) {
                events.extend(splitter.push(chunk).unwrap());
            }
            splitter.finish().unwrap();
            assert_eq!(events, whole, "chunk size {size}");
        }
    }

    #[test]
    fn encode_round_trips() {
        let event = SseEvent::named("receipt", "line one\nline two");
        assert_eq!(event.encode(), "event: receipt\ndata: line one\ndata: line two\n\n");
        assert_eq!(split_all(event.encode().as_bytes(), 1024).unwrap(), vec![event]);
    }

    #[test]
    fn bounds_unterminated_lines_and_long_events() {
        let mut splitter = SseSplitter::new(16);
        assert_eq!(splitter.push(&[b'x'; 17]), Err(SseError::TooLarge { limit: 16 }));

        let mut splitter = SseSplitter::new(16);
        splitter.push(b"data: 0123456\n").unwrap();
        assert_eq!(splitter.push(b"data: 7\n"), Err(SseError::TooLarge { limit: 16 }));

        // The bound is per event, so many small events pass.
        let many = "data: 1\n\n".repeat(100);
        assert_eq!(split_all(many.as_bytes(), 16).unwrap().len(), 100);
    }

    #[test]
    fn rejects_bad_utf8_and_truncation() {
        assert_eq!(split_all(b"data: \xff\n\n", 64), Err(SseError::InvalidUtf8));
        assert_eq!(split_all(b"data: half\n", 64), Err(SseError::Truncated));
        assert_eq!(split_all(b"data: no newline", 64), Err(SseError::Truncated));
    }

    #[test]
    fn events_without_data_are_not_dispatched() {
        assert_eq!(split_all(b"event: ping\n\n", 64).unwrap(), vec![]);
    }
}
