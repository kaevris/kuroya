use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

#[cfg(test)]
pub(super) const MAX_LSP_URI_BYTES: usize = 16 * 1024;
#[cfg(not(test))]
const MAX_LSP_URI_BYTES: usize = 16 * 1024;

pub fn path_to_file_uri(path: &Path) -> String {
    let canonicalized_path = path.canonicalize().ok();
    let path = canonicalized_path
        .as_deref()
        .unwrap_or(path)
        .to_string_lossy();
    let path = if path.as_bytes().contains(&b'\\') {
        Cow::Owned(path.replace('\\', "/"))
    } else {
        path
    };

    #[cfg(windows)]
    let mut path = path;

    #[cfg(windows)]
    if let Some(verbatim_unc_path) = path.strip_prefix("//?/UNC/") {
        let mut normalized = String::with_capacity(2 + verbatim_unc_path.len());
        normalized.push_str("//");
        normalized.push_str(verbatim_unc_path);
        path = Cow::Owned(normalized);
    }
    #[cfg(windows)]
    if let Some(verbatim_path) = path.strip_prefix("//?/") {
        path = Cow::Owned(verbatim_path.to_owned());
    }

    #[cfg(windows)]
    if let Some(unc_path) = path.strip_prefix("//")
        && let Some((authority, path)) = unc_path.split_once('/')
        && !authority.is_empty()
        && !path.is_empty()
    {
        let mut uri =
            String::with_capacity("file://".len() + authority.len() + 1 + uri_path_capacity(path));
        uri.push_str("file://");
        uri.push_str(authority);
        uri.push('/');
        push_percent_encoded_uri_path(&mut uri, path);
        return uri;
    }

    let needs_leading_slash = !path.starts_with('/');
    let mut uri = String::with_capacity(
        "file://".len() + usize::from(needs_leading_slash) + uri_path_capacity(&path),
    );
    uri.push_str("file://");
    if needs_leading_slash {
        uri.push('/');
    }
    push_percent_encoded_uri_path(&mut uri, &path);
    uri
}

pub fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    if uri.len() > MAX_LSP_URI_BYTES {
        return None;
    }
    let path = uri.strip_prefix("file://")?;
    if path.contains('?') || path.contains('#') {
        return None;
    }
    let parsed = parse_file_uri_path(path)?;
    #[cfg(windows)]
    {
        match parsed {
            FileUriPath::Local(path) => {
                let mut path = percent_decode_windows_uri_path_bytes_to_vec(path, false)?;
                let strip_prefix_len = windows_local_uri_path_prefix_len(&path)?;
                if strip_prefix_len > 0 {
                    path.drain(..strip_prefix_len);
                }
                normalize_windows_uri_path_separators(&mut path);
                String::from_utf8(path).ok().map(PathBuf::from)
            }
            FileUriPath::Unc { authority, path } => {
                let mut decoded = Vec::with_capacity(2 + authority.len() + path.len());
                decoded.extend_from_slice(b"\\\\");
                decoded.extend_from_slice(authority.as_bytes());
                push_percent_decoded_uri_path_bytes(&mut decoded, path, true, true)?;
                String::from_utf8(decoded).ok().map(PathBuf::from)
            }
        }
    }
    #[cfg(not(windows))]
    {
        let FileUriPath::Local(path) = parsed;
        let decoded = percent_decode_uri_path(path)?;
        Some(match decoded {
            Cow::Borrowed(path) => PathBuf::from(path),
            Cow::Owned(path) => PathBuf::from(path),
        })
    }
}

#[cfg(windows)]
fn windows_local_uri_path_prefix_len(bytes: &[u8]) -> Option<usize> {
    if bytes.starts_with(b"//") {
        return None;
    }
    if is_windows_drive_uri_path_prefix(bytes) {
        return (bytes.get(3) == Some(&b'/')).then_some(1);
    }
    Some(0)
}

#[cfg(windows)]
fn is_windows_drive_uri_path_prefix(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':'
}

#[cfg(windows)]
fn is_windows_unc_authority(authority: &str) -> bool {
    !authority.is_empty()
        && authority
            .bytes()
            .all(|byte| !matches!(byte, 0..=0x20 | 0x7f | b'%' | b':' | b'/' | b'\\'))
}

#[cfg(windows)]
fn normalize_windows_uri_path_separators(path: &mut [u8]) {
    for byte in path {
        if *byte == b'/' {
            *byte = b'\\';
        }
    }
}

