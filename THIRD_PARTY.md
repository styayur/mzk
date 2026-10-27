# Third-party boundaries

The MIT license covers mzk's own code and documentation only. Jamendo names are attribution, not endorsement. Music, metadata and provider services retain their respective rights and terms; no audio is bundled. mpv is a separate executable under its own build-specific licenses; it is neither downloaded nor redistributed by mzk.

Rust dependency versions are fixed in Cargo.lock. The package script emits `dependencies.json` from Cargo metadata, recording versions, declared licenses and source repositories, and copies available crate license/notice files into `dependency-licenses`. That inventory includes test/target dependencies and is not a claim that all are linked into each binary. Consult the matching crates for source and complete terms. Dependencies are not relicensed as MIT merely by being used here. TLS includes rustls and AWS-LC and their upstream terms. No fuzzy-matching framework or music dataset is bundled.
