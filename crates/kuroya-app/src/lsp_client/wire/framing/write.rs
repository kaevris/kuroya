use kuroya_core::{ContentChange, TextSnapshot, lsp::path_to_file_uri};
use serde_json::Value;
use std::{
    io::{self, IoSlice},
    path::Path,
};
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    process::ChildStdin,
};

const CONTENT_LENGTH_PREFIX: &[u8] = b"Content-Length: ";
const HEADER_SUFFIX: &[u8] = b"\r\n\r\n";
const CONTENT_LENGTH_HEADER_CAPACITY: usize =
    CONTENT_LENGTH_PREFIX.len() + CONTENT_LENGTH_DIGIT_CAPACITY + HEADER_SUFFIX.len();
const CONTENT_LENGTH_DIGIT_CAPACITY: usize = 20;
const JSON_TEXT_WRITE_BUFFER_CAPACITY: usize = 8 * 1024;

pub(in crate::lsp_client) async fn write_message(
    stdin: &mut ChildStdin,
    value: &Value,
) -> anyhow::Result<()> {
    let body = serde_json::to_vec(value)?;
    let (header, header_len) = content_length_header(body.len());
    write_frame(stdin, &header[..header_len], &body).await?;
    stdin.flush().await?;
    Ok(())
}

pub(in crate::lsp_client) async fn write_did_open_full_document(
    stdin: &mut ChildStdin,
    path: &Path,
    language_id: &str,
    version: i32,
    text: &TextSnapshot,
) -> anyhow::Result<()> {
    let uri = path_to_file_uri(path);
    let (prefix, suffix) = did_open_full_document_body_parts(&uri, language_id, version);
    write_full_document_text_message(stdin, prefix.as_bytes(), text, suffix.as_bytes()).await
}

pub(in crate::lsp_client) async fn write_did_change_full_document(
    stdin: &mut ChildStdin,
    path: &Path,
    version: i32,
    text: &TextSnapshot,
) -> anyhow::Result<()> {
    let uri = path_to_file_uri(path);
    let (prefix, suffix) = did_change_full_document_body_parts(&uri, version);
    write_full_document_text_message(stdin, prefix.as_bytes(), text, suffix.as_bytes()).await
}

/// Writes an incremental `textDocument/didChange` carrying a single ranged
/// content change (`rangeLength` is omitted; it is optional and redundant).
pub(in crate::lsp_client) async fn write_did_change_incremental(
    stdin: &mut ChildStdin,
    path: &Path,
    version: i32,
    change: &ContentChange,
) -> anyhow::Result<()> {
    let uri = path_to_file_uri(path);
    let (prefix, suffix) = did_change_incremental_body_parts(&uri, version, change);
    write_json_escaped_text_message(
        stdin,
        prefix.as_bytes(),
        json_escaped_str_content_len(&change.text),
        std::iter::once(change.text.as_str()),
        suffix.as_bytes(),
    )
    .await
}

async fn write_full_document_text_message<W>(
    writer: &mut W,
    prefix: &[u8],
    text: &TextSnapshot,
    suffix: &[u8],
) -> anyhow::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    write_json_escaped_text_message(
        writer,
        prefix,
        json_escaped_snapshot_content_len(text)?,
        text.chunks(),
        suffix,
    )
    .await
}

/// Streams a frame whose body is `prefix` + JSON-escaped text chunks +
/// `suffix`, with a Content-Length header matching the exact byte count.
async fn write_json_escaped_text_message<'a, W, I>(
    writer: &mut W,
    prefix: &[u8],
    text_len: usize,
    chunks: I,
    suffix: &[u8],
) -> anyhow::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
    I: Iterator<Item = &'a str>,
{
    let content_length = checked_content_length_add(prefix.len(), text_len)?;
    let content_length = checked_content_length_add(content_length, suffix.len())?;
    let (header, header_len) = content_length_header(content_length);

    write_frame(writer, &header[..header_len], prefix).await?;
    let mut text_scratch = Vec::with_capacity(JSON_TEXT_WRITE_BUFFER_CAPACITY);
    for chunk in chunks {
        write_json_escaped_str_content(writer, chunk, &mut text_scratch).await?;
    }
    flush_json_text_scratch(writer, &mut text_scratch).await?;
    writer.write_all(suffix).await?;
    writer.flush().await?;

    Ok(())
}

