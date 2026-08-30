use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};

use axum::body::Bytes;
use chrono::Local;
use meilisearch_sdk::{
    client::Client,
    errors::{Error as MeiliError, ErrorCode},
};
use parking_lot::Mutex;
use tracing::{error, info, warn};

use crate::ops::DownloadIndex;

const DOWNLOAD_STAT_SEED_LIMIT: usize = 1_000;
const POLICY_REFRESH_INTERVAL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CacheKey {
    pub(crate) id: i64,
    pub(crate) video: bool,
}

#[derive(Clone)]
pub(crate) struct CacheValue {
    pub(crate) bytes: Bytes,
    pub(crate) cached_at: i64,
}

struct RamEntry {
    value: CacheValue,
    last_access: u64,
}

#[derive(Clone, Copy, Debug)]
struct DownloadStat {
    count: u64,
    date: i64,
    baseline_known: bool,
}

#[derive(Default)]
struct SmartCacheInner {
    entries: HashMap<CacheKey, RamEntry>,
    used_bytes: usize,
    reserved_bytes: usize,
    reservations: HashSet<CacheKey>,
    access_clock: u64,
    downloads: HashMap<i64, DownloadStat>,
    persist_queue: VecDeque<i64>,
    persist_queued: HashSet<i64>,
    persist_worker_running: bool,
}

pub(crate) struct SmartCache {
    capacity: usize,
    inner: Mutex<SmartCacheInner>,
    meili_client: Arc<Client>,
}

pub(crate) struct FillReservation {
    cache: Arc<SmartCache>,
    key: CacheKey,
    size: usize,
    active: bool,
}

impl SmartCache {
    pub(crate) fn new(capacity: usize, meili_client: Arc<Client>) -> Arc<Self> {
        Arc::new(Self {
            capacity,
            inner: Mutex::new(SmartCacheInner::default()),
            meili_client,
        })
    }

    pub(crate) fn get(&self, key: CacheKey) -> Option<CacheValue> {
        let mut inner = self.inner.lock();
        inner.access_clock = inner.access_clock.wrapping_add(1);
        let access = inner.access_clock;
        let entry = inner.entries.get_mut(&key)?;
        entry.last_access = access;
        Some(entry.value.clone())
    }

    pub(crate) fn invalidate(&self, key: CacheKey) {
        let mut inner = self.inner.lock();
        remove_entry(&mut inner, key);
    }

