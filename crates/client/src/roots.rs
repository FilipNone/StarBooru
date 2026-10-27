//! Trusted roots for Android.
//!
//! reqwest's default verifier asks the OS through a Java helper that has to
//! be set up from JNI and packaged into the APK; without it the first HTTPS
//! request panics. Android also keeps its trust store as one PEM file per CA
//! in folders any app may read, so the app trusts exactly those instead.
//! Certificates the user added in Android's settings live where apps can't
//! read them and are not trusted.

#![cfg_attr(
    not(target_os = "android"),
    allow(dead_code, reason = "only Android reads its CA folder")
)]

use std::path::Path;

use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject;

/// The Conscrypt module's store (updated through the Play Store since
/// Android 14) first, then the one in the system image.
pub const ANDROID_CA_DIRS: [&str; 2] = [
    "/apex/com.android.conscrypt/cacerts",
    "/system/etc/security/cacerts",
];

/// The roots in the first of `dirs` that has any.
pub fn first_store(dirs: &[&str]) -> Vec<reqwest::Certificate> {
    dirs.iter()
        .map(|dir| certificates_in(Path::new(dir)))
        .find(|certs| !certs.is_empty())
        .unwrap_or_default()
}

/// Every usable root in the files of `dir`. Unreadable files and
/// certificates that aren't valid trust anchors are skipped, because one
/// bad one would make the whole client fail to build.
pub fn certificates_in(dir: &Path) -> Vec<reqwest::Certificate> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| std::fs::read(entry.path()).ok())
        .flat_map(|pem| certificates_from(&pem))
        .collect()
}

/// The valid trust anchors among the PEM certificates in `pem`. Text
/// around the blocks (Android's files start with a readable dump) is
/// ignored.
pub fn certificates_from(pem: &[u8]) -> Vec<reqwest::Certificate> {
    CertificateDer::pem_slice_iter(pem)
        .flatten()
        .filter(|der| webpki::anchor_from_trusted_cert(der).is_ok())
        .filter_map(|der| reqwest::Certificate::from_der(der.as_ref()).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = include_str!("../testdata/isrg-root-x1.pem");

    #[test]
    fn reads_android_style_files() {
        assert_eq!(certificates_from(ROOT.as_bytes()).len(), 1);
        let with_dump = format!("Certificate:\n    Data:\n        Version: 3 (0x2)\n{ROOT}");
        assert_eq!(certificates_from(with_dump.as_bytes()).len(), 1);
        assert_eq!(
            certificates_from(format!("{ROOT}{ROOT}").as_bytes()).len(),
            2
        );
    }

    #[test]
    fn skips_what_is_not_a_root() {
        assert!(certificates_from(b"").is_empty());
        assert!(certificates_from(b"not a certificate").is_empty());
        let garbled = "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n";
        assert!(certificates_from(garbled.as_bytes()).is_empty());
        assert!(certificates_from(format!("{garbled}{ROOT}").as_bytes()).len() == 1);
    }

    #[test]
    fn a_missing_folder_is_empty() {
        assert!(certificates_in(Path::new("/nonexistent/starbooru-cacerts")).is_empty());
        assert!(first_store(&["/nonexistent/a", "/nonexistent/b"]).is_empty());
    }

    #[test]
    fn takes_the_first_folder_with_roots() {
        let dir = std::env::temp_dir().join(format!("starbooru-roots-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.0"), ROOT).unwrap();
        std::fs::write(dir.join("junk"), "junk").unwrap();
        let path = dir.to_str().unwrap();
        assert_eq!(first_store(&["/nonexistent/a", path]).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
