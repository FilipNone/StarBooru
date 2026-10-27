//! A short, human description of a client ("Firefox on Linux") from its
//! User-Agent, for the session list. Only this description is stored.

/// What the StarBooru apps send, followed by `/<version> (<os>)`.
pub const APP_AGENT: &str = "StarBooru-app";

const MAX_TOOL_NAME: usize = 32;

pub fn describe_user_agent(ua: &str) -> String {
    let ua = ua.trim();
    if let Some(rest) = ua.strip_prefix(APP_AGENT) {
        let os = rest
            .split_once('(')
            .and_then(|(_, r)| r.split_once(')'))
            .map(|(os, _)| app_os(os.trim()));
        return match os {
            Some(os) => format!("StarBooru app on {os}"),
            None => "StarBooru app".to_owned(),
        };
    }
    let browser = [
        ("Edg/", "Edge"),
        ("OPR/", "Opera"),
        ("Firefox/", "Firefox"),
        ("Chromium/", "Chromium"),
        ("Chrome/", "Chrome"),
        ("Safari/", "Safari"),
    ]
    .into_iter()
    .find(|(marker, _)| ua.contains(marker))
    .map(|(_, name)| name);
    let os = [
        ("Android", "Android"),
        ("iPhone", "iOS"),
        ("iPad", "iPadOS"),
        ("CrOS", "ChromeOS"),
        ("Windows", "Windows"),
        ("Macintosh", "macOS"),
        ("Mac OS X", "macOS"),
        ("Linux", "Linux"),
    ]
    .into_iter()
    .find(|(marker, _)| ua.contains(marker))
    .map(|(_, name)| name);
    match (browser, os) {
        (Some(b), Some(o)) => format!("{b} on {o}"),
        (Some(b), None) => b.to_owned(),
        (None, Some(o)) => o.to_owned(),
        // Tools like `curl/8.0`: the name before the slash, if it looks like one.
        (None, None) => ua
            .split('/')
            .next()
            .filter(|name| {
                !name.is_empty()
                    && name.len() <= MAX_TOOL_NAME
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            })
            .map_or_else(|| "Unknown client".to_owned(), str::to_owned),
    }
}

/// `std::env::consts::OS` names, made readable.
fn app_os(os: &str) -> &'static str {
    match os {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        "android" => "Android",
        "ios" => "iOS",
        _ => "another system",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_browsers_apps_and_tools() {
        let cases = [
            (
                "Mozilla/5.0 (X11; Linux x86_64; rv:131.0) Gecko/20100101 Firefox/131.0",
                "Firefox on Linux",
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0",
                "Edge on Windows",
            ),
            (
                "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Mobile Safari/537.36",
                "Chrome on Android",
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
                "Safari on iOS",
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15",
                "Safari on macOS",
            ),
            ("StarBooru-app/0.1.0 (linux)", "StarBooru app on Linux"),
            (
                "StarBooru-app/0.1.0 (plan9)",
                "StarBooru app on another system",
            ),
            ("StarBooru-app/0.1.0", "StarBooru app"),
            ("curl/8.10.1", "curl"),
            ("", "Unknown client"),
            ("<script>alert(1)</script>", "Unknown client"),
            (&"x".repeat(5000), "Unknown client"),
        ];
        for (ua, want) in cases {
            assert_eq!(describe_user_agent(ua), want, "{ua}");
        }
    }
}