async fn write_frame<W>(writer: &mut W, header: &[u8], body: &[u8]) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    if !writer.is_write_vectored() {
        writer.write_all(header).await?;
        writer.write_all(body).await?;
        return Ok(());
    }

    let mut header_written = 0;
    let mut body_written = 0;

    while header_written < header.len() || body_written < body.len() {
        let buffers = [
            IoSlice::new(&header[header_written..]),
            IoSlice::new(&body[body_written..]),
        ];
        let buffers = if header_written == header.len() {
            &buffers[1..]
        } else {
            &buffers[..]
        };

        let written = writer.write_vectored(buffers).await?;
        if written == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "failed to write LSP frame",
            ));
        }

        advance_frame_offsets(
            written,
            header,
            body,
            &mut header_written,
            &mut body_written,
        )?;
    }

    Ok(())
}

fn advance_frame_offsets(
    mut written: usize,
    header: &[u8],
    body: &[u8],
    header_written: &mut usize,
    body_written: &mut usize,
) -> io::Result<()> {
    let header_advance = if *header_written < header.len() {
        let header_remaining = header.len() - *header_written;
        let header_advance = header_remaining.min(written);
        written -= header_advance;
        header_advance
    } else {
        0
    };

    let body_remaining = body.len() - *body_written;
    if written > body_remaining {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "writer reported more bytes than the LSP frame contains",
        ));
    }

    *header_written += header_advance;
    *body_written += written;

    Ok(())
}

fn did_open_full_document_body_parts(
    uri: &str,
    language_id: &str,
    version: i32,
) -> (String, String) {
    let mut prefix = String::from(
        "{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didOpen\",\"params\":{\"textDocument\":{\"languageId\":",
    );
    push_json_string_literal(&mut prefix, language_id);
    prefix.push_str(",\"text\":\"");

    let mut suffix = String::from("\",\"uri\":");
    push_json_string_literal(&mut suffix, uri);
    suffix.push_str(",\"version\":");
    suffix.push_str(&version.to_string());
    suffix.push_str("}}}");

    (prefix, suffix)
}

fn did_change_full_document_body_parts(uri: &str, version: i32) -> (String, String) {
    let prefix = String::from(
        "{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{\"contentChanges\":[{\"text\":\"",
    );

    let mut suffix = String::from("\"}],\"textDocument\":{\"uri\":");
    push_json_string_literal(&mut suffix, uri);
    suffix.push_str(",\"version\":");
    suffix.push_str(&version.to_string());
    suffix.push_str("}}}");

    (prefix, suffix)
}

fn did_change_incremental_body_parts(
    uri: &str,
    version: i32,
    change: &ContentChange,
) -> (String, String) {
    let mut prefix = String::from(
        "{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/didChange\",\"params\":{\"contentChanges\":[{\"range\":{\"start\":{\"line\":",
    );
    prefix.push_str(&change.start_line.to_string());
    prefix.push_str(",\"character\":");
    prefix.push_str(&change.start_character.to_string());
    prefix.push_str("},\"end\":{\"line\":");
    prefix.push_str(&change.end_line.to_string());
    prefix.push_str(",\"character\":");
    prefix.push_str(&change.end_character.to_string());
    prefix.push_str("}},\"text\":\"");

    let mut suffix = String::from("\"}],\"textDocument\":{\"uri\":");
    push_json_string_literal(&mut suffix, uri);
    suffix.push_str(",\"version\":");
    suffix.push_str(&version.to_string());
    suffix.push_str("}}}");

    (prefix, suffix)
}

fn json_escaped_snapshot_content_len(text: &TextSnapshot) -> anyhow::Result<usize> {
    text.chunks().try_fold(0, |len, chunk| {
        checked_content_length_add(len, json_escaped_str_content_len(chunk))
    })
}

fn json_escaped_str_content_len(value: &str) -> usize {
    value
        .chars()
        .map(|ch| json_escape(ch).map_or_else(|| ch.len_utf8(), JsonEscape::len))
        .sum()
}

fn checked_content_length_add(lhs: usize, rhs: usize) -> anyhow::Result<usize> {
    lhs.checked_add(rhs)
        .ok_or_else(|| anyhow::anyhow!("LSP message content length overflows usize"))
}

