//! Pure XDND URI parser, shared with PaintAtlas regression tests.
use percent_encoding::percent_decode;
use std::{
    io,
    path::{Path, PathBuf},
    str::Utf8Error,
};

#[derive(Debug)]
pub enum DndDataParseError {
    EmptyData,
    InvalidUtf8(#[allow(dead_code)] Utf8Error),
    HostnameSpecified(#[allow(dead_code)] String),
    UnexpectedProtocol(#[allow(dead_code)] String),
    UnresolvablePath(#[allow(dead_code)] io::Error),
}

impl From<Utf8Error> for DndDataParseError {
    fn from(e: Utf8Error) -> Self {
        DndDataParseError::InvalidUtf8(e)
    }
}

impl From<io::Error> for DndDataParseError {
    fn from(e: io::Error) -> Self {
        DndDataParseError::UnresolvablePath(e)
    }
}

// PaintAtlas: browsers send image URLs in text/uri-list. Preserve remote/data
// URIs verbatim (including percent escapes); the application loads them in a
// worker. PathBuf is only the transport through winit's DroppedFile API here.
pub fn parse_uri_data(data: &[u8]) -> Result<Vec<PathBuf>, DndDataParseError> {
    let text = std::str::from_utf8(data)?.trim_end_matches('\0');
    let mut paths = Vec::new();
    for uri in text
        .lines()
        .map(str::trim)
        .filter(|u| !u.is_empty() && !u.starts_with('#'))
    {
        if uri.starts_with("https://")
            || uri.starts_with("http://")
            || uri.starts_with("data:image/")
            || uri.starts_with("blob:")
        {
            paths.push(PathBuf::from(uri));
        } else if let Some(path) = uri.strip_prefix("file://") {
            let path = path
                .strip_prefix("localhost")
                .filter(|p| p.starts_with('/'))
                .unwrap_or(path);
            let decoded = percent_decode(path.as_bytes()).decode_utf8()?;
            if !decoded.starts_with('/') {
                return Err(DndDataParseError::HostnameSpecified(decoded.into_owned()));
            }
            paths.push(Path::new(decoded.as_ref()).canonicalize()?);
        } else {
            return Err(DndDataParseError::UnexpectedProtocol(uri.to_owned()));
        }
    }
    if paths.is_empty() {
        Err(DndDataParseError::EmptyData)
    } else {
        Ok(paths)
    }
}

#[cfg(test)]
mod paintatlas_tests {
    use super::*;
    #[test]
    fn browser_image_uris_preserve_escaping_and_accept_uri_list_comments() {
        let data = b"# image drag\r\nhttps://example.com/a%20b.png?q=x%2Fy\r\ndata:image/png;base64,AAAA\r\n";
        let paths = parse_uri_data(data).unwrap();
        assert_eq!(
            paths[0].to_str().unwrap(),
            "https://example.com/a%20b.png?q=x%2Fy"
        );
        assert_eq!(paths[1].to_str().unwrap(), "data:image/png;base64,AAAA");
        assert!(parse_uri_data(b"javascript:alert(1)").is_err());
        assert!(parse_uri_data(b"# empty\n").is_err());
    }
    #[test]
    fn local_file_drag_still_decodes_percent_escapes() {
        let path = std::env::temp_dir().join(format!("paintatlas drag {}.png", std::process::id()));
        std::fs::write(&path, b"test").unwrap();
        let uri = format!("file://{}\r\n", path.to_string_lossy().replace(' ', "%20"));
        assert_eq!(
            parse_uri_data(uri.as_bytes()).unwrap(),
            vec![path.canonicalize().unwrap()]
        );
        std::fs::remove_file(path).unwrap();
    }
}
