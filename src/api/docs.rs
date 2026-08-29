use axum::{response::Html, routing::get, Router};
use maud::{html, DOCTYPE};

async fn documentation() -> Html<String> {
    Html(
        html! {
            (DOCTYPE)
            html lang="en" {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    title { "Mirria HTTP API" }
                    style { r#"
                        :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
                        body { max-width: 76rem; margin: 2rem auto; padding: 0 1rem; line-height: 1.5; }
                        code { background: color-mix(in srgb, CanvasText 10%, Canvas); padding: .1rem .3rem; }
                        pre { overflow-x: auto; background: color-mix(in srgb, CanvasText 8%, Canvas); padding: .75rem; }
                        pre code { background: transparent; padding: 0; }
                        table { border-collapse: collapse; width: 100%; margin: .75rem 0 1.5rem; }
                        th, td { border: 1px solid color-mix(in srgb, CanvasText 25%, Canvas); padding: .5rem; text-align: left; vertical-align: top; }
                        nav ul { columns: 2; }
                        section { border-top: 1px solid color-mix(in srgb, CanvasText 25%, Canvas); margin-top: 2rem; padding-top: 1rem; }
                        .method { font-weight: 700; }
                        .note { border-left: .3rem solid color-mix(in srgb, CanvasText 40%, Canvas); padding-left: .75rem; }
                    "# }
                }
                body {
                    h1 { "Mirria HTTP API" }
                    p { "This human-readable reference is generated with Maud and is not an OpenAPI document." }
                    p { "All routes are read-only HTTP GET endpoints. JSON endpoints return UTF-8 JSON; downloads return a binary osu! beatmap archive." }

                    nav aria-label="API route navigation" {
                        h2 { "Route summary" }
                        table {
                            thead { tr { th { "Method" } th { "Path" } th { "Purpose" } } }
                            tbody {
                                tr { td class="method" { "GET" } td { a href="#beatmap-by-md5" { code { "/api/v1/beatmaps/md5/:checksum" } } } td { "Find the beatmapset containing a checksum." } }
                                tr { td class="method" { "GET" } td { a href="#beatmap-by-id" { code { "/api/v1/beatmaps/:id" } } } td { "Fetch one beatmap difficulty." } }
                                tr { td class="method" { "GET" } td { a href="#beatmapset-by-id" { code { "/api/v1/beatmapsets/:id" } } } td { "Fetch one beatmapset." } }
                                tr { td class="method" { "GET" } td { a href="#beatmapset-by-beatmap" { code { "/api/v1/beatmapsets/beatmap/:id" } } } td { "Find a beatmapset by a contained beatmap ID." } }
                                tr { td class="method" { "GET" } td { a href="#download" { code { "/api/v1/download/:id" } } } td { "Download a beatmapset archive." } }
                                tr { td class="method" { "GET" } td { a href="#download" { code { "/d/:id" } } } td { "Short alias for the download route." } }
                                tr { td class="method" { "GET" } td { a href="#search" { code { "/api/v1/search" } } } td { "Search indexed beatmapsets." } }
                                tr { td class="method" { "GET" } td { a href="#metrics" { code { "/metrics" } } } td { "Prometheus metrics exposition." } }
                                tr { td class="method" { "GET" } td { a href="#docs" { code { "/docs" } } } td { "This HTML reference." } }
                            }
                        }
                    }

                    section id="schemas" {
                        h2 { "JSON response schemas" }
                        p { "The four lookup routes and search reuse the following serialized Rust types. Fields marked optional may be JSON " code { "null" } "; fields marked omitted are absent when no value exists." }

                        h3 id="beatmap-schema" { "Beatmap" }
                        table {
                            thead { tr { th { "Fields" } th { "Type and meaning" } } }
                            tbody {
                                tr { td { code { "beatmapset_id, id, mode_int, total_length, hit_length, user_id, ranked, passcount, playcount" } } td { "Integers: parent set, beatmap and creator IDs; ruleset number; durations in seconds; rank state and play statistics." } }
                                tr { td { code { "difficulty_rating, accuracy, ar, bpm, cs, drain" } } td { "Numbers: star rating, OD, approach rate, tempo, circle size and HP drain." } }
                                tr { td { code { "mode, status, version, url" } } td { "Strings describing the ruleset, rank status, difficulty name and beatmap URL." } }
                                tr { td { code { "convert" } } td { "Boolean indicating a converted difficulty." } }
                                tr { td { code { "countCircles, countSliders, countSpinners, isScoreable" } } td { "Nullable object counts and scoreable flag." } }
                                tr { td { code { "lastUpdated" } } td { "Timestamp string as stored by the index." } }
                                tr { td { code { "deletedAt, checksum, max_combo" } } td { "Optional deletion timestamp, MD5 checksum and maximum combo; omitted when unavailable." } }
                            }
                        }

                        h3 id="beatmapset-schema" { "Beatmapset" }
                        table {
                            thead { tr { th { "Fields" } th { "Type and meaning" } } }
                            tbody {
                                tr { td { code { "id, user_id, play_count, favourite_count, offset, ranked, track_id" } } td { "Integer identifiers, counts, offset and rank state; " code { "track_id" } " is nullable." } }
                                tr { td { code { "artist, artist_unicode, title, title_unicode, creator, status, source, tags, preview_url" } } td { "Metadata strings; Unicode artist/title variants are nullable." } }
                                tr { td { code { "bpm" } } td { "Numeric tempo." } }
                                tr { td { code { "nsfw, video, storyboard, spotlight, can_be_hyped, discussion_enabled, discussion_locked, has_favourited" } } td { "Boolean feature and state flags." } }
                                tr { td { code { "is_scoreable" } } td { "Nullable scoreable flag." } }
                                tr { td { code { "last_updated, submitted_date, ranked_date, deleted_at, legacy_thread_url" } } td { "Timestamp or URL strings; all except the first two are nullable." } }
                                tr { td { code { "hype, nominations_summary, availability, covers" } } td { "JSON values retained from the osu! API." } }
                                tr { td { code { "beatmaps" } } td { "Array of Beatmap objects." } }
                                tr { td { code { "pack_tags" } } td { "Array of strings." } }
                                tr { td { code { "description, ratings" } } td { "Optional description string and integer array; omitted when unavailable." } }
                            }
                        }
                    }

                    section id="beatmap-by-md5" {
                        h2 { code { "GET /api/v1/beatmaps/md5/:checksum" } }
                        p { "Returns the first indexed beatmapset containing a beatmap with the supplied checksum." }
                        h3 { "Parameters" }
                        table {
                            thead { tr { th { "Location" } th { "Name" } th { "Required" } th { "Description" } } }
                            tbody {
                                tr { td { "Path" } td { code { "checksum" } } td { "Yes" } td { "Beatmap checksum string, normally an MD5 digest." } }
                            }
                        }
                        h3 { "Response and statuses" }
                        p { code { "200 OK" } " — " code { "application/json" } "; one " a href="#beatmapset-schema" { "Beatmapset" } " object." }
                        ul {
                            li { code { "404 Not Found" } " — no indexed beatmapset contains the checksum." }
                            li { code { "500 Internal Server Error" } " — the search index lookup failed." }
                        }
                        p { "Lookup errors do not have a documented JSON body." }
                        h3 { "Example" }
                        pre { code { "curl 'https://mirror.example/api/v1/beatmaps/md5/0123456789abcdef0123456789abcdef'" } }
                    }

                    section id="beatmap-by-id" {
                        h2 { code { "GET /api/v1/beatmaps/:id" } }
                        p { "Returns one indexed beatmap difficulty by beatmap ID." }
                        h3 { "Parameters" }
                        table {
                            thead { tr { th { "Location" } th { "Name" } th { "Required" } th { "Description" } } }
                            tbody {
                                tr { td { "Path" } td { code { "id" } } td { "Yes" } td { "Signed 64-bit beatmap ID. A value that cannot be parsed is looked up as ID 0 and normally returns 404." } }
                            }
                        }
                        h3 { "Response and statuses" }
                        p { code { "200 OK" } " — " code { "application/json" } "; one " a href="#beatmap-schema" { "Beatmap" } " object." }
                        ul {
                            li { code { "404 Not Found" } " — the indexed beatmap was not found." }
                            li { code { "500 Internal Server Error" } " — the search index lookup failed." }
                        }
                        p { "Lookup errors do not have a documented JSON body." }
                        h3 { "Example" }
                        pre { code { "curl 'https://mirror.example/api/v1/beatmaps/4662168'" } }
                    }

                    section id="beatmapset-by-id" {
                        h2 { code { "GET /api/v1/beatmapsets/:id" } }
                        p { "Returns one indexed beatmapset by beatmapset ID." }
                        h3 { "Parameters" }
                        table {
                            thead { tr { th { "Location" } th { "Name" } th { "Required" } th { "Description" } } }
                            tbody {
                                tr { td { "Path" } td { code { "id" } } td { "Yes" } td { "Signed 64-bit beatmapset ID. A value that cannot be parsed is looked up as ID 0 and normally returns 404." } }
                            }
                        }
                        h3 { "Response and statuses" }
                        p { code { "200 OK" } " — " code { "application/json" } "; one " a href="#beatmapset-schema" { "Beatmapset" } " object." }
                        ul {
                            li { code { "404 Not Found" } " — the indexed beatmapset was not found." }
                            li { code { "500 Internal Server Error" } " — the search index lookup failed." }
                        }
                        p { "Lookup errors do not have a documented JSON body." }
                        h3 { "Example" }
                        pre { code { "curl 'https://mirror.example/api/v1/beatmapsets/2556827'" } }
                    }

                    section id="beatmapset-by-beatmap" {
                        h2 { code { "GET /api/v1/beatmapsets/beatmap/:id" } }
                        p { "Returns the first indexed beatmapset containing the supplied beatmap ID." }
                        h3 { "Parameters" }
                        table {
                            thead { tr { th { "Location" } th { "Name" } th { "Required" } th { "Description" } } }
                            tbody {
                                tr { td { "Path" } td { code { "id" } } td { "Yes" } td { "Signed 64-bit beatmap ID. A value that cannot be parsed is looked up as ID 0 and normally returns 404." } }
                            }
                        }
                        h3 { "Response and statuses" }
                        p { code { "200 OK" } " — " code { "application/json" } "; one " a href="#beatmapset-schema" { "Beatmapset" } " object." }
                        ul {
                            li { code { "404 Not Found" } " — no indexed beatmapset contains the beatmap ID." }
                            li { code { "500 Internal Server Error" } " — the search index lookup failed." }
                        }
                        p { "Lookup errors do not have a documented JSON body." }
                        h3 { "Example" }
                        pre { code { "curl 'https://mirror.example/api/v1/beatmapsets/beatmap/4662168'" } }
                    }

                    section id="download" {
                        h2 { "Map download" }
                        p {
                            code { "GET /api/v1/download/:id" }
                            " and its exact short alias "
                            code { "GET /d/:id" }
                            " download an osu! beatmap archive. "
                            code { ":id" }
                            " is the numeric beatmapset ID."
                        }
                        h3 { "Parameters" }
                        table {
                            thead { tr { th { "Location" } th { "Name" } th { "Required/default" } th { "Description" } } }
                            tbody {
                                tr { td { "Path" } td { code { "id" } } td { "Required" } td { "Signed 64-bit beatmapset ID." } }
                                tr { td { "Query" } td { code { "video" } } td { code { "true" } " by default" } td { "Boolean: " code { "true" } " returns the normal archive; " code { "false" } " requests the upstream no-video variant. Only " code { "true" } " and " code { "false" } " are valid." } }
                            }
                        }
                        p class="note" { "The no-video variant uses the official upstream " code { "noVideo=1" } " option and a separate " code { "{id}_novid.osz" } " disk file and RAM cache key, so it never collides with the default " code { "{id}.osz" } " archive." }

                        h3 { "Streaming and cache behavior" }
                        p { "RAM and valid disk responses are cache hits; an origin download is a miss. Cached responses and upstream misses stream with backpressure. A miss is published to disk atomically only after the complete upstream body is validated and durably written. Byte-range requests are not supported; downloads return the complete archive." }
                        p { "The bounded smart RAM cache is populated lazily from eligible valid disk files or bytes already flowing through a request. Policy refresh never prefetches from the origin. Every ten minutes it retains candidates from the 50 latest ranked maps and the top 30 maps by successful download count, using the most recent successful download time and then map ID to break count ties. Entries outside that union are evicted, and byte capacity always wins, so a small cache may hold fewer maps. Video variants have independent byte entries." }
                        p { "RAM hits clone the retained byte buffer without copying its contents. An archive larger than the configured capacity is still served and kept on disk but is not admitted to RAM. Capacity bounds bytes owned or reserved by the cache; response clones already in flight may outlive eviction." }

                        h4 { "RAM capacity configuration" }
                        p { "The " code { "cache_size" } " setting is read from Mirria's confy YAML configuration at API startup. It defaults to " code { "\"10%\"" } ". Values are trimmed and case-insensitive:" }
                        pre { code { r#"cache_size: "2048MB"
# or
cache_size: "4GB"
# or
cache_size: "10%""# } }
                        ul {
                            li { code { "MB" } " and " code { "GB" } " are positive integer decimal byte units." }
                            li { "A percentage must be an integer from " code { "1%" } " through " code { "100%" } " and is resolved once against total host physical memory at startup." }
                            li { "Zero, missing or unknown suffixes, fractions, overflow and percentages above 100 are rejected; the API fails fast instead of running with an unintended capacity." }
                        }

                        h3 { "Successful response (200)" }
                        p { code { "application/x-osu-beatmap-archive" } " binary body." }
                        table {
                            thead { tr { th { "Header" } th { "Meaning" } } }
                            tbody {
                                tr { td { code { "Content-Type" } } td { code { "application/x-osu-beatmap-archive" } } }
                                tr { td { code { "Content-Disposition" } } td { "Attachment filename; UTF-8 names use RFC 5987 encoding." } }
                                tr { td { code { "Content-Length" } } td { "Archive length when known." } }
                                tr { td { code { "X-Cache-Hit" } } td { code { "hit" } " for RAM or disk, or " code { "miss" } " for an upstream stream." } }
                                tr { td { code { "X-RateLimit-Remaining" } } td { "Reservations still available in the applicable client window after this request was reserved." } }
                                tr { td { code { "X-RateLimit-Reset" } } td { "Whole seconds remaining until the applicable fixed window resets (0 through 5)." } }
                            }
                        }

                        h3 { "Rate limits" }
                        p { "Clients are identified by their direct peer IP address; forwarding headers are intentionally not trusted. Cached and non-cached downloads have independent five-second fixed windows." }
                        ul {
                            li { "Non-cached downloads: 10 successful downloads per window." }
                            li { "Cached downloads: 50 successful downloads per window." }
                        }
                        p { "Capacity is reserved before file or upstream work begins, preventing concurrent requests from bypassing a limit. A reservation is committed only when its response body completes successfully; upstream, cache-file, streaming or abandoned-client failures refund it." }

                        h3 { "Error responses" }
                        table {
                            thead { tr { th { "Status" } th { "Meaning" } th { "Body/headers" } } }
                            tbody {
                                tr { td { code { "400 Bad Request" } } td { "The path ID or video boolean could not be parsed." } td { "Axum rejection body; no stable JSON schema." } }
                                tr { td { code { "429 Too Many Requests" } } td { "The applicable client window has no capacity. No download or cache-file stream is started." } td { "JSON error body; " code { "X-RateLimit-Remaining: 0" } ", plus matching delta-seconds values in " code { "X-RateLimit-Reset" } " and " code { "Retry-After" } "." } }
                                tr { td { code { "500 Internal Server Error" } } td { "The local cache could not be prepared, inspected or opened." } td { code { r#"{"ok":false,"message":"..."}"# } } }
                                tr { td { code { "502 Bad Gateway" } } td { "The upstream download could not be started, declared an empty body or returned a non-success status." } td { code { r#"{"ok":false,"message":"..."}"# } } }
                            }
                        }

                        h3 { "Examples" }
                        pre { code { r#"# Default archive, including video when the set has one
curl -OJ 'https://mirror.example/api/v1/download/2556827'

# No-video variant through the short alias
curl -OJ 'https://mirror.example/d/2556827?video=false'"# } }
                    }

                    section id="search" {
                        h2 { code { "GET /api/v1/search" } }
                        p { "Runs a full-text and filtered search over indexed beatmapsets. The response is the hit list itself, not a pagination envelope." }
                        h3 { "Query parameters" }
                        table {
                            thead { tr { th { "Name" } th { "Type" } th { "Default" } th { "Behavior" } } }
                            tbody {
                                tr { td { code { "query" } } td { "String" } td { "Empty string" } td { "Full-text search text." } }
                                tr { td { code { "limit" } } td { "Signed integer" } td { code { "50" } } td { "Maximum hits passed to the search backend. The handler does not impose its own range validation." } }
                                tr { td { code { "offset" } } td { "Signed integer" } td { code { "0" } } td { "Hit offset passed to the search backend. The handler does not impose its own range validation." } }
                                tr { td { code { "statuses" } } td { "Array of strings" } td { code { "[ranked, loved, aproved, qualified]" } } td { "Status filter. The current default intentionally reflects the implemented spelling " code { "aproved" } "." } }
                                tr { td { code { "sort" } } td { "String" } td { code { "updated_desc" } } td { code { "updated_asc" } " sorts oldest update first; " code { "playcount" } " sorts play count ascending; every other value uses last-updated descending." } }
                                tr { td { code { "modes" } } td { "Array of enum strings" } td { code { "[osu, taiko, fruits, mania]" } } td { "Accepted values are " code { "osu" } ", " code { "taiko" } ", " code { "fruits" } " and " code { "mania" } "." } }
                            }
                        }
                        p { "Arrays use indexed serde_qs syntax, for example " code { "statuses[0]=ranked&statuses[1]=loved" } " and " code { "modes[0]=osu" } ". Percent-encode square brackets when required by the client." }

                        h3 { "Response and statuses" }
                        p { code { "200 OK" } " — " code { "application/json" } "; an array of " a href="#beatmapset-schema" { "Beatmapset" } " objects. An empty search result is " code { "[]" } "." }
                        ul {
                            li { code { "400 Bad Request" } " — malformed query syntax, type or ruleset value." }
                            li { code { "500 Internal Server Error" } " — the search backend failed." }
                        }
                        p { "Search errors do not have a documented JSON body." }
                        h3 { "Example" }
                        pre { code { "curl 'https://mirror.example/api/v1/search?query=camellia&limit=10&statuses%5B0%5D=ranked&modes%5B0%5D=osu&sort=updated_desc'" } }
                        pre { code { r#"[{"id":2556827,"artist":"Unlucky Morpheus","title":"Kamigami ga Koishita Gensoukyou","beatmaps":[],"pack_tags":[],"covers":{},"availability":{},"nominations_summary":{},"hype":null,"artist_unicode":null,"title_unicode":null,"creator":"tmk","user_id":1,"status":"ranked","bpm":180.0,"play_count":0,"favourite_count":0,"nsfw":false,"video":false,"storyboard":false,"is_scoreable":true,"source":"","tags":"","preview_url":"","offset":0,"spotlight":false,"ranked":1,"last_updated":"2026-08-17T15:27:44Z","submitted_date":"2026-08-17T15:27:44Z","ranked_date":null,"deleted_at":null,"can_be_hyped":false,"discussion_enabled":false,"discussion_locked":false,"legacy_thread_url":null,"track_id":null,"has_favourited":false}]"# } }
                        p { "The example shows the JSON shape with representative values; real metadata and nested arrays vary by result." }
                    }

                    section id="metrics" {
                        h2 { code { "GET /metrics" } }
                        p { "Returns the current Prometheus metrics exposition generated by the HTTP metrics recorder. Metric series and labels depend on observed runtime traffic." }
                        h3 { "Parameters" }
                        p { "No path or query parameters." }
                        h3 { "Response and statuses" }
                        p { code { "200 OK" } " — Prometheus text exposition returned as " code { "text/plain; charset=utf-8" } ". This handler defines no route-specific error status." }
                        h3 { "Example" }
                        pre { code { "curl 'https://mirror.example/metrics'" } }
                        pre { code { r#"# HELP <metric_name> <description>
# TYPE <metric_name> counter
<metric_name>{...} <value>"# } }
                    }

                    section id="docs" {
                        h2 { code { "GET /docs" } }
                        p { "Serves this generated, human-readable API reference." }
                        h3 { "Parameters" }
                        p { "No path or query parameters." }
                        h3 { "Response and statuses" }
                        p { code { "200 OK" } " — " code { "text/html; charset=utf-8" } ". This handler defines no route-specific error status." }
                        h3 { "Example" }
                        pre { code { "curl 'https://mirror.example/docs'" } }
                    }
                }
            }
        }
        .into_string(),
    )
}

pub fn serve() -> Router {
    Router::new().route("/docs", get(documentation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn generated_docs_cover_every_registered_route() {
        let Html(page) = documentation().await;

        for route in [
            "GET /api/v1/beatmaps/md5/:checksum",
            "GET /api/v1/beatmaps/:id",
            "GET /api/v1/beatmapsets/:id",
            "GET /api/v1/beatmapsets/beatmap/:id",
            "GET /api/v1/download/:id",
            "GET /d/:id",
            "GET /api/v1/search",
            "GET /metrics",
            "GET /docs",
        ] {
            assert!(page.contains(route), "missing documentation for {route}");
        }
    }

    #[tokio::test]
    async fn generated_docs_cover_route_contracts_and_errors() {
        let Html(page) = documentation().await;

        for expected in [
            "not an OpenAPI document",
            "Beatmapset",
            "Beatmap",
            "checksum",
            "video",
            "by default",
            "video=false",
            "{id}_novid.osz",
            "cache_size",
            "2048MB",
            "4GB",
            "10%",
            "50 latest ranked maps",
            "top 30 maps",
            "query",
            "limit",
            "offset",
            "statuses",
            "aproved",
            "updated_asc",
            "playcount",
            "modes",
            "X-Cache-Hit",
            "X-RateLimit-Remaining",
            "400 Bad Request",
            "404 Not Found",
            "429 Too Many Requests",
            "500 Internal Server Error",
            "502 Bad Gateway",
            "application/x-osu-beatmap-archive",
            "application/json",
            "text/plain; charset=utf-8",
            "text/html; charset=utf-8",
        ] {
            assert!(
                page.contains(expected),
                "missing documentation for {expected}"
            );
        }
    }
}