async fn write_json_escaped_str_content<W>(
    writer: &mut W,
    value: &str,
    scratch: &mut Vec<u8>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let mut unescaped_start = 0;
    for (idx, ch) in value.char_indices() {
        let Some(escape) = json_escape(ch) else {
            continue;
        };

        if unescaped_start < idx {
            push_json_text_bytes(writer, scratch, &value.as_bytes()[unescaped_start..idx]).await?;
        }
        push_json_escape_bytes(writer, scratch, escape).await?;
        unescaped_start = idx + ch.len_utf8();
    }

    if unescaped_start < value.len() {
        push_json_text_bytes(writer, scratch, &value.as_bytes()[unescaped_start..]).await?;
    }

    Ok(())
}

async fn push_json_text_bytes<W>(
    writer: &mut W,
    scratch: &mut Vec<u8>,
    mut bytes: &[u8],
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    if bytes.len() >= JSON_TEXT_WRITE_BUFFER_CAPACITY {
        flush_json_text_scratch(writer, scratch).await?;
        writer.write_all(bytes).await?;
        return Ok(());
    }

    while !bytes.is_empty() {
        if scratch.len() == JSON_TEXT_WRITE_BUFFER_CAPACITY {
            flush_json_text_scratch(writer, scratch).await?;
        }

        let available = JSON_TEXT_WRITE_BUFFER_CAPACITY - scratch.len();
        let byte_count = available.min(bytes.len());
        scratch.extend_from_slice(&bytes[..byte_count]);
        bytes = &bytes[byte_count..];
    }

    Ok(())
}

async fn push_json_escape_bytes<W>(
    writer: &mut W,
    scratch: &mut Vec<u8>,
    escape: JsonEscape,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    match escape {
        JsonEscape::Static(bytes) => push_json_text_bytes(writer, scratch, bytes.as_bytes()).await,
        JsonEscape::Unicode(byte) => {
            let mut bytes = *b"\\u00xx";
            write_hex_escape_digits(&mut bytes, byte);
            push_json_text_bytes(writer, scratch, &bytes).await
        }
    }
}

async fn flush_json_text_scratch<W>(writer: &mut W, scratch: &mut Vec<u8>) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    if !scratch.is_empty() {
        writer.write_all(scratch.as_slice()).await?;
        scratch.clear();
    }
    Ok(())
}

fn push_json_string_literal(output: &mut String, value: &str) {
    output.push('"');
    push_json_escaped_str_content(output, value);
    output.push('"');
}

fn push_json_escaped_str_content(output: &mut String, value: &str) {
    let mut unescaped_start = 0;
    for (idx, ch) in value.char_indices() {
        let Some(escape) = json_escape(ch) else {
            continue;
        };

        if unescaped_start < idx {
            output.push_str(&value[unescaped_start..idx]);
        }
        push_json_escape(output, escape);
        unescaped_start = idx + ch.len_utf8();
    }

    if unescaped_start < value.len() {
        output.push_str(&value[unescaped_start..]);
    }
}

fn push_json_escape(output: &mut String, escape: JsonEscape) {
    match escape {
        JsonEscape::Static(bytes) => output.push_str(bytes),
        JsonEscape::Unicode(byte) => {
            let mut bytes = *b"\\u00xx";
            write_hex_escape_digits(&mut bytes, byte);
            output.push_str(std::str::from_utf8(&bytes).expect("JSON escape is valid UTF-8"));
        }
    }
}

fn write_hex_escape_digits(output: &mut [u8; 6], byte: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    output[4] = HEX[(byte >> 4) as usize];
    output[5] = HEX[(byte & 0x0f) as usize];
}