    pub(crate) fn reserve_fill(
        self: &Arc<Self>,
        key: CacheKey,
        size: u64,
    ) -> Option<FillReservation> {
        let Ok(size) = usize::try_from(size) else {
            warn!(
                "Skipping RAM cache fill for map {} (video={}): {size} bytes exceeds platform limits",
                key.id, key.video
            );
            return None;
        };
        if size == 0 {
            warn!(
                "Skipping RAM cache fill for map {} (video={}): archive is empty",
                key.id, key.video
            );
            return None;
        }
        if size > self.capacity {
            info!(
                "Skipping RAM cache fill for map {} (video={}): {size} bytes exceeds {}-byte capacity",
                key.id, key.video, self.capacity
            );
            return None;
        }

        let mut inner = self.inner.lock();
        if inner.entries.contains_key(&key) {
            info!(
                "Skipping RAM cache fill for map {} (video={}): already cached",
                key.id, key.video
            );
            return None;
        }
        if inner.reservations.contains(&key) {
            info!(
                "Skipping RAM cache fill for map {} (video={}): fill already in progress",
                key.id, key.video
            );
            return None;
        }

        let Some(accounted) = inner
            .used_bytes
            .checked_add(inner.reserved_bytes)
            .and_then(|accounted| accounted.checked_add(size))
        else {
            warn!(
                "Skipping RAM cache fill for map {} (video={}): byte accounting overflow",
                key.id, key.video
            );
            return None;
        };
        if accounted > self.capacity {
            let incoming_priority = projected_retention_priority(&inner, key.id);
            let mut candidates = inner.entries.iter().collect::<Vec<_>>();
            candidates.sort_by_key(|(candidate, entry)| eviction_order(&inner, candidate, entry));

            let mut reclaimed = 0_usize;
            let mut victims = Vec::new();
            for (candidate, entry) in candidates {
                let candidate_priority = retention_priority(&inner, candidate.id);
                if candidate_priority > incoming_priority {
                    info!(
                        "Skipping RAM cache fill for map {} (video={}): download priority {:?} cannot displace map {} priority {:?}",
                        key.id,
                        key.video,
                        incoming_priority,
                        candidate.id,
                        candidate_priority
                    );
                    return None;
                }
                reclaimed = reclaimed.checked_add(entry.value.bytes.len())?;
                victims.push(*candidate);
                if accounted - reclaimed <= self.capacity {
                    break;
                }
            }
            if accounted - reclaimed > self.capacity {
                info!(
                    "Skipping RAM cache fill for map {} (video={}): insufficient reclaimable capacity",
                    key.id, key.video
                );
                return None;
            }
            for victim in victims {
                if let Some(entry) = inner.entries.get(&victim) {
                    info!(
                        "Evicting map {} (video={}, {} bytes) from RAM cache for map {}",
                        victim.id,
                        victim.video,
                        entry.value.bytes.len(),
                        key.id
                    );
                }
                remove_entry(&mut inner, victim);
            }
        }
        inner.reserved_bytes += size;
        inner.reservations.insert(key);
        info!(
            "Reserved {size} bytes in RAM cache for map {} (video={}); {} / {} bytes accounted",
            key.id,
            key.video,
            inner.used_bytes + inner.reserved_bytes,
            self.capacity
        );

        Some(FillReservation {
            cache: Arc::clone(self),
            key,
            size,
            active: true,
        })
    }

    pub(crate) fn record_success(self: Arc<Self>, id: i64) {
        let now = Local::now().timestamp();
        let start_worker = {
            let mut inner = self.inner.lock();
            let stat = inner.downloads.entry(id).or_insert(DownloadStat {
                count: 0,
                date: now,
                baseline_known: false,
            });
            stat.count = stat.count.saturating_add(1);
            stat.date = now;

            if inner.persist_queued.insert(id) {
                inner.persist_queue.push_back(id);
            }
            if inner.persist_worker_running {
                false
            } else {
                inner.persist_worker_running = true;
                true
            }
        };

        if start_worker {
            tokio::spawn(self.persist_loop());
        }
    }

    async fn persist_loop(self: Arc<Self>) {
        loop {
            let id = {
                let mut inner = self.inner.lock();
                match inner.persist_queue.pop_front() {
                    Some(id) => {
                        inner.persist_queued.remove(&id);
                        id
                    }
                    None => {
                        inner.persist_worker_running = false;
                        return;
                    }
                }
            };
            self.persist_one(id).await;
        }
    }

    async fn persist_one(&self, id: i64) {
        let baseline_known = self
            .inner
            .lock()
            .downloads
            .get(&id)
            .is_some_and(|stat| stat.baseline_known);
        if !baseline_known {
            let index = self.meili_client.index("downloads");
            let baseline = match index.get_document::<DownloadIndex>(&id.to_string()).await {
                Ok(document) => document,
                Err(MeiliError::Meilisearch(error))
                    if error.error_code == ErrorCode::DocumentNotFound =>
                {
                    DownloadIndex {
                        id,
                        date: i64::MIN,
                        count: 0,
                    }
                }
                Err(error) => {
                    error!(
                        "Failed to load the persisted download count for {id}; \
                         retaining the increment in memory: {error}"
                    );
                    return;
                }
            };

            let mut inner = self.inner.lock();
            if let Some(stat) = inner.downloads.get_mut(&id) {
                if !stat.baseline_known {
                    stat.count = stat.count.saturating_add(baseline.count);
                    stat.date = stat.date.max(baseline.date);
                    stat.baseline_known = true;
                }
            }
        }

        let document = {
            let inner = self.inner.lock();
            let stat = inner
                .downloads
                .get(&id)
                .expect("a completed download must have an in-memory statistic");
            DownloadIndex {
                id,
                date: stat.date,
                count: stat.count,
            }
        };
        let index = self.meili_client.index("downloads");
        match index.add_documents(&[document], Some("id")).await {
            Ok(task) => {
                if let Err(error) = task
                    .wait_for_completion(&self.meili_client, None, None)
                    .await
                {
                    error!("Failed to persist completed download for {id}: {error}");
                }
            }
            Err(error) => error!("Failed to enqueue completed download for {id}: {error}"),
        }
    }

