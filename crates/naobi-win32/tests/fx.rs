use naobi_win32::fx::{
    append_bounded, fetch_https, parse_allowlisted_https_url, FxError, MAX_RESPONSE_BYTES,
    TIMEOUT_MS,
};

// @R-010
#[test]
fn r010_fx_fetcher_enforces_https_and_host_allowlist() {
    let frank = parse_allowlisted_https_url("https://api.frankfurter.dev/v1/latest?base=EUR")
        .expect("frankfurter URL allowed");
    assert_eq!(frank.host, "api.frankfurter.dev");
    assert_eq!(frank.path_and_query, "/v1/latest?base=EUR");

    let ecb = parse_allowlisted_https_url(
        "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml",
    )
    .expect("ecb URL allowed");
    assert_eq!(ecb.host, "www.ecb.europa.eu");
    assert_eq!(ecb.path_and_query, "/stats/eurofxref/eurofxref-daily.xml");

    for bad in [
        "",
        "http://api.frankfurter.dev/v1/latest?base=EUR",
        "HTTP://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml",
        "https://evil.example.com/v1/latest",
        "https://api.frankfurter.dev.evil.com/v1/latest",
        "https://user@api.frankfurter.dev/v1/latest",
        "https://api.frankfurter.dev:8443/v1/latest",
        "https://api.frankfurter.dev/v1/latest#frag",
        "https://api.frankfurter.dev/v1/latest\r\nX-Inject: 1",
        "file:///C:/Windows/win.ini",
    ] {
        assert!(parse_allowlisted_https_url(bad).is_err(), "{bad:?}");
        assert!(fetch_https(bad).is_err(), "{bad:?}");
    }
}

// @R-010
#[test]
fn r010_fx_fetcher_enforces_256kb_cap_and_5s_timeout_constants() {
    assert_eq!(MAX_RESPONSE_BYTES, 262_144);
    assert_eq!(TIMEOUT_MS, 5_000);

    let mut buf = Vec::new();
    let exact = vec![b'a'; MAX_RESPONSE_BYTES];
    assert!(append_bounded(&mut buf, &exact).is_ok());
    assert_eq!(buf.len(), MAX_RESPONSE_BYTES);

    let err = append_bounded(&mut buf, b"x").expect_err("262_145th byte must fail");
    assert_eq!(err, FxError::ResponseTooLarge);
}

// @R-010
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r010_winhttp_fx_fetch_hits_allowlisted_endpoint() {
    let frank = fetch_https("https://api.frankfurter.dev/v1/latest?base=EUR");
    let ecb = fetch_https("https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml");
    match (&frank, &ecb) {
        (Ok(body), _) => {
            assert!(
                body.contains("\"rates\"") && body.contains("\"USD\""),
                "{body}"
            );
            assert!(body.len() <= MAX_RESPONSE_BYTES);
        }
        (_, Ok(body)) => {
            assert!(body.contains("Cube") && body.contains("USD"), "{body}");
            assert!(body.len() <= MAX_RESPONSE_BYTES);
        }
        (Err(e1), Err(e2)) => panic!("both FX endpoints failed: frankfurter={e1}, ecb={e2}"),
    }
}
