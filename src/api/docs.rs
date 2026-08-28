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
                        body { max-width: 70rem; margin: 2rem auto; padding: 0 1rem; line-height: 1.5; }
                        code { background: color-mix(in srgb, CanvasText 10%, Canvas); padding: .1rem .3rem; }
                        table { border-collapse: collapse; width: 100%; }
                        th, td { border: 1px solid color-mix(in srgb, CanvasText 25%, Canvas); padding: .5rem; text-align: left; vertical-align: top; }
                    "# }
                }
                body {
                    h1 { "Mirria HTTP API" }
                    p { "This human-readable reference is generated with Maud and is not an OpenAPI document." }

                    h2 { "Map download" }
                    p {
                        code { "GET /api/v1/download/:id" }
                        " and the short alias "
                        code { "GET /d/:id" }
                        " download an osu! beatmap archive. "
                        code { ":id" }
                        " is the numeric beatmap-set ID."
                    }
                    p { "Cached responses and upstream cache misses are streamed with backpressure. A miss is published to the cache atomically only after the complete upstream body has been received and durably written. Byte-range requests are not supported; downloads return the complete archive." }

                    h3 { "Successful response (200)" }
                    table {
                        thead { tr { th { "Header" } th { "Meaning" } } }
                        tbody {
                            tr { td { code { "Content-Type" } } td { code { "application/x-osu-beatmap-archive" } } }
                            tr { td { code { "Content-Disposition" } } td { "Attachment filename; UTF-8 names use RFC 5987 encoding." } }
                            tr { td { code { "Content-Length" } } td { "Archive length when known." } }
                            tr { td { code { "X-Cache-Hit" } } td { code { "hit" } " for a cached file or " code { "miss" } " for an upstream stream." } }
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
                    p { "Capacity is reserved before file or upstream work begins, preventing concurrent requests from bypassing a limit. A reservation is committed only when its response body completes successfully; upstream, cache-file, streaming, or abandoned-client failures refund it." }

                    h3 { "Error responses" }
                    table {
                        thead { tr { th { "Status" } th { "Meaning" } th { "Headers" } } }
                        tbody {
                            tr { td { code { "429 Too Many Requests" } } td { "The applicable client window has no capacity. No download or cache-file stream is started." } td { code { "X-RateLimit-Remaining: 0" } ", plus matching delta-seconds values in " code { "X-RateLimit-Reset" } " and " code { "Retry-After" } } }
                            tr { td { code { "500 Internal Server Error" } } td { "The local cache could not be prepared, inspected, or opened." } td { "JSON error body." } }
                            tr { td { code { "502 Bad Gateway" } } td { "The upstream download could not be started or returned a non-success status." } td { "JSON error body." } }
                        }
                    }

                    h2 { "Documentation" }
                    p { code { "GET /docs" } " serves this page." }
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
    async fn generated_docs_cover_download_contract() {
        let Html(page) = documentation().await;

        for expected in [
            "GET /api/v1/download/:id",
            "X-Cache-Hit",
            "X-RateLimit-Remaining",
            "Whole seconds remaining",
            "429 Too Many Requests",
            "GET /docs",
        ] {
            assert!(
                page.contains(expected),
                "missing documentation for {expected}"
            );
        }
    }
}