enum FileUriPath<'a> {
    Local(&'a str),
    #[cfg(windows)]
    Unc {
        authority: &'a str,
        path: &'a str,
    },
}

fn parse_file_uri_path(path: &str) -> Option<FileUriPath<'_>> {
    if path.starts_with("//") {
        return None;
    }
    if path.starts_with('/') {
        return Some(FileUriPath::Local(path));
    }

    let (authority, _) = path.split_once('/')?;
    let rest = &path[authority.len()..];
    if !rest.starts_with('/') || rest.starts_with("//") {
        return None;
    }
    if authority.eq_ignore_ascii_case("localhost") {
        return Some(FileUriPath::Local(rest));
    }

    #[cfg(windows)]
    {
        if is_windows_unc_authority(authority) && rest.len() > 1 {
            return Some(FileUriPath::Unc {
                authority,
                path: rest,
            });
        }
    }

    None
}

fn uri_path_capacity(path: &str) -> usize {
    path.bytes()
        .map(|byte| if is_plain_uri_path_byte(byte) { 1 } else { 3 })
        .sum()
}

#[cfg(test)]
pub(super) fn percent_encode_uri_path(path: &str) -> String {
    let mut encoded = String::with_capacity(uri_path_capacity(path));
    push_percent_encoded_uri_path(&mut encoded, path);
    encoded
}

fn push_percent_encoded_uri_path(encoded: &mut String, path: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in path.bytes() {
        if is_plain_uri_path_byte(byte) {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
}

fn is_plain_uri_path_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'/'
            | b':'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
    )
}

#[cfg(any(not(windows), test))]
pub(super) fn percent_decode_uri_path(path: &str) -> Option<Cow<'_, str>> {
    match percent_decode_uri_path_bytes(path)? {
        Cow::Borrowed(_) => Some(Cow::Borrowed(path)),
        Cow::Owned(decoded) => String::from_utf8(decoded).ok().map(Cow::Owned),
    }
}

#[cfg(any(not(windows), test))]
fn percent_decode_uri_path_bytes(path: &str) -> Option<Cow<'_, [u8]>> {
    let bytes = path.as_bytes();
    let mut first_percent = None;
    for (idx, byte) in bytes.iter().copied().enumerate() {
        match byte {
            0 => return None,
            b'%' => {
                first_percent = Some(idx);
                break;
            }
            _ => {}
        }
    }
    let Some(first_percent) = first_percent else {
        return Some(Cow::Borrowed(bytes));
    };

    let mut decoded = Vec::with_capacity(path.len());
    decoded.extend_from_slice(&bytes[..first_percent]);
    push_percent_decoded_uri_path_bytes_from(&mut decoded, bytes, first_percent, false, false)?;
    Some(Cow::Owned(decoded))
}

#[cfg(windows)]
fn percent_decode_windows_uri_path_bytes_to_vec(
    path: &str,
    windows_slashes: bool,
) -> Option<Vec<u8>> {
    let mut decoded = Vec::with_capacity(path.len());
    push_percent_decoded_uri_path_bytes(&mut decoded, path, windows_slashes, true)?;
    Some(decoded)
}

#[cfg(windows)]
fn push_percent_decoded_uri_path_bytes(
    decoded: &mut Vec<u8>,
    path: &str,
    windows_slashes: bool,
    reject_windows_separators: bool,
) -> Option<()> {
    push_percent_decoded_uri_path_bytes_from(
        decoded,
        path.as_bytes(),
        0,
        windows_slashes,
        reject_windows_separators,
    )
}

fn push_percent_decoded_uri_path_bytes_from(
    decoded: &mut Vec<u8>,
    bytes: &[u8],
    mut idx: usize,
    windows_slashes: bool,
    reject_windows_separators: bool,
) -> Option<()> {
    while idx < bytes.len() {
        let is_escaped = bytes[idx] == b'%';
        let mut byte = if is_escaped {
            let high = bytes.get(idx + 1).copied().and_then(hex_value)?;
            let low = bytes.get(idx + 2).copied().and_then(hex_value)?;
            idx += 3;
            (high << 4) | low
        } else {
            let byte = bytes[idx];
            idx += 1;
            byte
        };
        if byte == 0 {
            return None;
        }
        if reject_windows_separators {
            if is_escaped && matches!(byte, b'/' | b'\\') {
                return None;
            }
            if !is_escaped && byte == b'\\' {
                return None;
            }
        }
        if windows_slashes && byte == b'/' {
            byte = b'\\';
        }
        decoded.push(byte);
    }
    Some(())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
