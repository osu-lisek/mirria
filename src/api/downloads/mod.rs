use std::{
    collections::HashMap,
    io,
    net::{IpAddr, SocketAddr},
    path::{Path as FilePath, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Weak,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::{Body, Bytes},
    extract::{ConnectInfo, Path},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::get,
    Extension, Router,
};
use chrono::{DateTime, Local};
use futures_util::{Stream, StreamExt};
use parking_lot::Mutex as StdMutex;
use serde_json::json;
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex, OwnedMutexGuard},
};
use tracing::{error, info};

use crate::{
    crawler::Context,
    ops::{beatmapset::get_beatmapset_by_id, DownloadIndex},
    osu::client::OsuApi,
};

const RATE_WINDOW_SECONDS: u64 = 5;
const MISS_LIMIT: u32 = 10;
const HIT_LIMIT: u32 = 50;
const COPY_BUFFER_SIZE: usize = 64 * 1024;
const MAP_LOCK_PRUNE_INTERVAL: u64 = 256;
const RATE_LIMIT_CLIENT_PRUNE_SECONDS: u64 = RATE_WINDOW_SECONDS;

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

type DownloadByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, io::Error>> + Send + 'static>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CacheKind {
    Hit,
    Miss,
}

impl CacheKind {
    fn limit(self) -> u32 {
        match self {
            Self::Hit => HIT_LIMIT,
            Self::Miss => MISS_LIMIT,
        }
    }

    fn header_value(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Miss => "miss",
        }
    }
}

#[derive(Default)]
struct RateWindow {
    started_at: u64,
    reserved_or_successful: u32,
}

impl RateWindow {
    fn refresh(&mut self, now: u64) {
        let current_window = now - (now % RATE_WINDOW_SECONDS);
        if self.started_at != current_window {
            self.started_at = current_window;
            self.reserved_or_successful = 0;
        }
    }
}

#[derive(Default)]
struct ClientRateWindows {
    hit: RateWindow,
    miss: RateWindow,
    active_reservations: u64,
}

#[derive(Default)]
struct RateLimiter {
    clients: StdMutex<HashMap<IpAddr, ClientRateWindows>>,
    next_client_prune_at: AtomicU64,
}

#[derive(Debug)]
struct RateLimitExceeded {
    reset_at: u64,
}

struct RateReservation {
    limiter: Arc<RateLimiter>,
    client: IpAddr,
    kind: CacheKind,
    window_started_at: u64,
    remaining: u32,
    reset_at: u64,
    committed: bool,
}

impl RateLimiter {
    fn prune_clients_if_due(&self, clients: &mut HashMap<IpAddr, ClientRateWindows>, now: u64) {
        if now < self.next_client_prune_at.load(Ordering::Relaxed) {
            return;
        }

        self.next_client_prune_at.store(
            now.saturating_add(RATE_LIMIT_CLIENT_PRUNE_SECONDS),
            Ordering::Relaxed,
        );
        clients.retain(|_, windows| {
            windows.active_reservations > 0
                || windows.hit.started_at.saturating_add(RATE_WINDOW_SECONDS) > now
                || windows.miss.started_at.saturating_add(RATE_WINDOW_SECONDS) > now
        });
        clients.shrink_to_fit();
    }

    fn reserve(
        self: &Arc<Self>,
        client: IpAddr,
        kind: CacheKind,
    ) -> Result<RateReservation, RateLimitExceeded> {
        self.reserve_at(client, kind, unix_seconds())
    }

    fn reserve_at(
        self: &Arc<Self>,
        client: IpAddr,
        kind: CacheKind,
        now: u64,
    ) -> Result<RateReservation, RateLimitExceeded> {
        let mut clients = self.clients.lock();
        self.prune_clients_if_due(&mut clients, now);
        let client_windows = clients.entry(client).or_default();
        let window = match kind {
            CacheKind::Hit => &mut client_windows.hit,
            CacheKind::Miss => &mut client_windows.miss,
        };
        window.refresh(now);

        let reset_at = window.started_at + RATE_WINDOW_SECONDS;
        if window.reserved_or_successful >= kind.limit() {
            return Err(RateLimitExceeded { reset_at });
        }

        window.reserved_or_successful += 1;
        let window_started_at = window.started_at;
        let remaining = kind.limit() - window.reserved_or_successful;
        client_windows.active_reservations += 1;
        Ok(RateReservation {
            limiter: Arc::clone(self),
            client,
            kind,
            window_started_at,
            remaining,
            reset_at,
            committed: false,
        })
    }
}

