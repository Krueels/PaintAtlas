//! External image drops. Network and image decoding never run on the UI thread.
use crate::clipboard::Image;
use base64::Engine;
use eframe::egui;
use std::{
    io::{Cursor, Read},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
};

const MAX_ENCODED: usize = 32 * 1024 * 1024;
const MAX_BATCH: usize = 8;
#[derive(Debug)]
pub enum Source {
    File(PathBuf),
    Remote(String),
    Data(String),
    Bytes(Arc<[u8]>),
}
impl Source {
    fn uri(text: &str) -> Result<Self, String> {
        if text.starts_with("data:image/") {
            return Ok(Self::Data(text.into()));
        }
        let url = url::Url::parse(text).map_err(|_| "Endereço de imagem inválido.".to_owned())?;
        match url.scheme() {
            "http" | "https" if url.host_str().is_some() => Ok(Self::Remote(url.into())),
            "file" => url.to_file_path().map(Self::File).map_err(|_| "Arquivo local inválido.".into()),
            "blob" => Err("O navegador não disponibilizou os pixels desta imagem temporária. Use Copiar imagem e cole no canvas.".into()),
            _ => Err("Arraste uma imagem ou seu endereço HTTP/HTTPS, não um link de página.".into()),
        }
    }
    fn label(&self) -> String {
        match self {
            Self::File(path) => path
                .file_stem()
                .map(|s| s.to_string_lossy().chars().take(90).collect())
                .unwrap_or_else(|| "Imagem".into()),
            _ => "Imagem do navegador".into(),
        }
    }
    #[cfg(test)]
    pub fn load(self) -> Result<Loaded, String> {
        self.load_with_cancel(&Cancellation::default())
    }
    fn load_with_cancel(self, cancel: &Cancellation) -> Result<Loaded, String> {
        let name = self.label();
        let image = match self {
            Self::Bytes(bytes) => decode(&bytes),
            Self::File(path) => {
                let file = std::fs::File::open(path)
                    .map_err(|e| format!("Não foi possível abrir a imagem: {e}"))?;
                decode(&read_limited(file)?)
            }
            Self::Data(uri) => decode(&data_uri(&uri)?),
            Self::Remote(url) => decode(&download(&url, cancel)?),
        }?;
        Ok(Loaded { image, name })
    }
}
pub fn sources(files: Vec<egui::DroppedFile>) -> Result<Vec<Result<Source, String>>, String> {
    let mut sources = Vec::new();
    for file in files {
        if let Some(bytes) = file.bytes {
            if bytes.len() > MAX_ENCODED {
                return Err("O arquivo da imagem excede 32 MB.".into());
            }
            if matches!(file.mime.as_str(), "text/uri-list" | "text/plain") {
                let text = std::str::from_utf8(&bytes)
                    .map_err(|_| "Endereço arrastado inválido.".to_owned())?;
                for uri in text
                    .lines()
                    .map(str::trim)
                    .filter(|u| !u.is_empty() && !u.starts_with('#'))
                {
                    if sources.len() >= MAX_BATCH {
                        return Err(format!("Arraste no máximo {MAX_BATCH} imagens por vez."));
                    }
                    sources.push(Source::uri(uri));
                }
            } else {
                sources.push(Ok(Source::Bytes(bytes)));
            }
        } else if let Some(path) = file.path {
            let text = path.to_str().unwrap_or("");
            if text.starts_with("data:") || text.contains("://") {
                sources.push(Source::uri(text));
            } else {
                sources.push(Ok(Source::File(path)));
            }
        } else if !file.name.is_empty() {
            sources.push(Source::uri(&file.name));
        } else {
            sources.push(Err(
                "O programa de origem não forneceu os dados da imagem.".into()
            ));
        }
        if sources.len() > MAX_BATCH {
            return Err(format!("Arraste no máximo {MAX_BATCH} imagens por vez."));
        }
    }
    if sources.is_empty() {
        return Err("Nenhuma imagem foi recebida.".into());
    }
    Ok(sources)
}
fn read_limited(reader: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_ENCODED + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Falha ao ler imagem: {e}"))?;
    if bytes.len() > MAX_ENCODED {
        return Err("O arquivo da imagem excede 32 MB.".into());
    }
    Ok(bytes)
}
fn data_uri(uri: &str) -> Result<Vec<u8>, String> {
    let (header, data) = uri.split_once(',').ok_or("Imagem embutida inválida.")?;
    if !header.starts_with("data:image/") || data.len() > MAX_ENCODED * 4 / 3 + 1024 {
        return Err("Imagem embutida inválida ou muito grande.".into());
    }
    let bytes = if header.split(';').any(|p| p == "base64") {
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| "Imagem base64 inválida.".to_owned())?
    } else {
        percent_encoding::percent_decode_str(data).collect()
    };
    if bytes.len() > MAX_ENCODED {
        return Err("O arquivo da imagem excede 32 MB.".into());
    }
    Ok(bytes)
}
fn decode(bytes: &[u8]) -> Result<Image, String> {
    if bytes.len() > MAX_ENCODED {
        return Err("O arquivo da imagem excede 32 MB.".into());
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(crate::document::MAX_SIDE);
    limits.max_image_height = Some(crate::document::MAX_SIDE);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let rgba=reader.decode().map_err(|_|"Não foi possível decodificar a imagem. Use PNG, JPG, WebP, GIF ou BMP de até 4096 × 4096 px.".to_owned())?.into_rgba8();
    let image = Image {
        size: [rgba.width() as usize, rgba.height() as usize],
        pixels: rgba.into_raw(),
    };
    image.validate()?;
    Ok(image)
}
#[derive(Default)]
struct Cancellation {
    stopped: AtomicBool,
    child: Mutex<Option<Child>>,
}
impl Cancellation {
    fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(child) = self.child.lock().unwrap().as_mut() {
            let _ = child.kill();
        }
    }
}
fn download(url: &str, cancel: &Cancellation) -> Result<Vec<u8>, String> {
    // curl uses the system's TLS/certificate support and avoids adding a second
    // HTTP/TLS runtime to this desktop app. Arguments never go through a shell.
    let mut child = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-redirs",
            "5",
            "--connect-timeout",
            "8",
            "--max-time",
            "25",
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "--max-filesize",
            "33554432",
            "--user-agent",
            "PaintAtlas/0.1",
            "--url",
            url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Não foi possível baixar a imagem com curl: {e}"))?;
    let stdout = child.stdout.take().unwrap();
    {
        let mut slot = cancel.child.lock().unwrap();
        if cancel.stopped.load(Ordering::Relaxed) {
            let _ = child.kill();
        }
        *slot = Some(child);
    }
    let bytes = read_limited(stdout);
    let mut child = cancel.child.lock().unwrap().take().unwrap();
    if bytes.is_err() || cancel.stopped.load(Ordering::Relaxed) {
        let _ = child.kill();
        let _ = child.wait();
        return if bytes.is_err() {
            bytes
        } else {
            Err("Importação cancelada.".into())
        };
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Falha ao baixar imagem: {e}"))?;
    if !output.status.success() {
        return Err("Não foi possível baixar a imagem. O site pode exigir login ou bloquear o download; tente Copiar imagem e colar.".into());
    }
    bytes
}
pub struct Loaded {
    pub image: Image,
    pub name: String,
}
pub struct Job {
    pub target: Option<u64>,
    pub total: usize,
    pub received: usize,
    receiver: Receiver<Result<Loaded, String>>,
    cancelled: Arc<Cancellation>,
}
impl Job {
    pub fn start(
        files: Vec<egui::DroppedFile>,
        target: Option<u64>,
        ctx: egui::Context,
    ) -> Result<Self, String> {
        let sources = sources(files)?;
        let total = sources.len();
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(Cancellation::default());
        let cancel = cancelled.clone();
        std::thread::Builder::new()
            .name("image-drop".into())
            .spawn(move || {
                for source in sources {
                    if cancel.stopped.load(Ordering::Relaxed) {
                        break;
                    }
                    let result = source.and_then(|source| source.load_with_cancel(&cancel));
                    if cancel.stopped.load(Ordering::Relaxed) || sender.send(result).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .map_err(|e| format!("Não foi possível iniciar a importação: {e}"))?;
        Ok(Self {
            target,
            total,
            received: 0,
            receiver,
            cancelled,
        })
    }
    pub fn poll(&mut self) -> Option<Result<Loaded, String>> {
        match self.receiver.try_recv() {
            Ok(result) => {
                self.received += 1;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.received = self.total;
                Some(Err(
                    "A importação terminou antes de carregar todas as imagens.".into(),
                ))
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancelled.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn png() -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(3, 2, image::Rgba([180, 50, 20, 128]));
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }
    #[test]
    fn bytes_data_urls_and_local_file_urls_preserve_rgba_without_clipboard() {
        let bytes = png();
        let expected = decode(&bytes).unwrap();
        let uri = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        assert_eq!(Source::uri(&uri).unwrap().load().unwrap().image, expected);
        let uri = format!(
            "data:image/png,{}",
            percent_encoding::percent_encode(&bytes, percent_encoding::NON_ALPHANUMERIC)
        );
        assert_eq!(Source::uri(&uri).unwrap().load().unwrap().image, expected);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("brush reference.png");
        std::fs::write(&path, &bytes).unwrap();
        let uri = url::Url::from_file_path(path).unwrap();
        assert_eq!(
            Source::uri(uri.as_str()).unwrap().load().unwrap().image,
            expected
        );
        assert_eq!(Source::Bytes(bytes.into()).load().unwrap().image, expected);
    }
    #[test]
    fn uri_lists_keep_encoded_urls_and_unsupported_payloads_fail_cleanly() {
        let files = vec![egui::DroppedFile {
            mime: "text/uri-list".into(),
            bytes: Some(Arc::from(
                b"# browser\r\nhttps://example.com/a%20b.png?q=x%2Fy\r\n".as_slice(),
            )),
            ..Default::default()
        }];
        let source = sources(files).unwrap().pop().unwrap().unwrap();
        assert!(
            matches!(source,Source::Remote(ref s) if s=="https://example.com/a%20b.png?q=x%2Fy")
        );
        assert!(Source::uri("javascript:alert(1)").is_err());
        assert!(Source::uri("blob:https://example.com/temporary").is_err());
        assert!(decode(b"<html>not an image</html>").is_err());
        assert!(data_uri("data:image/png;base64,not base64!").is_err());
        assert!(sources(vec![egui::DroppedFile::default(); 9]).is_err());
        let text = "https://example.com/a.png\n".repeat(9);
        assert!(
            sources(vec![egui::DroppedFile {
                mime: "text/uri-list".into(),
                bytes: Some(Arc::from(text.into_bytes())),
                ..Default::default()
            }])
            .is_err()
        );
    }
    #[test]
    fn gif_uses_the_first_frame_and_bmp_is_supported() {
        let gif = base64::engine::general_purpose::STANDARD
            .decode("R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7")
            .unwrap();
        assert_eq!(decode(&gif).unwrap().size, [1, 1]);
        let mut bmp = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(3, 2)
            .write_to(&mut bmp, image::ImageFormat::Bmp)
            .unwrap();
        assert_eq!(decode(bmp.get_ref()).unwrap().size, [3, 2]);
    }
    #[test]
    fn browser_image_url_downloads_from_http_without_file_extension() {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let bytes = png();
        let expected = decode(&bytes).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let _ = stream.read(&mut request).unwrap();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();
            stream.write_all(&bytes).unwrap();
        });
        let image = Source::uri(&format!("http://{address}/image?name=a%20b"))
            .unwrap()
            .load()
            .unwrap()
            .image;
        assert_eq!(image, expected);
        server.join().unwrap();
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    #[test]
    fn cancelling_an_import_terminates_the_pending_http_connection() {
        use std::net::TcpListener;
        let server = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = server.local_addr().unwrap();
        let (connected_tx, connected_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = server.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            connected_tx.send(()).unwrap();
            let mut byte = [0];
            stream.read(&mut byte).unwrap() == 0
        });
        let file = egui::DroppedFile {
            path: Some(format!("http://{address}/pending.png").into()),
            ..Default::default()
        };
        let job = Job::start(vec![file], None, egui::Context::default()).unwrap();
        connected_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        drop(job);
        assert!(thread.join().unwrap());
    }
}