#[derive(Clone, Copy)]
enum JsonEscape {
    Static(&'static str),
    Unicode(u8),
}

impl JsonEscape {
    fn len(self) -> usize {
        match self {
            Self::Static(bytes) => bytes.len(),
            Self::Unicode(_) => 6,
        }
    }
}

fn json_escape(ch: char) -> Option<JsonEscape> {
    match ch {
        '"' => Some(JsonEscape::Static("\\\"")),
        '\\' => Some(JsonEscape::Static("\\\\")),
        '\u{08}' => Some(JsonEscape::Static("\\b")),
        '\t' => Some(JsonEscape::Static("\\t")),
        '\n' => Some(JsonEscape::Static("\\n")),
        '\u{0c}' => Some(JsonEscape::Static("\\f")),
        '\r' => Some(JsonEscape::Static("\\r")),
        '\u{00}'..='\u{1f}' => Some(JsonEscape::Unicode(ch as u8)),
        _ => None,
    }
}

fn content_length_header(content_length: usize) -> ([u8; CONTENT_LENGTH_HEADER_CAPACITY], usize) {
    let mut header = [0; CONTENT_LENGTH_HEADER_CAPACITY];
    let mut len = 0;

    header[..CONTENT_LENGTH_PREFIX.len()].copy_from_slice(CONTENT_LENGTH_PREFIX);
    len += CONTENT_LENGTH_PREFIX.len();

    let mut digits = [0; CONTENT_LENGTH_DIGIT_CAPACITY];
    let mut digit_count = 0;
    let mut remaining = content_length;
    loop {
        digits[digit_count] = b'0' + (remaining % 10) as u8;
        digit_count += 1;
        remaining /= 10;
        if remaining == 0 {
            break;
        }
    }

    for digit in digits[..digit_count].iter().rev() {
        header[len] = *digit;
        len += 1;
    }

    header[len..len + HEADER_SUFFIX.len()].copy_from_slice(HEADER_SUFFIX);
    len += HEADER_SUFFIX.len();

    (header, len)
}

#[cfg(test)]
mod tests {
    use super::{
        HEADER_SUFFIX, JSON_TEXT_WRITE_BUFFER_CAPACITY, advance_frame_offsets,
        content_length_header, did_change_full_document_body_parts,
        did_change_incremental_body_parts, did_open_full_document_body_parts,
        flush_json_text_scratch, json_escaped_str_content_len, write_frame,
        write_full_document_text_message, write_json_escaped_str_content,
        write_json_escaped_text_message,
    };
    use kuroya_core::{ContentChange, TextBuffer, TextSnapshot};
    use serde_json::{Value, json};
    use std::{
        io::{self, IoSlice},
        pin::Pin,
        str,
        task::{Context, Poll},
    };
    use tokio::io::AsyncWrite;

    #[test]
    fn content_length_header_formats_zero_and_multi_digit_lengths() {
        for (length, expected) in [
            (0, "Content-Length: 0\r\n\r\n"),
            (42, "Content-Length: 42\r\n\r\n"),
            (123_456, "Content-Length: 123456\r\n\r\n"),
        ] {
            let (header, header_len) = content_length_header(length);

            assert_eq!(&header[..header_len], expected.as_bytes());
        }
    }

    #[test]
    fn content_length_header_formats_max_usize() {
        let expected = format!("Content-Length: {}\r\n\r\n", usize::MAX);

        let (header, header_len) = content_length_header(usize::MAX);

        assert_eq!(&header[..header_len], expected.as_bytes());
    }

    #[test]
    fn advance_frame_offsets_tracks_crossing_header_boundary() {
        let header = b"Content-Length: 2\r\n\r\n";
        let body = b"{}";
        let mut header_written = 0;
        let mut body_written = 0;

        advance_frame_offsets(
            header.len() + 1,
            header,
            body,
            &mut header_written,
            &mut body_written,
        )
        .expect("reported bytes fit within frame");

        assert_eq!(header_written, header.len());
        assert_eq!(body_written, 1);
    }

    #[test]
    fn advance_frame_offsets_rejects_impossible_byte_count() {
        let header = b"Content-Length: 2\r\n\r\n";
        let body = b"{}";
        let mut header_written = header.len();
        let mut body_written = 1;

        let error = advance_frame_offsets(2, header, body, &mut header_written, &mut body_written)
            .expect_err("reported bytes exceed remaining frame");

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(header_written, header.len());
        assert_eq!(body_written, 1);
    }

    #[test]
    fn json_escaped_str_content_len_matches_serde_json_string_body_len() {
        let text = "plain \"quote\" backslash \\ slash / \n\r\t\u{08}\u{0c}\u{01}\u{1f}\u{1f642}";
        let serialized = serde_json::to_string(text).expect("text should serialize");

        assert_eq!(json_escaped_str_content_len(text), serialized.len() - 2);
    }

    #[tokio::test]
    async fn write_json_escaped_str_content_writes_expected_escapes() {
        let mut writer = recording_writer(3, false);
        let mut scratch = Vec::with_capacity(JSON_TEXT_WRITE_BUFFER_CAPACITY);

        write_json_escaped_str_content(&mut writer, "a\"\\\n\t\u{01}\u{1f}z", &mut scratch)
            .await
            .expect("escaped string content should write");
        flush_json_text_scratch(&mut writer, &mut scratch)
            .await
            .expect("escaped string scratch should flush");

        assert_eq!(
            str::from_utf8(&writer.bytes).expect("escaped output should be UTF-8"),
            "a\\\"\\\\\\n\\t\\u0001\\u001fz"
        );
    }

