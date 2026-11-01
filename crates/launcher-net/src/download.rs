use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use reqwest::header::RANGE;
use reqwest::{Client, StatusCode};
use serde::Serialize;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{sleep, Instant};

use crate::{NetError, Result};

/// One file to download.
#[derive(Debug, Clone)]
pub struct DownloadSpec {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
    pub sha512: Option<String>,
    pub size: Option<u64>,
}

impl DownloadSpec {
    pub fn new(url: impl Into<String>, dest: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            dest: dest.into(),
            sha1: None,
            sha256: None,
            sha512: None,
            size: None,
        }
    }

    pub fn with_sha1(mut self, sha1: Option<String>) -> Self {
        self.sha1 = sha1;
        self
    }

    pub fn with_sha256(mut self, sha256: Option<String>) -> Self {
        self.sha256 = sha256;
        self
    }

    pub fn with_sha512(mut self, sha512: Option<String>) -> Self {
        self.sha512 = sha512;
        self
    }

    pub fn with_size(mut self, size: Option<u64>) -> Self {
        self.size = size;
        self
    }

    pub fn label(&self) -> String {
        self.dest
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&self.url)
            .to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct DownloadControl {
    paused: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

impl DownloadControl {
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.paused.store(false, Ordering::Relaxed);
    }

    /// Clear the cancel flag before starting a new batch.
    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::Relaxed);
        self.paused.store(false, Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    async fn wait_if_paused(&self) -> Result<()> {
        while self.is_paused() {
            if self.is_cancelled() {
                return Err(NetError::Cancelled);
            }
            sleep(Duration::from_millis(120)).await;
        }
        if self.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DownloadEvent {
    Progress {
        completed: usize,
        total: usize,
        bytes: u64,
        current: String,
    },
    Finished {
        files: usize,
        bytes: u64,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadSummary {
    pub files: usize,
    pub bytes: u64,
}

/// Shared progress counters. Cloneable across download workers.
#[derive(Clone)]
pub struct ProgressTracker {
    tx: Option<UnboundedSender<DownloadEvent>>,
    total: usize,
    completed: Arc<AtomicUsize>,
    bytes: Arc<AtomicU64>,
}

impl ProgressTracker {
    pub fn new(total: usize, tx: Option<UnboundedSender<DownloadEvent>>) -> Self {
        Self {
            tx,
            total,
            completed: Arc::new(AtomicUsize::new(0)),
            bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    fn emit(&self, current: &str) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(DownloadEvent::Progress {
                completed: self.completed.load(Ordering::Relaxed),
                total: self.total,
                bytes: self.bytes.load(Ordering::Relaxed),
                current: current.to_string(),
            });
        }
    }

    fn add_bytes(&self, current: &str, delta: u64) {
        self.bytes.fetch_add(delta, Ordering::Relaxed);
        self.emit(current);
    }

    fn file_finished(&self, current: &str) {
        self.completed.fetch_add(1, Ordering::Relaxed);
        self.emit(current);
    }

    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub struct DownloadManager {
    inner: Arc<Inner>,
}

struct Inner {
    client: Client,
    control: DownloadControl,
    concurrency: usize,
}

impl DownloadManager {
    pub fn new(client: Client, concurrency: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                client,
                control: DownloadControl::default(),
                concurrency: concurrency.clamp(1, 32),
            }),
        }
    }

    pub fn control(&self) -> DownloadControl {
        self.inner.control.clone()
    }

    pub fn client(&self) -> &Client {
        &self.inner.client
    }

    pub fn concurrency(&self) -> usize {
        self.inner.concurrency
    }

    /// Download a single file, resuming from a `.part` file when possible.
    pub async fn fetch(&self, spec: DownloadSpec) -> Result<PathBuf> {
        let tracker = ProgressTracker::new(1, None);
        self.fetch_one(&spec, &tracker, self.inner.control.clone())
            .await?;
        tracker.file_finished(&spec.label());
        Ok(spec.dest)
    }

    /// Download a batch with bounded concurrency. Returns `Partial` when one or
    /// more files fail; successfully downloaded files stay on disk.
    pub async fn fetch_all(
        &self,
        specs: Vec<DownloadSpec>,
        tracker: ProgressTracker,
    ) -> Result<DownloadSummary> {
        let control = self.inner.control.clone();
        control.reset();

        let total = specs.len();
        let failures = futures::stream::iter(specs.into_iter().map(|spec| {
            let manager = self.clone();
            let tracker = tracker.clone();
            let control = control.clone();
            async move {
                match manager.fetch_one(&spec, &tracker, control).await {
                    Ok(()) => {
                        tracker.file_finished(&spec.label());
                        None
                    }
                    Err(err) => Some((spec.url.clone(), err.to_string())),
                }
            }
        }))
        .buffer_unordered(self.inner.concurrency)
        .filter_map(|failure| async move { failure })
        .collect::<Vec<_>>()
        .await;

        if !failures.is_empty() {
            let detail = failures
                .iter()
                .take(3)
                .map(|(url, err)| format!("{url}: {err}"))
                .collect::<Vec<_>>()
                .join(" | ");
            return Err(NetError::Partial(failures.len(), detail));
        }

        let bytes = tracker.bytes();
        if let Some(tx) = &tracker.tx {
            let _ = tx.send(DownloadEvent::Finished {
                files: total,
                bytes,
            });
        }
        Ok(DownloadSummary {
            files: total,
            bytes,
        })
    }

    async fn fetch_one(
        &self,
        spec: &DownloadSpec,
        tracker: &ProgressTracker,
        control: DownloadControl,
    ) -> Result<()> {
        if spec.dest.exists() {
            if !has_hashes(spec) || verify_file(&spec.dest, spec).await? {
                return Ok(());
            }
            // A corrupted cache entry must never be reused.
            fs::remove_file(&spec.dest).await.at(&spec.dest)?;
        }

        if let Some(parent) = spec.dest.parent() {
            fs::create_dir_all(parent).await.at(parent)?;
        }

        let part = part_path(&spec.dest);
        let mut last_error = None;

        for attempt in 1..=3 {
            control.wait_if_paused().await?;
            match self
                .transfer(spec, &part, tracker, &control)
                .await
            {
                Ok(()) => {
                    if !has_hashes(spec) || verify_file(&part, spec).await? {
                        fs::rename(&part, &spec.dest).await.at(&spec.dest)?;
                        return Ok(());
                    }
                    // Restart from zero: resuming a bad part would fail forever.
                    fs::remove_file(&part).await.ok();
                    last_error = Some(NetError::Manifest(format!(
                        "hash verification failed for {}",
                        spec.url
                    )));
                    tracing::warn!(url = %spec.url, "hash mismatch, restarting download");
                }
                Err(NetError::Cancelled) => return Err(NetError::Cancelled),
                Err(NetError::HttpStatus { status: 416, .. }) => {
                    fs::remove_file(&part).await.ok();
                    last_error = Some(NetError::Manifest(
                        "server rejected resume range, restarting".into(),
                    ));
                }
                Err(err) => {
                    tracing::warn!(url = %spec.url, attempt, %err, "download attempt failed");
                    last_error = Some(err);
                }
            }

            if attempt < 3 {
                sleep(Duration::from_millis(300 * attempt as u64)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| NetError::Manifest(format!("download failed: {}", spec.url))))
    }

    async fn transfer(
        &self,
        spec: &DownloadSpec,
        part: &Path,
        tracker: &ProgressTracker,
        control: &DownloadControl,
    ) -> Result<()> {
        let existing = fs::metadata(part).await.map(|m| m.len()).unwrap_or(0);
        let mut request = self.inner.client.get(&spec.url);
        if existing > 0 {
            request = request.header(RANGE, format!("bytes={existing}-"));
        }
        let response = request.send().await?;
        let status = response.status();

        let resuming = existing > 0 && status == StatusCode::PARTIAL_CONTENT;
        if !status.is_success() && status != StatusCode::RANGE_NOT_SATISFIABLE {
            return Err(NetError::HttpStatus {
                status: status.as_u16(),
                url: spec.url.clone(),
            });
        }
        if status == StatusCode::RANGE_NOT_SATISFIABLE {
            return Err(NetError::HttpStatus {
                status: 416,
                url: spec.url.clone(),
            });
        }

        let mut file = if resuming {
            OpenOptions::new().append(true).open(part).await.at(part)?
        } else {
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(part)
                .await
                .at(part)?
        };

        let mut stream = response.bytes_stream();
        let mut last_emit = Instant::now();
        let mut pending: u64 = 0;

        while let Some(chunk) = stream.next().await {
            control.wait_if_paused().await?;
            let chunk = chunk?;
            file.write_all(&chunk).await.at(part)?;
            pending += chunk.len() as u64;

            if last_emit.elapsed() >= Duration::from_millis(120) {
                tracker.add_bytes(&spec.label(), pending);
                pending = 0;
                last_emit = Instant::now();
            }
        }
        file.flush().await.at(part)?;
        if pending > 0 {
            tracker.add_bytes(&spec.label(), pending);
        }
        Ok(())
    }
}

fn part_path(dest: &Path) -> PathBuf {
    let mut os = dest.as_os_str().to_os_string();
    os.push(".part");
    PathBuf::from(os)
}

fn has_hashes(spec: &DownloadSpec) -> bool {
    spec.sha1.is_some() || spec.sha256.is_some() || spec.sha512.is_some()
}

/// Verify any subset of hashes; all provided hashes must match.
pub async fn verify_file(path: &Path, spec: &DownloadSpec) -> Result<bool> {
    if let Some(expected) = &spec.sha1 {
        if !hash_matches(path, HashKind::Sha1, expected).await? {
            return Ok(false);
        }
    }
    if let Some(expected) = &spec.sha256 {
        if !hash_matches(path, HashKind::Sha256, expected).await? {
            return Ok(false);
        }
    }
    if let Some(expected) = &spec.sha512 {
        if !hash_matches(path, HashKind::Sha512, expected).await? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[derive(Clone, Copy)]
enum HashKind {
    Sha1,
    Sha256,
    Sha512,
}

async fn hash_matches(path: &Path, kind: HashKind, expected: &str) -> Result<bool> {
    let target = path.to_path_buf();
    let reported = target.clone();
    let actual = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        use std::io::Read;
        let mut file = std::fs::File::open(&target)?;
        let mut buf = [0u8; 64 * 1024];
        macro_rules! digest {
            ($hasher:ty) => {{
                let mut hasher = <$hasher>::new();
                loop {
                    let n = file.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    hasher.update(&buf[..n]);
                }
                hex::encode(hasher.finalize())
            }};
        }
        Ok(match kind {
            HashKind::Sha1 => digest!(Sha1),
            HashKind::Sha256 => digest!(Sha256),
            HashKind::Sha512 => digest!(Sha512),
        })
    })
    .await
    .map_err(|e| NetError::Join(e.to_string()))?
    .map_err(|source| NetError::Io {
        path: reported.clone(),
        source,
    })?;

    if actual.eq_ignore_ascii_case(expected) {
        Ok(true)
    } else {
        tracing::warn!(%actual, %expected, "hash mismatch");
        Ok(false)
    }
}

// Imported through the crate root to avoid repeating the blanket impl here.
use crate::IoContext as _;
