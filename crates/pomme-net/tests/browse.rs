//! Bonjour browsing on the computer's network (`LiveNet::browse`). Needs a
//! `_expo._tcp` service advertised (Metro, which Expo CLI advertises unless `CI` is set, or
//! `dns-sd -R`), so it runs only with POMME_BONJOUR set to the name to find.
use pomme_hle::net::Net;

#[test]
fn finds_an_advertised_service() {
    let Ok(name) = std::env::var("POMME_BONJOUR") else {
        eprintln!("skipped: set POMME_BONJOUR to an advertised _expo._tcp service's name");
        return;
    };
    let mut net = pomme_net::LiveNet::new(Default::default());
    let mut found = Vec::new();
    for _ in 0..50 {
        found = net.browse("_expo._tcp");
        if found.iter().any(|s| s.name == name) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let s = found.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name} not found: {found:?}"));
    assert_eq!(s.kind, "_expo._tcp");
    assert!(s.port != 0 && !s.addresses.is_empty(), "{s:?}");
    println!("{s:?}");
}