impl RateReservation {
    fn commit(&mut self) {
        if self.committed {
            return;
        }

        let mut clients = self.limiter.clients.lock();
        if let Some(client_windows) = clients.get_mut(&self.client) {
            client_windows.active_reservations =
                client_windows.active_reservations.saturating_sub(1);
        }
        self.committed = true;
    }
}

impl Drop for RateReservation {
    fn drop(&mut self) {
        if self.committed {
            return;
        }

        let mut clients = self.limiter.clients.lock();
        let Some(client_windows) = clients.get_mut(&self.client) else {
            return;
        };
        client_windows.active_reservations = client_windows.active_reservations.saturating_sub(1);
        let window = match self.kind {
            CacheKind::Hit => &mut client_windows.hit,
            CacheKind::Miss => &mut client_windows.miss,
        };

        if window.started_at == self.window_started_at {
            window.reserved_or_successful = window.reserved_or_successful.saturating_sub(1);
        }
    }
}

#[derive(Default)]
pub(crate) struct DownloadState {
    rate_limiter: Arc<RateLimiter>,
    map_locks: StdMutex<HashMap<i64, Weak<Mutex<()>>>>,
    map_lock_accesses: AtomicU64,
}

impl DownloadState {
    fn map_lock(&self, id: i64) -> Arc<Mutex<()>> {
        let mut locks = self.map_locks.lock();
        let accesses = self
            .map_lock_accesses
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let should_prune = accesses.is_multiple_of(MAP_LOCK_PRUNE_INTERVAL);
        if should_prune {
            locks.retain(|_, lock| lock.strong_count() > 0);
            locks.shrink_to_fit();
        }
        if let Some(lock) = locks.get(&id).and_then(Weak::upgrade) {
            return lock;
        }

        let lock = Arc::new(Mutex::new(()));
        locks.insert(id, Arc::downgrade(&lock));
        lock
    }
}

struct PartialCacheFile {
    path: Option<PathBuf>,
}

impl PartialCacheFile {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for PartialCacheFile {
    fn drop(&mut self) {
        let Some(path) = self.path.take() else {
            return;
        };

        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Err(error) = fs::remove_file(&path).await {
                    if error.kind() != io::ErrorKind::NotFound {
                        error!(
                            "Failed to remove partial map cache {}: {error}",
                            path.display()
                        );
                    }
                }
            });
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn seconds_until_reset(reset_at: u64, now: u64) -> u64 {
    reset_at.saturating_sub(now).min(RATE_WINDOW_SECONDS)
}

fn temporary_path(cache_path: &FilePath) -> PathBuf {
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = cache_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("map.osz");
    cache_path.with_file_name(format!(".{name}.{}.{}.part", std::process::id(), sequence))
}

fn cache_file_has_content(metadata: Option<&std::fs::Metadata>) -> bool {
    metadata.is_some_and(|metadata| metadata.len() > 0)
}

fn upstream_length_is_acceptable(expected_length: Option<u64>) -> bool {
    expected_length != Some(0)
}

fn error_response(status: StatusCode, message: &str) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"ok": false, "message": message}).to_string(),
        ))
        .unwrap_or_else(|error| {
            error!("Failed to build JSON error response: {error}");
            let mut response = Response::new(Body::empty());
            *response.status_mut() = status;
            response
        })
}

fn rate_limited_response(limit: RateLimitExceeded) -> Response {
    rate_limited_response_at(limit, unix_seconds())
}

fn rate_limited_response_at(limit: RateLimitExceeded, now: u64) -> Response {
    let reset_after = seconds_until_reset(limit.reset_at, now);
    Response::builder()
        .status(StatusCode::TOO_MANY_REQUESTS)
        .header("X-RateLimit-Remaining", "0")
        .header("X-RateLimit-Reset", reset_after.to_string())
        .header(header::RETRY_AFTER, reset_after.to_string())
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"ok": false, "message": "Download rate limit exceeded"}).to_string(),
        ))
        .unwrap_or_else(|error| {
            error!("Failed to build rate-limit response: {error}");
            error_response(
                StatusCode::TOO_MANY_REQUESTS,
                "Download rate limit exceeded",
            )
        })
}

