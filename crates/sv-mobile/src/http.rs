// FilePath: crates/sv-mobile/src/http.rs
//! HTTPS on phones. reqwest's default verifier asks the operating system, which on Android
//! needs a JVM context and an extra Kotlin component, and the keyboard runs outside Tauri. The
//! client trusts Mozilla's root store (webpki-root-certs) instead, the same on every phone.

use sv_domain::{AppError, AppResult};

pub fn http_client() -> AppResult<reqwest::Client> {
    let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .map(|cert| reqwest::Certificate::from_der(cert.as_ref()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(AppError::network)?;
    reqwest::Client::builder()
        .tls_certs_only(roots)
        .build()
        .map_err(AppError::network)
}
