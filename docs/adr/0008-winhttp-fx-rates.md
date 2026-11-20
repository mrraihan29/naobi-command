# Allow hardened WinHttp in naobi-win32 for daily currency rates

Status: accepted
Amends: R-010 (strict network silence), deny.toml network feature ban

The Owner wants macOS Spotlight/Raycast-style inline currency conversion (`53 usd`, `53 usd to idr`) that stays accurate without manually shelling out to external tools. Rather than bundling third-party Rust HTTP stacks (which increase binary size and supply-chain surface) or abandoning automatic refresh, `naobi-win32` is permitted to enable `Win32_Networking_WinHttp` solely for fetching foreign exchange reference rates.

## Decision

- **Transport.** Only `WinHttp` inside `naobi-win32` may open an outbound HTTPS connection; Rust HTTP client crates (`reqwest`, `hyper`, `ureq`, etc.) remain banned in `deny.toml`.
- **Hardened client protocol.** Requests use HTTPS only (TLS 1.2+ with OS certificate validation enabled), a hardcoded host allowlist (`api.frankfurter.dev` primary, `www.ecb.europa.eu` fallback), zero cookies or tracking headers, zero user query or clipboard data in requests, a 5 s timeout, and a 256 KB maximum response size.
- **Caching and cadence.** Rates are cached locally in `%LOCALAPPDATA%\NaobiCommand\fx-rates.json` and refreshed at most once every 24 hours (or on an explicit Owner command). `naobi-calc` stays pure, offline, and socket-free, reading only the cached rate table.

## Considered options

- **Zero-socket core with out-of-process `curl.exe` / PowerShell refresh.** Rejected by the Owner in favor of native, self-contained `WinHttp` under strict security controls.
- **Third-party Rust HTTP client (`ureq` / `reqwest`).** Rejected to keep binary size well below the 20 MB budget (`R-006`) and rely on Windows' built-in Schannel TLS stack.

## Consequences

- `deny.toml` and `R-010` must carve out `Win32_Networking_WinHttp` while keeping all other network crates and sockets forbidden.
- `naobi-perf network-silence` (`R-010`) remains clean during normal search/idle sessions once the daily rate cache is fresh (or when `currency.auto_refresh` is disabled during audit runs).