    #[tokio::test]
    async fn write_full_document_text_message_sets_content_length_to_emitted_body_bytes() {
        let text = "line \"one\"\npath \\\ncontrol \u{01}\nface \u{1f642}";
        let snapshot = text_snapshot(text);
        let mut writer = recording_writer(7, true);

        write_full_document_text_message(&mut writer, b"{\"text\":\"", &snapshot, b"\"}")
            .await
            .expect("full document text message should write");

        let (content_length, body) = frame_body(&writer.bytes);
        assert_eq!(content_length, body.len());
        assert_eq!(
            serde_json::from_slice::<Value>(body).expect("body should be valid JSON"),
            json!({ "text": text })
        );
        assert_eq!(writer.flushes, 1);
        assert!(writer.vectored_writes > 0);
    }

    #[tokio::test]
    async fn write_full_document_text_message_batches_escaped_multi_chunk_snapshot() {
        let text = "\n".repeat(JSON_TEXT_WRITE_BUFFER_CAPACITY * 3);
        let snapshot = text_snapshot(&text);
        assert!(
            snapshot.chunks().count() > 1,
            "test must exercise a multi-chunk snapshot"
        );
        let mut writer = recording_writer(usize::MAX, true);

        write_full_document_text_message(&mut writer, b"{\"text\":\"", &snapshot, b"\"}")
            .await
            .expect("full document text message should write");

        let (content_length, body) = frame_body(&writer.bytes);
        assert_eq!(content_length, body.len());
        assert_eq!(
            serde_json::from_slice::<Value>(body).expect("body should be valid JSON"),
            json!({ "text": text })
        );
        assert!(
            writer.scalar_writes <= 8,
            "expected escaped text writes to be batched, got {} scalar writes",
            writer.scalar_writes
        );
    }

    #[tokio::test]
    async fn did_open_full_document_body_parts_write_valid_notification() {
        let text = "fn main() {\n    println!(\"hi\");\n}\n";
        let snapshot = text_snapshot(text);
        let (prefix, suffix) =
            did_open_full_document_body_parts("file:///workspace/src/main.rs", "rust", 12);
        let mut writer = recording_writer(9, true);

        write_full_document_text_message(
            &mut writer,
            prefix.as_bytes(),
            &snapshot,
            suffix.as_bytes(),
        )
        .await
        .expect("didOpen message should write");

        let (_, body) = frame_body(&writer.bytes);
        let value: Value = serde_json::from_slice(body).expect("didOpen body should be valid JSON");
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["method"], "textDocument/didOpen");
        assert_eq!(
            value["params"]["textDocument"]["uri"],
            "file:///workspace/src/main.rs"
        );
        assert_eq!(value["params"]["textDocument"]["languageId"], "rust");
        assert_eq!(value["params"]["textDocument"]["version"], 12);
        assert_eq!(value["params"]["textDocument"]["text"], text);
    }

