//! Verschluesselter Ablageort fuer Tokens und API-Schluessel.
//!
//! Warum nicht einfach in `config.json`: die Konfiguration soll exportierbar und
//! lesbar sein. Ein Bearer-Token fuer die eigene NocoDB oder ein
//! Anthropic-Schluessel darf das nicht sein.
//!
//! Aufbau: zwei Dateien im privaten App-Verzeichnis. `device.key` haelt 32
//! Zufallsbytes, `secrets.bin` die mit AES-256-GCM verschluesselte Map. Auf
//! Android liegt das private Verzeichnis in der App-Sandbox und ist ohne Root
//! fuer andere Apps nicht lesbar; auf dem Desktop schuetzen die Dateirechte
//! (0600).
//!
//! Was das **nicht** ist: hardwaregestuetzte Schluesselhaltung. Der Geraeteschluessel
//! liegt neben den Daten. Gegen einen Angreifer mit Dateizugriff hilft das nicht —
//! dafuer braucht es den Android Keystore, siehe `docs/roadmap-ki.md`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;

use crate::error::{Error, Result};

const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// Alle Geheimnisse als Name -> Wert. Wird nie serialisiert ausgegeben.
pub struct SecretStore {
    key_path: PathBuf,
    data_path: PathBuf,
    entries: BTreeMap<String, String>,
}

/// Eigene Ausgabe statt `derive(Debug)`: ein abgeleitetes `Debug` wuerde die
/// Werte in jedes Log und jede Panik-Meldung schreiben.
impl std::fmt::Debug for SecretStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretStore")
            .field("data_path", &self.data_path)
            .field("entries", &format_args!("{} Eintraege (verborgen)", self.entries.len()))
            .finish()
    }
}

impl SecretStore {
    /// Oeffnet den Store im angegebenen Verzeichnis und legt ihn an, falls er
    /// noch nicht existiert.
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let key_path = dir.join("device.key");
        let data_path = dir.join("secrets.bin");
        let mut store = Self { key_path, data_path, entries: BTreeMap::new() };
        store.load()?;
        Ok(store)
    }

    fn cipher(&self) -> Result<Aes256Gcm> {
        let raw = if self.key_path.exists() {
            let raw = fs::read(&self.key_path)?;
            if raw.len() != KEY_LEN {
                return Err(Error::Secrets(format!(
                    "device.key ist {} Bytes lang, erwartet {KEY_LEN}",
                    raw.len()
                )));
            }
            raw
        } else {
            let mut raw = vec![0u8; KEY_LEN];
            OsRng.fill_bytes(&mut raw);
            write_private(&self.key_path, &raw)?;
            raw
        };
        let key = Key::<Aes256Gcm>::from_slice(&raw);
        Ok(Aes256Gcm::new(key))
    }

    fn load(&mut self) -> Result<()> {
        if !self.data_path.exists() {
            return Ok(());
        }
        let blob = fs::read(&self.data_path)?;
        if blob.is_empty() {
            return Ok(());
        }
        if blob.len() <= NONCE_LEN {
            return Err(Error::Secrets("secrets.bin ist unvollstaendig".into()));
        }
        let cipher = self.cipher()?;
        let (nonce, ciphertext) = blob.split_at(NONCE_LEN);
        let plain = cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| Error::Secrets("secrets.bin passt nicht zu device.key".into()))?;
        self.entries = serde_json::from_slice(&plain)?;
        Ok(())
    }

    fn persist(&self) -> Result<()> {
        let cipher = self.cipher()?;
        let plain = serde_json::to_vec(&self.entries)?;
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plain.as_ref())
            .map_err(|_| Error::Secrets("Verschluesseln fehlgeschlagen".into()))?;
        let mut blob = nonce.to_vec();
        blob.extend_from_slice(&ciphertext);
        write_private(&self.data_path, &blob)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(String::as_str)
    }

    /// Wie [`Self::get`], aber mit sprechendem Fehler statt `None` — die
    /// Connectoren melden damit [`crate::model::SyncFault::Misconfigured`].
    pub fn require(&self, name: &str) -> Result<&str> {
        self.get(name)
            .ok_or_else(|| Error::Secrets(format!("Kein Eintrag '{name}' hinterlegt")))
    }

    pub fn set(&mut self, name: &str, value: &str) -> Result<()> {
        self.entries.insert(name.to_string(), value.to_string());
        self.persist()
    }

    pub fn remove(&mut self, name: &str) -> Result<()> {
        self.entries.remove(name);
        self.persist()
    }

    /// Nur die Namen — fuer die Einstellungen ("hinterlegt: ja/nein"), niemals
    /// die Werte.
    pub fn names(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Entfernt Eintraege, auf die die Konfiguration nicht mehr verweist.
    pub fn retain_only(&mut self, keep: &[String]) -> Result<usize> {
        let before = self.entries.len();
        self.entries.retain(|k, _| keep.iter().any(|n| n == k));
        let removed = before - self.entries.len();
        if removed > 0 {
            self.persist()?;
        }
        Ok(removed)
    }
}

/// Schreibt eine Datei, die nur dem Besitzer gehoert.
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Erst in eine Nebendatei, dann umbenennen: ein abgebrochener Schreibvorgang
    // hinterlaesst so keinen halben Secret-Store.
    let tmp = path.with_extension("tmp");
    let mut file = fs::File::create(&tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tory-secrets-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn schreibt_und_liest_wieder() {
        let dir = tempdir("roundtrip");
        let mut store = SecretStore::open(&dir).unwrap();
        store.set("nocodb.haupt", "geheim-token").unwrap();
        store.set("ai.anthropic", "sk-ant-xyz").unwrap();

        let wieder = SecretStore::open(&dir).unwrap();
        assert_eq!(wieder.get("nocodb.haupt"), Some("geheim-token"));
        assert_eq!(wieder.get("ai.anthropic"), Some("sk-ant-xyz"));
        assert_eq!(wieder.names(), vec!["ai.anthropic", "nocodb.haupt"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn datei_enthaelt_den_wert_nicht_im_klartext() {
        let dir = tempdir("opaque");
        let mut store = SecretStore::open(&dir).unwrap();
        store.set("t", "streng-geheim").unwrap();
        let blob = fs::read(dir.join("secrets.bin")).unwrap();
        assert!(!String::from_utf8_lossy(&blob).contains("streng-geheim"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn falscher_geraeteschluessel_faellt_auf() {
        let dir = tempdir("wrongkey");
        let mut store = SecretStore::open(&dir).unwrap();
        store.set("t", "wert").unwrap();
        let mut andere = vec![0u8; KEY_LEN];
        OsRng.fill_bytes(&mut andere);
        write_private(&dir.join("device.key"), &andere).unwrap();
        let err = SecretStore::open(&dir).unwrap_err();
        assert!(matches!(err, Error::Secrets(_)), "unerwartet: {err:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn raeumt_verwaiste_eintraege_auf() {
        let dir = tempdir("retain");
        let mut store = SecretStore::open(&dir).unwrap();
        store.set("bleibt", "a").unwrap();
        store.set("geht", "b").unwrap();
        assert_eq!(store.retain_only(&["bleibt".to_string()]).unwrap(), 1);
        assert!(store.has("bleibt"));
        assert!(!store.has("geht"));
        let _ = fs::remove_dir_all(&dir);
    }
}
