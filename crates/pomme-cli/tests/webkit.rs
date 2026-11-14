//! End to end: WKWebView with Google's YouTube iOS player helper
//! (tests/fixtures/ytplayer). The page is a scripted stand-in for YouTube's
//! iframe player (`webkit::Scripted`): it does what the real one does at the
//! points the helper listens to (navigating to `ytplayer://` callbacks once
//! the player is ready and as playback changes, answering the player's
//! getters), so the app's side — the helper's HTML and JSON, navigation
//! policy, script evaluation and its results — runs as on a phone, without a
//! browser or the network. Built with the iOS SDK; skipped without Xcode or
//! the sysroot.

mod common;

use pomme_hle::webkit::Scripted;
use serde_json::json;

/// The label's lines (the delegate's events).
fn events(views: &str) -> Vec<String> {
    let start = views.find("tag 100").unwrap_or_else(|| panic!("no event label in\n{views}"));
    let line = &views[start..];
    let text = line.split_once('"').map(|(_, t)| t).unwrap_or("");
    let text = text[..text.find("\"\n").unwrap_or(text.len())].to_string();
    text.split("\\n").map(|l| l.replace("\\\"", "\"")).collect()
}

#[test]
fn youtube_player_helper() {
    let Some(mut emu) = common::boot("ytplayer") else { return };
    // YouTube's iframe player, as the helper sees it: ready once its API has
    // loaded, buffering then playing when asked to play, and its getters.
    let page = Scripted::new()
        .on_load(|p, doc| {
            if doc.contains("new YT.Player('player'") {
                p.navigate("ytplayer://onYouTubeIframeAPIReady");
                p.navigate("ytplayer://onReady?data=null");
            }
        })
        .on_eval("player.playVideo();", |p, _| {
            p.navigate("ytplayer://onStateChange?data=3");
            p.navigate("ytplayer://onStateChange?data=1");
            Ok(None)
        })
        .on_eval("player.getDuration();", |_, _| Ok(Some(json!(213.0))))
        .on_eval("player.getPlayerState();", |_, _| Ok(Some(json!(1))));
    *emu.webkit.engine = Some(Box::new(page.clone()));
    let views = common::views(&mut emu, 3.0);
    assert_eq!(events(&views), ["loading", "ready", "duration 213", "state buffering", "state playing", "asked: playing"], "{views}");

    let log = page.log();
    // The helper's page: its template with the player's parameters, at the app's origin.
    let (html, base) = &log.loaded[0];
    assert_eq!(base.as_deref(), Some("http://dev.pomme.ytplayer"));
    assert!(html.contains("<script src=\"https://www.youtube.com/iframe_api\""), "{html}");
    assert!(html.contains("\"videoId\" : \"M7lc1UVf-VE\""), "{html}");
    assert!(html.contains("\"origin\" : \"http:\\/\\/dev.pomme.ytplayer\""), "{html}");
    assert!(!html.contains("%@"), "the template was filled in");
    // Its own origin it lets load; the player's callbacks it takes and cancels.
    assert_eq!(log.allowed, ["http://dev.pomme.ytplayer"]);
    assert_eq!(log.cancelled, ["ytplayer://onYouTubeIframeAPIReady", "ytplayer://onReady?data=null", "ytplayer://onStateChange?data=3", "ytplayer://onStateChange?data=1"]);
    assert_eq!(log.scripts, ["player.getDuration();", "player.playVideo();", "player.getPlayerState();"]);
}

/// With no web engine, the page never loads: the helper's view is there, and
/// nothing comes from the player (the load fails as WebKit's would).
#[test]
fn without_an_engine() {
    let Some(mut emu) = common::boot("ytplayer") else { return };
    let views = common::views(&mut emu, 2.0);
    assert_eq!(events(&views), ["loading"], "{views}");
    assert!(views.contains("WKWebView"), "{views}");
}

/// The host's web view (WebKit, in the pomme-webview helper): the app's own
/// page loads at its base URL, its message reaches the handler, its made-up
/// URL reaches the navigation delegate (which cancels it), the user script ran,
/// script results convert as WebKit's do, a tap presses the page's button
/// and a drag scrolls it, and the page's pixels are drawn where the web view is. Offline: the page is a string. macOS only.
#[cfg(target_os = "macos")]
#[test]
fn host_web_view() {
    let Some(mut emu) = common::boot("webview") else { return };
    let engine = pomme_webview::WebViewEngine::start(std::path::Path::new(env!("CARGO_BIN_EXE_pomme-webview"))).expect("start the helper");
    *emu.webkit.engine = Some(Box::new(engine));
    // A tap on the page's button, then a drag up the page.
    pomme_hle::touch::tap(&mut emu.touch.script, 2_000_000_000, 195.0, 200.0);
    pomme_hle::touch::swipe(&mut emu.touch.script, 3_000_000_000, (195.0, 280.0), (195.0, 130.0), 300_000_000);
    let views = common::views(&mut emu, 5.0);
    let mut lines = events(&views);
    lines.sort();
    assert_eq!(
        lines,
        [
            "allow https://example.com/",
            "called home: app://hello?from=example.com",
            "finished https://example.com/",
            r#"message app: "scrolled down""#,
            r#"message app: "tapped""#,
            r#"message app: {"hello":"pomme","n":3}"#,
            r#"result {"fromUserScript":42,"list":[true,"x"],"sum":3,"title":""}"#,
            "undefined is nil",
        ],
        "{views}"
    );
    // The page is red, where the web view is (0, 100, 390 × 200), and nowhere else.
    let pix = pomme_hle::render::render(&mut emu, 1.0).unwrap();
    let at = |x: u32, y: u32| pix.pixel(x, y).map(|p| (p.red(), p.green(), p.blue())).unwrap();
    assert_eq!(at(195, 200), (255, 0, 0));
    assert_eq!(at(195, 50), (255, 255, 255));
}

/// Real YouTube in the host's web view: the video loads and plays. Needs the
/// network, so it runs only with POMME_ONLINE=1.
#[cfg(target_os = "macos")]
#[test]
fn youtube_player_helper_online() {
    if std::env::var_os("POMME_ONLINE").is_none() {
        eprintln!("skipping: set POMME_ONLINE=1 to play a YouTube video");
        return;
    }
    let Some(mut emu) = common::boot("ytplayer") else { return };
    *emu.net.client = None;
    let engine = pomme_webview::WebViewEngine::start(std::path::Path::new(env!("CARGO_BIN_EXE_pomme-webview"))).expect("start the helper");
    *emu.webkit.engine = Some(Box::new(engine));
    let views = common::views(&mut emu, 15.0);
    let lines = events(&views);
    for want in ["ready", "duration 1344", "state playing", "asked: playing"] {
        assert!(lines.iter().any(|l| l == want), "no {want:?} in {lines:?}");
    }
}
