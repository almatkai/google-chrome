# Parent verification and preview scope

Requires Tezbar 0.3.0-beta.12 or later. The published app at the start of this work was beta.11; beta.12 app publication is a separate gate. This is a Rust development preview, not blanket upstream parity or live desktop/account acceptance.

Independent source review was completed before these repairs. Original freezes and their reports remain historical; the parent repaired source supersedes them. Builds ran locally on Windows and on an Apple Silicon Mac over Tailscale. No GitHub Actions builds or real service-account mutations were used. Source and immutable SDK provenance, original resources, Cargo.lock and dependency notices are retained.

The repaired source passed 12 Rust tests and nine real JSONL fixture checks on each platform. A mocked JXA regression verifies stable tab IDs when same-URL tabs shift; native JXA syntax parsing also passed. Browser-only commands no longer depend on unrelated Local State, and pagination stops with guidance at its cap. The Mac package uses an optimized native ARM executable; the Windows package is only a protocol probe and is not published as Windows browser support.

Google Chrome is absent on the build Mac. Actual browser dictionary operations, permissions, GPUI interaction and legacy cache migration remain unverified. Seven inbound AI tools and preferred-app Quick Link binding remain gaps. See PORT-REPORT.md.