fn content_disposition(id: i64, display_name: Option<&str>) -> HeaderValue {
    let fallback = format!("{id}.osz");
    let fallback_header = || {
        HeaderValue::from_str(&format!("attachment; filename=\"{fallback}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment"))
    };
    let Some(display_name) = display_name else {
        return fallback_header();
    };

    let encoded = urlencoding::encode(display_name);
    HeaderValue::from_str(&format!(
        "attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}"
    ))
    .unwrap_or_else(|_| fallback_header())
}

fn successful_response(
    kind: CacheKind,
    remaining: u32,
    reset_after: u64,
    id: i64,
    display_name: Option<&str>,
    content_length: Option<u64>,
    body: Body,
) -> Response {
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-osu-beatmap-archive")
        .header(
            header::CONTENT_DISPOSITION,
            content_disposition(id, display_name),
        )
        .header("X-Cache-Hit", kind.header_value())
        .header("X-RateLimit-Remaining", remaining.to_string())
        .header("X-RateLimit-Reset", reset_after.to_string());

    if let Some(content_length) = content_length {
        builder = builder.header(header::CONTENT_LENGTH, content_length);
    }

    builder.body(body).unwrap_or_else(|error| {
        error!("Failed to build map download response: {error}");
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to build download response",
        )
    })
}

fn cached_body(mut file: File, mut reservation: RateReservation) -> Body {
    let stream: DownloadByteStream = Box::pin(async_stream::try_stream! {
        let mut buffer = vec![0; COPY_BUFFER_SIZE];
        loop {
            let read = file.read(&mut buffer).await?;
            if read == 0 {
                reservation.commit();
                break;
            }
            yield Bytes::copy_from_slice(&buffer[..read]);
        }
    });
    Body::from_stream(stream)
}

struct DownloadCompletion {
    meili_client: Arc<meilisearch_sdk::client::Client>,
    id: i64,
}
struct CachePaths {
    temporary: PathBuf,
    final_path: PathBuf,
}

fn streaming_cache_miss_body(
    mut upstream: DownloadByteStream,
    expected_length: Option<u64>,
    mut temporary_file: File,
    cache_paths: CachePaths,
    mut reservation: RateReservation,
    _map_guard: OwnedMutexGuard<()>,
    completion: Option<DownloadCompletion>,
) -> Body {
    let mut partial = PartialCacheFile::new(cache_paths.temporary.clone());
    let stream: DownloadByteStream = Box::pin(async_stream::try_stream! {
        let mut received = 0_u64;

        while let Some(chunk) = upstream.next().await {
            let chunk = chunk?;
            temporary_file.write_all(&chunk).await?;
            received = received
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "upstream map is too large"))?;
            yield chunk;
        }

        if received == 0 {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "upstream map archive is empty",
            ))?;
        }
        if expected_length.is_some_and(|expected| expected != received) {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "upstream map length mismatch: expected {}, received {received}",
                    expected_length.unwrap_or_default()
                ),
            ))?;
        }

        temporary_file.flush().await?;
        temporary_file.sync_all().await?;
        drop(temporary_file);
        fs::rename(&cache_paths.temporary, &cache_paths.final_path).await?;
        partial.disarm();
        reservation.commit();

        if let Some(completion) = completion {
            tokio::spawn(async move {
                let index = DownloadIndex {
                    id: completion.id,
                    date: Local::now().timestamp(),
                };
                if let Err(error) = completion
                    .meili_client
                    .index("downloads")
                    .add_documents(&[index], Some("id"))
                    .await
                {
                    error!("Failed to update download index for {}: {error}", completion.id);
                }
            });
        }
    });
    Body::from_stream(stream)
}