    pub(crate) async fn refresh_once(&self) -> Result<(), String> {
        let downloads = self
            .meili_client
            .index("downloads")
            .search()
            .with_sort(&["count:desc", "date:desc", "id:asc"])
            .with_limit(DOWNLOAD_STAT_SEED_LIMIT)
            .execute::<DownloadIndex>()
            .await
            .map_err(|error| format!("download statistics query: {error}"))?;
        let refreshed = downloads.hits.len();
        self.apply_downloads(downloads.hits.into_iter().map(|hit| hit.result).collect());
        info!(
            "Smart-cache background refresh loaded {refreshed} download statistics; tracking {} maps",
            self.inner.lock().downloads.len()
        );
        Ok(())
    }

    pub(crate) async fn refresh_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(POLICY_REFRESH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = self.refresh_once().await {
                warn!("Smart-cache download statistics refresh failed: {error}");
            }
        }
    }

    fn apply_downloads(&self, downloads: Vec<DownloadIndex>) {
        let mut inner = self.inner.lock();
        for download in downloads {
            let current = inner.downloads.get(&download.id).copied();
            let (count, date) = match current {
                Some(current) if current.baseline_known => (
                    current.count.max(download.count),
                    current.date.max(download.date),
                ),
                Some(pending) => (
                    download.count.saturating_add(pending.count),
                    download.date.max(pending.date),
                ),
                None => (download.count, download.date),
            };
            inner.downloads.insert(
                download.id,
                DownloadStat {
                    count,
                    date,
                    baseline_known: true,
                },
            );
        }
    }

    #[cfg(test)]
    fn set_downloads_for_test(&self, downloads: Vec<DownloadIndex>) {
        self.apply_downloads(downloads);
    }

    #[cfg(test)]
    fn accounted_bytes(&self) -> usize {
        let inner = self.inner.lock();
        inner.used_bytes + inner.reserved_bytes
    }

    #[cfg(test)]
    fn contains(&self, key: CacheKey) -> bool {
        self.inner.lock().entries.contains_key(&key)
    }

    #[cfg(test)]
    fn persistence_state(&self, id: i64) -> (u64, usize, bool) {
        let inner = self.inner.lock();
        (
            inner
                .downloads
                .get(&id)
                .map(|stat| stat.count)
                .unwrap_or_default(),
            inner.persist_queue.len(),
            inner.persist_worker_running,
        )
    }
}