    #[tokio::test]
    async fn did_change_full_document_body_parts_write_valid_notification() {
        let text = "let value = \"changed\";\n";
        let snapshot = text_snapshot(text);
        let (prefix, suffix) =
            did_change_full_document_body_parts("file:///workspace/src/main.rs", 13);
        let mut writer = recording_writer(9, true);

        write_full_document_text_message(
            &mut writer,
            prefix.as_bytes(),
            &snapshot,
            suffix.as_bytes(),
        )
        .await
        .expect("didChange message should write");

        let (_, body) = frame_body(&writer.bytes);
        let value: Value =
            serde_json::from_slice(body).expect("didChange body should be valid JSON");
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["method"], "textDocument/didChange");
        assert_eq!(
            value["params"]["textDocument"]["uri"],
            "file:///workspace/src/main.rs"
        );
        assert_eq!(value["params"]["textDocument"]["version"], 13);
        assert_eq!(value["params"]["contentChanges"][0]["text"], text);
    }

    #[tokio::test]
    async fn did_change_incremental_body_parts_write_valid_ranged_notification() {
        let change = ContentChange {
            start_line: 1,
            start_character: 6,
            end_line: 2,
            end_character: 0,
            text: "brave \"world\"\n".to_owned(),
        };
        let (prefix, suffix) =
            did_change_incremental_body_parts("file:///workspace/src/main.rs", 14, &change);
        let mut writer = recording_writer(9, true);

        write_json_escaped_text_message(
            &mut writer,
            prefix.as_bytes(),
            json_escaped_str_content_len(&change.text),
            std::iter::once(change.text.as_str()),
            suffix.as_bytes(),
        )
        .await
        .expect("incremental didChange message should write");

        let (_, body) = frame_body(&writer.bytes);
        let value: Value = serde_json::from_slice(body).expect("body should be valid JSON");
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["method"], "textDocument/didChange");
        assert_eq!(
            value["params"]["textDocument"]["uri"],
            "file:///workspace/src/main.rs"
        );
        assert_eq!(value["params"]["textDocument"]["version"], 14);
        assert_eq!(
            value["params"]["contentChanges"][0]["range"]["start"],
            json!({"line": 1, "character": 6})
        );
        assert_eq!(
            value["params"]["contentChanges"][0]["range"]["end"],
            json!({"line": 2, "character": 0})
        );
        assert_eq!(
            value["params"]["contentChanges"][0]["text"],
            "brave \"world\"\n"
        );
        // rangeLength is optional and intentionally omitted.
        assert!(
            value["params"]["contentChanges"][0]
                .get("rangeLength")
                .is_none()
        );
    }

    #[tokio::test]
    async fn write_frame_uses_vectored_writes_for_partial_frame_progress() {
        let mut writer = recording_writer(4, true);

        write_frame(&mut writer, b"abc", b"defgh")
            .await
            .expect("frame should write");

        assert_eq!(writer.bytes, b"abcdefgh");
        assert_eq!(writer.scalar_writes, 0);
        assert!(writer.vectored_writes > 1);
    }

    #[tokio::test]
    async fn write_frame_falls_back_when_vectored_writes_are_not_supported() {
        let mut writer = recording_writer(4, false);

        write_frame(&mut writer, b"abc", b"defgh")
            .await
            .expect("frame should write");

        assert_eq!(writer.bytes, b"abcdefgh");
        assert!(writer.scalar_writes > 1);
        assert_eq!(writer.vectored_writes, 0);
    }

    struct RecordingWriter {
        max_write: usize,
        bytes: Vec<u8>,
        scalar_writes: usize,
        vectored_writes: usize,
        flushes: usize,
        supports_vectored: bool,
    }

    fn recording_writer(max_write: usize, supports_vectored: bool) -> RecordingWriter {
        RecordingWriter {
            max_write,
            bytes: Vec::new(),
            scalar_writes: 0,
            vectored_writes: 0,
            flushes: 0,
            supports_vectored,
        }
    }

    fn text_snapshot(text: &str) -> TextSnapshot {
        TextBuffer::from_text(1, None, text.to_owned()).text_snapshot()
    }

    fn frame_body(bytes: &[u8]) -> (usize, &[u8]) {
        let separator = bytes
            .windows(HEADER_SUFFIX.len())
            .position(|window| window == HEADER_SUFFIX)
            .expect("frame should contain header separator");
        let header = str::from_utf8(&bytes[..separator]).expect("header should be UTF-8");
        let content_length = header
            .strip_prefix("Content-Length: ")
            .expect("header should contain Content-Length")
            .parse::<usize>()
            .expect("Content-Length should parse");
        let body = &bytes[separator + HEADER_SUFFIX.len()..];

        assert_eq!(body.len(), content_length);

        (content_length, body)
    }

    impl AsyncWrite for RecordingWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.scalar_writes += 1;
            let written = self.max_write.min(buf.len());
            self.bytes.extend_from_slice(&buf[..written]);
            Poll::Ready(Ok(written))
        }

        fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.flushes += 1;
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_write_vectored(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            bufs: &[IoSlice<'_>],
        ) -> Poll<io::Result<usize>> {
            self.vectored_writes += 1;
            let mut remaining = self.max_write;
            let mut written = 0;

            for buf in bufs {
                if remaining == 0 {
                    break;
                }

                let chunk_len = remaining.min(buf.len());
                self.bytes.extend_from_slice(&buf[..chunk_len]);
                written += chunk_len;
                remaining -= chunk_len;
            }

            Poll::Ready(Ok(written))
        }

        fn is_write_vectored(&self) -> bool {
            self.supports_vectored
        }
    }
}