async fn download(
    Extension(ctx): Extension<Arc<Mutex<Context>>>,
    Extension(state): Extension<Arc<DownloadState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<i64>,
) -> Response {
    let map_guard = state.map_lock(id).lock_owned().await;

    let ctx = { ctx.lock().await.clone() };
    let cache_path = FilePath::new(&ctx.config.beatmaps_folder).join(format!("{id}.osz"));
    let cache_metadata = match fs::metadata(&cache_path).await {
        Ok(metadata) if metadata.is_file() => Some(metadata),
        Ok(_) => {
            error!("Map cache path is not a file: {}", cache_path.display());
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "Invalid map cache path");
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            error!(
                "Failed to inspect map cache {}: {error}",
                cache_path.display()
            );
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to inspect map cache",
            );
        }
    };
    let cache_exists = cache_file_has_content(cache_metadata.as_ref());
    if cache_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.len() == 0)
    {
        info!("Ignoring empty cached map {}", cache_path.display());
    }

    let beatmapset = get_beatmapset_by_id(ctx.to_owned(), id).await.ok();
    let cached_at = cache_metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64);
    let stale = match (&beatmapset, cached_at) {
        (Some(set), Some(cached_at)) => DateTime::parse_from_rfc3339(&set.last_updated)
            .map(|last_updated| last_updated.timestamp() > cached_at)
            .unwrap_or_else(|error| {
                error!("Invalid last_updated for map {id}: {error}");
                false
            }),
        _ => false,
    };
    let display_name = beatmapset
        .as_ref()
        .map(|set| format!("{} {} - {}.osz", set.mapset_id, set.artist, set.title));

    if cache_exists && !stale {
        drop(ctx);
        drop(map_guard);

        let reservation = match state.rate_limiter.reserve(peer.ip(), CacheKind::Hit) {
            Ok(reservation) => reservation,
            Err(limit) => return rate_limited_response(limit),
        };
        let file = match File::open(&cache_path).await {
            Ok(file) => file,
            Err(error) => {
                error!(
                    "Failed to open cached map {}: {error}",
                    cache_path.display()
                );
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to open cached map",
                );
            }
        };
        let content_length = match file.metadata().await {
            Ok(metadata) => Some(metadata.len()),
            Err(error) => {
                error!(
                    "Failed to inspect cached map {}: {error}",
                    cache_path.display()
                );
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to inspect cached map",
                );
            }
        };
        let remaining = reservation.remaining;
        let reset_after = seconds_until_reset(reservation.reset_at, unix_seconds());
        return successful_response(
            CacheKind::Hit,
            remaining,
            reset_after,
            id,
            display_name.as_deref(),
            content_length,
            cached_body(file, reservation),
        );
    }

    let reservation = match state.rate_limiter.reserve(peer.ip(), CacheKind::Miss) {
        Ok(reservation) => reservation,
        Err(limit) => return rate_limited_response(limit),
    };
    let mut osu = ctx.osu.clone();
    let meili_client = Arc::clone(&ctx.meili_client);
    drop(ctx);

    if let Some(parent) = cache_path.parent() {
        if let Err(error) = fs::create_dir_all(parent).await {
            error!(
                "Failed to create map cache directory {}: {error}",
                parent.display()
            );
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to prepare map cache",
            );
        }
    }

    let temporary_path = temporary_path(&cache_path);
    let temporary_file = match OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary_path)
        .await
    {
        Ok(file) => file,
        Err(error) => {
            error!(
                "Failed to create partial map cache {}: {error}",
                temporary_path.display()
            );
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to prepare map cache",
            );
        }
    };
    let mut partial = PartialCacheFile::new(temporary_path.clone());

    let upstream_response = match osu.begin_download(id).await {
        Ok(response) => response,
        Err(error) => {
            error!("Failed to start upstream map download {id}: {error}");
            return error_response(StatusCode::BAD_GATEWAY, "Upstream download failed");
        }
    };

    if !upstream_response.status().is_success() {
        error!(
            "Upstream map download {id} returned {}",
            upstream_response.status()
        );
        return error_response(StatusCode::BAD_GATEWAY, "Upstream download failed");
    }

    let expected_length = upstream_response.content_length();
    if !upstream_length_is_acceptable(expected_length) {
        error!("Upstream map download {id} declared an empty body");
        return error_response(StatusCode::BAD_GATEWAY, "Upstream download was empty");
    }
    let upstream = upstream_response
        .bytes_stream()
        .map(|chunk| chunk.map_err(io::Error::other));
    let completion = DownloadCompletion { meili_client, id };
    partial.disarm();
    let remaining = reservation.remaining;
    let reset_after = seconds_until_reset(reservation.reset_at, unix_seconds());

    info!("Streaming map {id} from upstream into cache");
    let body = streaming_cache_miss_body(
        Box::pin(upstream),
        expected_length,
        temporary_file,
        CachePaths {
            temporary: temporary_path,
            final_path: cache_path,
        },
        reservation,
        map_guard,
        Some(completion),
    );
    successful_response(
        CacheKind::Miss,
        remaining,
        reset_after,
        id,
        display_name.as_deref(),
        expected_length,
        body,
    )
}