impl FillReservation {
    pub(crate) fn commit(mut self, bytes: Bytes, cached_at: i64) -> bool {
        let mut inner = self.cache.inner.lock();
        inner.reserved_bytes = inner.reserved_bytes.saturating_sub(self.size);
        inner.reservations.remove(&self.key);
        self.active = false;

        if bytes.len() != self.size {
            warn!(
                "Discarding RAM cache fill for map {} (video={}): expected {} bytes, received {}",
                self.key.id,
                self.key.video,
                self.size,
                bytes.len()
            );
            return false;
        }
        if !inner.downloads.contains_key(&self.key.id) {
            warn!(
                "Discarding RAM cache fill for map {} (video={}): completed download was not recorded",
                self.key.id, self.key.video
            );
            return false;
        }
        let Some(used_bytes) = inner.used_bytes.checked_add(bytes.len()) else {
            warn!(
                "Discarding RAM cache fill for map {} (video={}): byte accounting overflow",
                self.key.id, self.key.video
            );
            return false;
        };
        if used_bytes > self.cache.capacity {
            info!(
                "Discarding RAM cache fill for map {} (video={}): {used_bytes} bytes would exceed {}-byte capacity",
                self.key.id, self.key.video, self.cache.capacity
            );
            return false;
        }

        inner.access_clock = inner.access_clock.wrapping_add(1);
        let last_access = inner.access_clock;
        inner.used_bytes = used_bytes;
        inner.entries.insert(
            self.key,
            RamEntry {
                value: CacheValue { bytes, cached_at },
                last_access,
            },
        );
        info!(
            "Cached map {} (video={}) in RAM; {} / {} bytes used across {} entries",
            self.key.id,
            self.key.video,
            inner.used_bytes,
            self.cache.capacity,
            inner.entries.len()
        );
        true
    }
}

impl Drop for FillReservation {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut inner = self.cache.inner.lock();
        inner.reserved_bytes = inner.reserved_bytes.saturating_sub(self.size);
        inner.reservations.remove(&self.key);
    }
}

fn remove_entry(inner: &mut SmartCacheInner, key: CacheKey) {
    if let Some(entry) = inner.entries.remove(&key) {
        inner.used_bytes = inner.used_bytes.saturating_sub(entry.value.bytes.len());
    }
}

fn retention_priority(inner: &SmartCacheInner, id: i64) -> (u64, i64) {
    inner
        .downloads
        .get(&id)
        .map(|stat| (stat.count, stat.date))
        .unwrap_or((0, i64::MIN))
}

fn projected_retention_priority(inner: &SmartCacheInner, id: i64) -> (u64, i64) {
    inner
        .downloads
        .get(&id)
        .map(|stat| (stat.count.saturating_add(1), i64::MAX))
        .unwrap_or((1, i64::MAX))
}

