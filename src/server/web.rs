//! The web dashboard at `/`: the files under `web/`, compiled into the
//! binary, plus the browser-facing security headers every response carries.
//!
//! The page itself holds no data and needs no token. It reads the same
//! `/api/*` endpoints as any other client, from this origin only; when the
//! server has a token, the page asks for it and keeps it in the tab's
//! session storage, sending it as an `Authorization` header.

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

/// Scripts, styles, images and fetches from this origin only; no inline
/// code, plugins, framing or form posts; DOM injection sinks closed to plain
/// strings (Trusted Types, where the browser supports it).
pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
img-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; \
frame-ancestors 'none'; require-trusted-types-for 'script'; trusted-types 'none'";

struct Asset {
    path: &'static str,
    content_type: &'static str,
    body: &'static str,
}

const ASSETS: [Asset; 4] = [
    Asset {
        path: "/",
        content_type: "text/html; charset=utf-8",
        body: include_str!("../../web/index.html"),
    },
    Asset {
        path: "/assets/app.js",
        content_type: "text/javascript; charset=utf-8",
        body: include_str!("../../web/app.js"),
    },
    Asset {
        path: "/assets/app.css",
        content_type: "text/css; charset=utf-8",
        body: include_str!("../../web/app.css"),
    },
    Asset {
        path: "/assets/favicon.svg",
        content_type: "image/svg+xml",
        body: include_str!("../../web/favicon.svg"),
    },
];

/// One GET route per file, so metrics see a fixed set of route labels.
pub fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    ASSETS.iter().fold(Router::new(), |router, asset| {
        let (content_type, body) = (asset.content_type, asset.body);
        router.route(
            asset.path,
            get(move || async move { ([(header::CONTENT_TYPE, content_type)], body).into_response() }),
        )
    })
}

/// Browser hardening on every response, the JSON API included.
pub fn secure(headers: &mut HeaderMap) {
    let set = |headers: &mut HeaderMap, name: HeaderName, value: &'static str| {
        headers.insert(name, HeaderValue::from_static(value));
    };
    set(headers, header::CONTENT_SECURITY_POLICY, CSP);
    set(headers, header::X_FRAME_OPTIONS, "DENY");
    set(headers, header::REFERRER_POLICY, "no-referrer");
    set(
        headers,
        HeaderName::from_static("cross-origin-opener-policy"),
        "same-origin",
    );
    set(
        headers,
        HeaderName::from_static("cross-origin-resource-policy"),
        "same-origin",
    );
    set(
        headers,
        HeaderName::from_static("permissions-policy"),
        "camera=(), microphone=(), geolocation=(), payment=(), usb=()",
    );
}

/// Browsers say where a request comes from (`Sec-Fetch-Site`). Data is only
/// for this server's own page, or for clients that aren't browsers (curl,
/// Prometheus), which send no such header. A page on another site, or on
/// another localhost port, gets 403 instead of relying on CORS alone. The
/// server never sends `Access-Control-Allow-*` headers.
pub async fn same_origin(req: Request, next: Next) -> Response {
    let site = req
        .headers()
        .get("sec-fetch-site")
        .and_then(|v| v.to_str().ok());
    if site.is_some_and(|s| s != "same-origin" && s != "none") {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "Cross-origin requests are not allowed; open the dashboard from this server"
            })),
        )
            .into_response();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(path: &str) -> &'static str {
        ASSETS.iter().find(|a| a.path == path).unwrap().body
    }

    /// Lower-cased so the checks can't be dodged with `onClick` or `<SCRIPT`.
    fn lower(path: &str) -> String {
        asset(path).to_ascii_lowercase()
    }

    #[test]
    fn csp_is_strict() {
        for part in [
            "default-src 'none'",
            "script-src 'self'",
            "style-src 'self'",
            "connect-src 'self'",
            "frame-ancestors 'none'",
            "form-action 'none'",
            "base-uri 'none'",
        ] {
            assert!(CSP.contains(part), "{part}");
        }
        assert!(!CSP.contains("unsafe"), "{CSP}");
        assert!(!CSP.contains('*'), "{CSP}");
        assert!(!CSP.contains("http"), "{CSP}");
    }

    #[test]
    fn html_has_no_inline_code_or_styles() {
        let html = lower("/");
        // Every <script> loads a file from this server.
        let scripts: Vec<&str> = html.split("<script").skip(1).collect();
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].starts_with(" src=\"/assets/app.js\""));
        assert!(!html.contains("<style"));
        assert!(!html.contains(" style="));
        assert!(!html.contains("javascript:"));
        // No inline event handlers (onclick=, onload=, ...).
        for (i, _) in html.match_indices(" on") {
            let rest = &html[i + 3..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect();
            assert!(
                !rest[name.len()..].starts_with('='),
                "inline handler on{name}= in index.html"
            );
        }
    }

    #[test]
    fn script_never_parses_markup_or_evaluates_strings() {
        let js = lower("/assets/app.js");
        for banned in [
            "innerhtml",
            "outerhtml",
            "insertadjacenthtml",
            "document.write",
            "eval(",
            "new function",
            "createcontextualfragment",
            "domparser",
            "localstorage",
        ] {
            assert!(!js.contains(banned), "app.js uses {banned}");
        }
        // The token is only ever sent in a header.
        assert!(js.contains("headers.authorization = 'bearer ' + token"));
        assert!(js.contains("sessionstorage"));
    }

    #[test]
    fn page_loads_nothing_from_other_origins() {
        for asset in &ASSETS {
            let text = asset.body.to_ascii_lowercase();
            // The SVG namespace is a name, not a fetch.
            let text = text.replace("http://www.w3.org/2000/svg", "");
            assert!(!text.contains("http://"), "{}", asset.path);
            assert!(!text.contains("https://"), "{}", asset.path);
            assert!(!text.contains("//cdn"), "{}", asset.path);
        }
        // Only GET: the dashboard has no way to change anything.
        let js = lower("/assets/app.js");
        for method in ["'post'", "'put'", "'delete'", "'patch'", "method:"] {
            assert!(!js.contains(method), "app.js sends {method}");
        }
    }
}