pub fn state() -> Arc<DownloadState> {
    Arc::new(DownloadState::default())
}

pub fn serve() -> Router {
    Router::new()
        .route("/api/v1/download/:id", get(download))
        .route("/d/:id", get(download))
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, time::Duration};

    use futures_util::stream;
    use tempfile::tempdir;

    use super::*;

    fn test_reservation(kind: CacheKind) -> (Arc<RateLimiter>, RateReservation) {
        let limiter = Arc::new(RateLimiter::default());
        let reservation = limiter
            .reserve_at(Ipv4Addr::LOCALHOST.into(), kind, 100)
            .unwrap();
        (limiter, reservation)
    }

    #[test]
    fn explicit_zero_upstream_length_is_rejected_before_streaming() {
        assert!(!upstream_length_is_acceptable(Some(0)));
        assert!(upstream_length_is_acceptable(Some(1)));
        assert!(upstream_length_is_acceptable(None));
    }

    #[tokio::test]
    async fn zero_length_final_cache_is_not_a_hit() {
        let directory = tempdir().unwrap();
        let cache_path = directory.path().join("empty.osz");
        File::create(&cache_path).await.unwrap();

        let empty = fs::metadata(&cache_path).await.unwrap();
        assert!(!cache_file_has_content(Some(&empty)));

        fs::write(&cache_path, b"valid").await.unwrap();
        let populated = fs::metadata(&cache_path).await.unwrap();
        assert!(cache_file_has_content(Some(&populated)));
        assert!(!cache_file_has_content(None));
    }

    #[test]
    fn map_lock_registry_periodically_prunes_dead_entries_and_preserves_active_locks() {
        let state = DownloadState::default();
        for id in 0..MAP_LOCK_PRUNE_INTERVAL {
            drop(state.map_lock(id as i64));
        }
        assert_eq!(state.map_locks.lock().len(), 1);

        let active = state.map_lock(-1);
        for id in 0..(MAP_LOCK_PRUNE_INTERVAL - 1) {
            drop(state.map_lock(1_000 + id as i64));
        }
        let locks = state.map_locks.lock();
        assert!(locks.get(&-1).and_then(Weak::upgrade).is_some());
        assert_eq!(locks.len(), 2);
        drop(locks);
        drop(active);
    }

    #[test]
    fn rate_limiter_periodically_prunes_expired_clients_but_keeps_active_reservations() {
        let limiter = Arc::new(RateLimiter::default());
        let expired_client = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let active_client = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2));
        let current_client = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 3));

        let mut expired = limiter
            .reserve_at(expired_client, CacheKind::Hit, 100)
            .unwrap();
        expired.commit();
        let active = limiter
            .reserve_at(active_client, CacheKind::Miss, 100)
            .unwrap();
        let current = limiter
            .reserve_at(current_client, CacheKind::Hit, 105)
            .unwrap();

        let clients = limiter.clients.lock();
        assert!(!clients.contains_key(&expired_client));
        assert!(clients.contains_key(&active_client));
        assert!(clients.contains_key(&current_client));
        drop(clients);
        drop(active);
        drop(current);
    }

    #[test]
    fn reservations_prevent_concurrent_rate_limit_bypass_and_refund_failures() {
        let client = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let limiter = Arc::new(RateLimiter::default());
        let mut reservations = (0..MISS_LIMIT)
            .map(|_| limiter.reserve_at(client, CacheKind::Miss, 100).unwrap())
            .collect::<Vec<_>>();

        assert!(limiter.reserve_at(client, CacheKind::Miss, 100).is_err());
        drop(reservations.pop());
        assert!(limiter.reserve_at(client, CacheKind::Miss, 100).is_ok());
        let hit_reservations = (0..HIT_LIMIT)
            .map(|_| limiter.reserve_at(client, CacheKind::Hit, 100).unwrap())
            .collect::<Vec<_>>();
        let hit_limit = match limiter.reserve_at(client, CacheKind::Hit, 100) {
            Err(limit) => limit,
            Ok(_) => panic!("the 51st cached download reservation must be rejected"),
        };
        let response = rate_limited_response_at(hit_limit, 100);
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()["X-RateLimit-Remaining"], "0");
        assert_eq!(response.headers()["X-RateLimit-Reset"], "5");
        assert_eq!(response.headers()[header::RETRY_AFTER], "5");
        drop(hit_reservations);

        for reservation in &mut reservations {
            reservation.commit();
        }
        assert!(limiter.reserve_at(client, CacheKind::Miss, 105).is_ok());
    }

    #[test]
    fn successful_download_headers_report_cache_and_reserved_capacity() {
        let response = successful_response(
            CacheKind::Miss,
            9,
            5,
            42,
            Some("42 artist - title.osz"),
            Some(12),
            Body::empty(),
        );

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["X-Cache-Hit"], "miss");
        assert_eq!(response.headers()["X-RateLimit-Remaining"], "9");
        assert_eq!(response.headers()["X-RateLimit-Reset"], "5");
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "12");
        assert!(response.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .contains("filename*=UTF-8''42%20artist%20-%20title.osz"));
    }
    #[test]
    fn reset_header_is_a_bounded_seconds_delta() {
        assert_eq!(seconds_until_reset(105, 100), 5);
        assert_eq!(seconds_until_reset(105, 104), 1);
        assert_eq!(seconds_until_reset(105, 105), 0);
        assert_eq!(seconds_until_reset(105, 106), 0);
        assert_eq!(seconds_until_reset(200, 100), RATE_WINDOW_SECONDS);
    }

    #[tokio::test]
    async fn cached_body_streams_and_refunds_an_abandoned_client() {
        let directory = tempdir().unwrap();
        let cache_path = directory.path().join("41.osz");
        fs::write(&cache_path, vec![7_u8; COPY_BUFFER_SIZE * 2])
            .await
            .unwrap();
        let file = File::open(&cache_path).await.unwrap();
        let client = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let (limiter, reservation) = test_reservation(CacheKind::Hit);
        let other_reservations = (1..HIT_LIMIT)
            .map(|_| limiter.reserve_at(client, CacheKind::Hit, 100).unwrap())
            .collect::<Vec<_>>();
        let mut streamed = cached_body(file, reservation).into_data_stream();

        assert_eq!(
            streamed.next().await.unwrap().unwrap().len(),
            COPY_BUFFER_SIZE
        );
        assert!(limiter.reserve_at(client, CacheKind::Hit, 100).is_err());
        drop(streamed);
        let completion_reservation = limiter
            .reserve_at(client, CacheKind::Hit, 100)
            .expect("abandoned stream must refund its reservation");
        let completion_file = File::open(&cache_path).await.unwrap();
        let mut completed = cached_body(completion_file, completion_reservation).into_data_stream();
        while let Some(chunk) = completed.next().await {
            chunk.unwrap();
        }
        assert!(limiter.reserve_at(client, CacheKind::Hit, 100).is_err());
        drop(other_reservations);
    }

    #[tokio::test]
    async fn miss_streams_before_atomic_promotion_and_caches_complete_body() {
        let directory = tempdir().unwrap();
        let cache_path = directory.path().join("42.osz");
        let temporary_path = temporary_path(&cache_path);
        let temporary_file = File::create(&temporary_path).await.unwrap();
        let (_limiter, reservation) = test_reservation(CacheKind::Miss);
        let map_guard = Arc::new(Mutex::new(())).lock_owned().await;
        let chunks = stream::iter([
            Ok(Bytes::from_static(b"first")),
            Ok(Bytes::from_static(b"-second")),
        ]);
        let body = streaming_cache_miss_body(
            Box::pin(chunks),
            Some(12),
            temporary_file,
            CachePaths {
                temporary: temporary_path.clone(),
                final_path: cache_path.clone(),
            },
            reservation,
            map_guard,
            None,
        );
        let mut streamed = body.into_data_stream();

        assert_eq!(streamed.next().await.unwrap().unwrap(), "first");
        assert!(!cache_path.exists());
        assert!(temporary_path.exists());
        assert_eq!(streamed.next().await.unwrap().unwrap(), "-second");
        assert!(streamed.next().await.is_none());

        assert_eq!(fs::read(&cache_path).await.unwrap(), b"first-second");
        assert!(!temporary_path.exists());
    }

    #[tokio::test]
    async fn failed_or_abandoned_miss_removes_partial_and_refunds_quota() {
        let directory = tempdir().unwrap();
        let cache_path = directory.path().join("43.osz");
        let temporary_path = temporary_path(&cache_path);
        let temporary_file = File::create(&temporary_path).await.unwrap();
        let (limiter, reservation) = test_reservation(CacheKind::Miss);
        let map_guard = Arc::new(Mutex::new(())).lock_owned().await;
        let chunks = stream::iter([
            Ok(Bytes::from_static(b"partial")),
            Ok(Bytes::from_static(b"-unread")),
        ]);
        let body = streaming_cache_miss_body(
            Box::pin(chunks),
            None,
            temporary_file,
            CachePaths {
                temporary: temporary_path.clone(),
                final_path: cache_path.clone(),
            },
            reservation,
            map_guard,
            None,
        );
        let mut streamed = body.into_data_stream();

        assert_eq!(streamed.next().await.unwrap().unwrap(), "partial");
        drop(streamed);
        tokio::time::sleep(Duration::from_millis(25)).await;

        assert!(!cache_path.exists());
        assert!(!temporary_path.exists());
        assert!(limiter
            .reserve_at(Ipv4Addr::LOCALHOST.into(), CacheKind::Miss, 100)
            .is_ok());
    }
    #[tokio::test]
    async fn unpolled_miss_body_removes_partial_and_refunds_quota() {
        let directory = tempdir().unwrap();
        let cache_path = directory.path().join("45.osz");
        let temporary_path = temporary_path(&cache_path);
        let temporary_file = File::create(&temporary_path).await.unwrap();
        let (limiter, reservation) = test_reservation(CacheKind::Miss);
        let map_guard = Arc::new(Mutex::new(())).lock_owned().await;
        let body = streaming_cache_miss_body(
            Box::pin(stream::pending()),
            None,
            temporary_file,
            CachePaths {
                temporary: temporary_path.clone(),
                final_path: cache_path.clone(),
            },
            reservation,
            map_guard,
            None,
        );

        drop(body);
        tokio::time::timeout(Duration::from_secs(1), async {
            while temporary_path.exists() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("unpolled response body must remove its partial cache");

        assert!(!cache_path.exists());
        assert!(limiter
            .reserve_at(Ipv4Addr::LOCALHOST.into(), CacheKind::Miss, 100)
            .is_ok());
    }

    #[tokio::test]
    async fn empty_upstream_body_is_not_cached_or_charged() {
        let directory = tempdir().unwrap();
        let cache_path = directory.path().join("46.osz");
        let temporary_path = temporary_path(&cache_path);
        let temporary_file = File::create(&temporary_path).await.unwrap();
        let (limiter, reservation) = test_reservation(CacheKind::Miss);
        let map_guard = Arc::new(Mutex::new(())).lock_owned().await;
        let body = streaming_cache_miss_body(
            Box::pin(stream::empty()),
            Some(0),
            temporary_file,
            CachePaths {
                temporary: temporary_path.clone(),
                final_path: cache_path.clone(),
            },
            reservation,
            map_guard,
            None,
        );
        let mut streamed = body.into_data_stream();

        assert!(streamed.next().await.unwrap().is_err());
        drop(streamed);
        tokio::time::timeout(Duration::from_secs(1), async {
            while temporary_path.exists() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("empty upstream response must remove its partial cache");

        assert!(!cache_path.exists());
        assert!(limiter
            .reserve_at(Ipv4Addr::LOCALHOST.into(), CacheKind::Miss, 100)
            .is_ok());
    }

    #[tokio::test]
    async fn length_mismatch_does_not_promote_partial_cache() {
        let directory = tempdir().unwrap();
        let cache_path = directory.path().join("44.osz");
        let temporary_path = temporary_path(&cache_path);
        let temporary_file = File::create(&temporary_path).await.unwrap();
        let (_limiter, reservation) = test_reservation(CacheKind::Miss);
        let map_guard = Arc::new(Mutex::new(())).lock_owned().await;
        let chunks = stream::iter([Ok(Bytes::from_static(b"short"))]);
        let body = streaming_cache_miss_body(
            Box::pin(chunks),
            Some(10),
            temporary_file,
            CachePaths {
                temporary: temporary_path.clone(),
                final_path: cache_path.clone(),
            },
            reservation,
            map_guard,
            None,
        );
        let mut streamed = body.into_data_stream();

        assert_eq!(streamed.next().await.unwrap().unwrap(), "short");
        assert!(streamed.next().await.unwrap().is_err());
        drop(streamed);
        tokio::time::sleep(Duration::from_millis(25)).await;

        assert!(!cache_path.exists());
        assert!(!temporary_path.exists());
    }
}
