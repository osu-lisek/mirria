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
use tracing::{error, warn};

use crate::{ops::DownloadIndex, osu::types::Beatmapset};

const LATEST_RANKED_LIMIT: usize = 50;
const POPULAR_DOWNLOAD_LIMIT: usize = 30;
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
    latest_ranked: Vec<i64>,
    downloads: HashMap<i64, DownloadStat>,
    popular_downloads: Vec<i64>,
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
        let size = usize::try_from(size).ok()?;
        if size == 0 || size > self.capacity {
            return None;
        }

        let mut inner = self.inner.lock();
        if !is_eligible(&inner, key.id)
            || inner.entries.contains_key(&key)
            || inner.reservations.contains(&key)
        {
            return None;
        }

        let accounted = inner
            .used_bytes
            .checked_add(inner.reserved_bytes)?
            .checked_add(size)?;
        if accounted > self.capacity {
            let incoming_priority = retention_priority(&inner, key.id);
            let mut candidates = inner.entries.iter().collect::<Vec<_>>();
            candidates.sort_by_key(|(candidate, entry)| eviction_order(&inner, candidate, entry));

            let mut reclaimed = 0_usize;
            let mut victims = Vec::new();
            for (candidate, entry) in candidates {
                if retention_priority(&inner, candidate.id) > incoming_priority {
                    return None;
                }
                reclaimed = reclaimed.checked_add(entry.value.bytes.len())?;
                victims.push(*candidate);
                if accounted - reclaimed <= self.capacity {
                    break;
                }
            }
            if accounted - reclaimed > self.capacity {
                return None;
            }
            for victim in victims {
                remove_entry(&mut inner, victim);
            }
        }
        inner.reserved_bytes += size;
        inner.reservations.insert(key);

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
            rebuild_popular(&mut inner);
            reconcile_policy(&mut inner);

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
                    rebuild_popular(&mut inner);
                    reconcile_policy(&mut inner);
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
        let ranked_query = async {
            self.meili_client
                .index("beatmapset")
                .search()
                .with_filter("status = 'ranked'")
                .with_sort(&["ranked_date:desc", "id:asc"])
                .with_limit(LATEST_RANKED_LIMIT)
                .execute::<Beatmapset>()
                .await
        };
        let downloads_query = async {
            self.meili_client
                .index("downloads")
                .search()
                .with_sort(&["count:desc", "date:desc", "id:asc"])
                .with_limit(POPULAR_DOWNLOAD_LIMIT)
                .execute::<DownloadIndex>()
                .await
        };
        let (ranked, downloads) = tokio::join!(ranked_query, downloads_query);
        let mut errors = Vec::new();
        let latest_ranked = match ranked {
            Ok(ranked) => Some(
                ranked
                    .hits
                    .into_iter()
                    .map(|hit| hit.result.mapset_id)
                    .collect::<Vec<_>>(),
            ),
            Err(error) => {
                errors.push(format!("latest ranked query: {error}"));
                None
            }
        };
        let downloads = match downloads {
            Ok(downloads) => Some(
                downloads
                    .hits
                    .into_iter()
                    .map(|hit| hit.result)
                    .collect::<Vec<_>>(),
            ),
            Err(error) => {
                errors.push(format!("popular downloads query: {error}"));
                None
            }
        };
        self.apply_policy(latest_ranked, downloads);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    pub(crate) async fn refresh_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(POLICY_REFRESH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(error) = self.refresh_once().await {
                warn!(
                    "Smart-cache policy refresh was incomplete; retaining failed portions: {error}"
                );
            }
        }
    }

    fn apply_policy(&self, latest_ranked: Option<Vec<i64>>, downloads: Option<Vec<DownloadIndex>>) {
        let mut inner = self.inner.lock();
        if let Some(latest_ranked) = latest_ranked {
            inner.latest_ranked = latest_ranked
                .into_iter()
                .take(LATEST_RANKED_LIMIT)
                .collect();
        }
        for download in downloads.into_iter().flatten() {
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
        rebuild_popular(&mut inner);
        reconcile_policy(&mut inner);
    }

    #[cfg(test)]
    fn set_policy_for_test(&self, latest_ranked: Vec<i64>, downloads: Vec<DownloadIndex>) {
        self.apply_policy(Some(latest_ranked), Some(downloads));
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

        if bytes.len() != self.size
            || !is_eligible(&inner, self.key.id)
            || inner
                .used_bytes
                .checked_add(bytes.len())
                .is_none_or(|used| used > self.cache.capacity)
        {
            return false;
        }

        inner.access_clock = inner.access_clock.wrapping_add(1);
        let last_access = inner.access_clock;
        inner.used_bytes += bytes.len();
        inner.entries.insert(
            self.key,
            RamEntry {
                value: CacheValue { bytes, cached_at },
                last_access,
            },
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

fn is_eligible(inner: &SmartCacheInner, id: i64) -> bool {
    inner.latest_ranked.contains(&id) || inner.popular_downloads.contains(&id)
}

fn rebuild_popular(inner: &mut SmartCacheInner) {
    let mut downloads = inner
        .downloads
        .iter()
        .map(|(&id, &stat)| (id, stat))
        .collect::<Vec<_>>();
    downloads.sort_by_key(|(id, stat)| (Reverse(stat.count), Reverse(stat.date), *id));
    downloads.truncate(POPULAR_DOWNLOAD_LIMIT);
    inner.popular_downloads = downloads.iter().map(|(id, _)| *id).collect();
}

fn reconcile_policy(inner: &mut SmartCacheInner) {
    let victims = inner
        .entries
        .keys()
        .copied()
        .filter(|key| !is_eligible(inner, key.id))
        .collect::<Vec<_>>();
    for victim in victims {
        remove_entry(inner, victim);
    }
}

fn remove_entry(inner: &mut SmartCacheInner, key: CacheKey) {
    if let Some(entry) = inner.entries.remove(&key) {
        inner.used_bytes = inner.used_bytes.saturating_sub(entry.value.bytes.len());
    }
}

fn retention_priority(inner: &SmartCacheInner, id: i64) -> (u8, usize) {
    let ranked = inner
        .latest_ranked
        .iter()
        .position(|candidate| *candidate == id);
    let popular = inner
        .popular_downloads
        .iter()
        .position(|candidate| *candidate == id);
    let tier = match (ranked, popular) {
        (Some(_), Some(_)) => 3_u8,
        (Some(_), None) => 2,
        (None, Some(_)) => 1,
        (None, None) => 0,
    };
    let rank_quality = ranked
        .map(|position| LATEST_RANKED_LIMIT.saturating_sub(position))
        .unwrap_or_default()
        + popular
            .map(|position| POPULAR_DOWNLOAD_LIMIT.saturating_sub(position))
            .unwrap_or_default();
    (tier, rank_quality)
}

fn eviction_order(
    inner: &SmartCacheInner,
    key: &CacheKey,
    entry: &RamEntry,
) -> ((u8, usize), u64, Reverse<i64>, bool) {
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
        cache.set_policy_for_test(vec![1], vec![]);
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
        cache.set_policy_for_test(vec![1, 2, 3], vec![]);
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
        cache.set_policy_for_test(vec![1], vec![]);
        let key = CacheKey { id: 1, video: true };

        let fill = cache.reserve_fill(key, 4).unwrap();
        assert!(cache.reserve_fill(key, 4).is_none());
        assert_eq!(cache.accounted_bytes(), 4);
        drop(fill);
        assert_eq!(cache.accounted_bytes(), 0);
        assert!(cache.reserve_fill(key, 8).is_some());
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
    fn lower_priority_admission_never_evicts_higher_priority_entries() {
        let cache = cache(2);
        cache.set_policy_for_test(vec![1, 2], vec![download(3, 100, 100)]);
        let best_ranked = CacheKey { id: 1, video: true };
        let popular = CacheKey { id: 3, video: true };
        cache
            .reserve_fill(best_ranked, 1)
            .unwrap()
            .commit(Bytes::from_static(b"r"), 0);
        cache
            .reserve_fill(popular, 1)
            .unwrap()
            .commit(Bytes::from_static(b"p"), 0);

        assert!(cache
            .reserve_fill(CacheKey { id: 2, video: true }, 2)
            .is_none());
        assert!(cache.contains(best_ranked));
        assert!(cache.contains(popular));
        assert_eq!(cache.accounted_bytes(), 2);
    }

    #[test]
    fn policy_keeps_latest_fifty_and_top_thirty_by_count_then_recency() {
        let cache = cache(1_000);
        let latest = (1..=50).collect::<Vec<_>>();
        let downloads = (100..=129)
            .map(|id| download(id, 1, id))
            .collect::<Vec<_>>();
        cache.set_policy_for_test(latest, downloads);

        for id in [1, 50, 100, 129] {
            let key = CacheKey { id, video: true };
            cache
                .reserve_fill(key, 1)
                .unwrap()
                .commit(Bytes::from_static(b"x"), 0);
        }
        assert!(cache.contains(CacheKey { id: 1, video: true }));
        assert!(cache.contains(CacheKey {
            id: 50,
            video: true
        }));
        assert!(cache.contains(CacheKey {
            id: 100,
            video: true
        }));
        assert!(cache.contains(CacheKey {
            id: 129,
            video: true
        }));

        cache.set_policy_for_test((1..=50).collect(), vec![download(130, 1, 130)]);
        assert!(!cache.contains(CacheKey {
            id: 100,
            video: true
        }));
        cache
            .reserve_fill(
                CacheKey {
                    id: 130,
                    video: true,
                },
                1,
            )
            .unwrap()
            .commit(Bytes::from_static(b"x"), 0);
        assert!(cache.contains(CacheKey {
            id: 130,
            video: true
        }));

        cache.set_policy_for_test(
            (200..=249).collect(),
            (300..=329).map(|id| download(id, 2, id)).collect(),
        );
        assert_eq!(cache.accounted_bytes(), 0);
    }

    #[test]
    fn capacity_evicts_popular_before_ranked_deterministically() {
        let cache = cache(2);
        cache.set_policy_for_test(vec![1], vec![download(2, 100, 100)]);
        cache
            .reserve_fill(CacheKey { id: 2, video: true }, 1)
            .unwrap()
            .commit(Bytes::from_static(b"p"), 0);
        cache
            .reserve_fill(CacheKey { id: 1, video: true }, 1)
            .unwrap()
            .commit(Bytes::from_static(b"r"), 0);
        cache.set_policy_for_test(vec![1, 3], vec![download(2, 100, 100)]);
        cache
            .reserve_fill(CacheKey { id: 3, video: true }, 1)
            .unwrap()
            .commit(Bytes::from_static(b"n"), 0);

        assert!(!cache.contains(CacheKey { id: 2, video: true }));
        assert!(cache.contains(CacheKey { id: 1, video: true }));
        assert!(cache.contains(CacheKey { id: 3, video: true }));
        assert_eq!(cache.accounted_bytes(), 2);
    }
}