fn eviction_order(
    inner: &SmartCacheInner,
    key: &CacheKey,
    entry: &RamEntry,
) -> ((u64, i64), u64, Reverse<i64>, bool) {
    (
        retention_priority(inner, key.id),
        entry.last_access,
        Reverse(key.id),
        key.video,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(capacity: usize) -> Arc<SmartCache> {
        SmartCache::new(
            capacity,
            Arc::new(Client::new("http://127.0.0.1:9", None::<String>).unwrap()),
        )
    }

    fn download(id: i64, count: u64, date: i64) -> DownloadIndex {
        DownloadIndex { id, count, date }
    }

    #[test]
    fn variants_are_isolated_and_capacity_counts_reservations() {
        let cache = cache(8);
        cache.set_downloads_for_test(vec![download(1, 1, 1)]);
        let video = CacheKey { id: 1, video: true };
        let no_video = CacheKey {
            id: 1,
            video: false,
        };

        let video_fill = cache.reserve_fill(video, 4).unwrap();
        let no_video_fill = cache.reserve_fill(no_video, 4).unwrap();
        assert_eq!(cache.accounted_bytes(), 8);
        assert!(cache
            .reserve_fill(CacheKey { id: 2, video: true }, 1)
            .is_none());

        assert!(video_fill.commit(Bytes::from_static(b"full"), 10));
        assert!(no_video_fill.commit(Bytes::from_static(b"lite"), 10));
        assert_eq!(cache.accounted_bytes(), 8);
        assert_eq!(cache.get(video).unwrap().bytes, "full");
        assert_eq!(cache.get(no_video).unwrap().bytes, "lite");
        cache.invalidate(no_video);
        assert!(cache.get(no_video).is_none());
        assert_eq!(cache.get(video).unwrap().bytes, "full");
    }

    #[test]
    fn dropped_and_concurrent_reservations_never_exceed_capacity() {
        let cache = cache(10);
        let first = cache
            .reserve_fill(CacheKey { id: 1, video: true }, 6)
            .unwrap();
        assert!(cache
            .reserve_fill(CacheKey { id: 2, video: true }, 5)
            .is_none());
        assert_eq!(cache.accounted_bytes(), 6);
        drop(first);
        assert_eq!(cache.accounted_bytes(), 0);
        assert!(cache
            .reserve_fill(CacheKey { id: 2, video: true }, 11)
            .is_none());
    }

    #[test]
    fn duplicate_reservations_are_rejected_without_consuming_capacity() {
        let cache = cache(8);
        let key = CacheKey { id: 1, video: true };

        let fill = cache.reserve_fill(key, 4).unwrap();
        assert!(cache.reserve_fill(key, 4).is_none());
        assert_eq!(cache.accounted_bytes(), 4);
        drop(fill);
        assert_eq!(cache.accounted_bytes(), 0);
        assert!(cache.reserve_fill(key, 8).is_some());
    }

    #[tokio::test]
    async fn completed_unseen_download_is_cached_for_the_next_request() {
        let cache = cache(4);
        let key = CacheKey {
            id: 42,
            video: true,
        };
        let fill = cache.reserve_fill(key, 4).unwrap();

        Arc::clone(&cache).record_success(key.id);
        assert!(fill.commit(Bytes::from_static(b"data"), 10));

        assert_eq!(cache.get(key).unwrap().bytes, "data");
        assert_eq!(cache.accounted_bytes(), 4);
    }

    #[tokio::test]
    async fn repeated_successes_coalesce_into_one_bounded_persistence_worker() {
        let cache = cache(1);
        for _ in 0..100 {
            Arc::clone(&cache).record_success(42);
        }

        assert_eq!(cache.persistence_state(42), (100, 1, true));
    }

    #[test]
    fn lower_download_count_never_evicts_higher_download_counts() {
        let cache = cache(2);
        cache.set_downloads_for_test(vec![
            download(1, 100, 100),
            download(2, 1, 100),
            download(3, 50, 100),
        ]);
        let most_downloaded = CacheKey { id: 1, video: true };
        let second_most_downloaded = CacheKey { id: 3, video: true };
        cache
            .reserve_fill(most_downloaded, 1)
            .unwrap()
            .commit(Bytes::from_static(b"a"), 0);
        cache
            .reserve_fill(second_most_downloaded, 1)
            .unwrap()
            .commit(Bytes::from_static(b"b"), 0);

        assert!(cache
            .reserve_fill(CacheKey { id: 2, video: true }, 2)
            .is_none());
        assert!(cache.contains(most_downloaded));
        assert!(cache.contains(second_most_downloaded));
        assert_eq!(cache.accounted_bytes(), 2);
    }

    #[test]
    fn ram_capacity_evicts_the_least_downloaded_maps_by_byte_size() {
        let cache = cache(4);
        cache.set_downloads_for_test(vec![
            download(1, 100, 100),
            download(2, 1, 100),
            download(3, 10, 100),
        ]);
        let most_downloaded = CacheKey { id: 1, video: true };
        let least_downloaded = CacheKey { id: 2, video: true };
        let incoming = CacheKey { id: 3, video: true };
        cache
            .reserve_fill(most_downloaded, 2)
            .unwrap()
            .commit(Bytes::from_static(b"aa"), 0);
        cache
            .reserve_fill(least_downloaded, 2)
            .unwrap()
            .commit(Bytes::from_static(b"bb"), 0);

        cache
            .reserve_fill(incoming, 2)
            .unwrap()
            .commit(Bytes::from_static(b"cc"), 0);

        assert!(cache.contains(most_downloaded));
        assert!(!cache.contains(least_downloaded));
        assert!(cache.contains(incoming));
        assert_eq!(cache.accounted_bytes(), 4);
    }
}
