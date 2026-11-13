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
    text.split("\\n").map(str::to_string).collect()
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
    emu.webkit.engine = Some(Box::new(page.clone()));
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
