use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use rand::{rngs::OsRng, RngCore};
use rusqlite::{params, Connection};
use std::path::Path;
use tracing::info;

pub struct StorageManager {
    conn: Connection,
    cipher: Option<Aes256Gcm>,
}

impl StorageManager {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path).context("Failed to open database")?;
        Self::init_with_conn(conn)
    }

    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("Failed to open in-memory database")?;
        Self::init_with_conn(conn)
    }

    fn init_with_conn(conn: Connection) -> Result<Self> {
        // Initialize schema
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value BLOB NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS bookmarks (
                id INTEGER PRIMARY KEY,
                url BLOB NOT NULL,
                title BLOB NOT NULL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY,
                url BLOB NOT NULL,
                title BLOB NOT NULL,
                visited_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS downloads (
                id INTEGER PRIMARY KEY,
                url BLOB NOT NULL,
                filename BLOB NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS tab_states (
                tab_id INTEGER PRIMARY KEY,
                state BLOB NOT NULL,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS shield_allowlist (
                host TEXT PRIMARY KEY,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        Ok(Self { conn, cipher: None })
    }

    /// Unlock the storage using the user's secret key.
    /// In Sentinel, this key is derived from the ZK-Identity secret.
    pub fn unlock(&mut self, user_secret: &[u8]) -> Result<()> {
        info!("Unlocking encrypted storage...");
        
        // 1. Retrieve or generate the salt
        let salt_str = match self.get_plaintext_setting("storage_salt")? {
            Some(s) => s,
            None => {
                let mut salt_bytes = [0u8; 16];
                OsRng.fill_bytes(&mut salt_bytes);
                let s = hex::encode(salt_bytes);
                self.set_plaintext_setting("storage_salt", &s)?;
                s
            }
        };

        let salt = SaltString::from_b64(&salt_str)
            .map_err(|e| anyhow::anyhow!("Salt error: {}", e))?;
        
        let argon2 = Argon2::default();
        let mut key = [0u8; 32];
        
        // Use the user_secret to derive the AES key
        let password_hash = argon2
            .hash_password(user_secret, &salt)
            .map_err(|e| anyhow::anyhow!("KDF error: {}", e))?;
        
        // Use the derived key bytes from Argon2id directly as the 256-bit AES key.
        let output = password_hash.hash.context("KDF output generation failed")?;
        key.copy_from_slice(&output.as_bytes()[..32]);
        
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| anyhow::anyhow!("Cipher init error: {}", e))?;
            
        self.cipher = Some(cipher);
        info!("Storage unlocked successfully.");
        Ok(())
    }

    fn set_plaintext_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value.as_bytes()],
        )?;
        Ok(())
    }

    fn get_plaintext_setting(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;

        if let Some(row) = rows.next()? {
            let value_bytes: Vec<u8> = row.get(0)?;
            Ok(Some(String::from_utf8(value_bytes)?))
        } else {
            Ok(None)
        }
    }

    fn encrypt(&self, data: &str) -> Result<Vec<u8>> {
        let cipher = self.cipher.as_ref().context("Storage is locked")?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let ciphertext = cipher
            .encrypt(nonce, data.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption error: {}", e))?;
            
        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend(ciphertext);
        Ok(result)
    }

    fn decrypt(&self, encrypted_data: &[u8]) -> Result<String> {
        let cipher = self.cipher.as_ref().context("Storage is locked")?;
        if encrypted_data.len() < 12 {
            return Err(anyhow::anyhow!("Invalid encrypted data"));
        }
        
        let nonce = Nonce::from_slice(&encrypted_data[..12]);
        let ciphertext = &encrypted_data[12..];
        
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption error: {}", e))?;
            
        String::from_utf8(plaintext).context("Invalid UTF-8 after decryption")
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let encrypted_value = self.encrypt(value)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, encrypted_value],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;

        if let Some(row) = rows.next()? {
            let encrypted_value: Vec<u8> = row.get(0)?;
            Ok(Some(self.decrypt(&encrypted_value)?))
        } else {
            Ok(None)
        }
    }

    pub fn add_bookmark(&self, url: &str, title: &str) -> Result<()> {
        let enc_url = self.encrypt(url)?;
        let enc_title = self.encrypt(title)?;
        self.conn.execute(
            "INSERT INTO bookmarks (url, title) VALUES (?1, ?2)",
            params![enc_url, enc_title],
        )?;
        Ok(())
    }

    pub fn get_bookmarks(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT url, title FROM bookmarks ORDER BY created_at DESC")?;
        let rows = stmt.query_map([], |row| {
            let u: Vec<u8> = row.get(0)?;
            let t: Vec<u8> = row.get(1)?;
            Ok((u, t))
        })?;

        let mut bookmarks = Vec::new();
        for res in rows {
            let (u, t) = res?;
            bookmarks.push((self.decrypt(&u)?, self.decrypt(&t)?));
        }
        Ok(bookmarks)
    }

    pub fn add_history(&self, url: &str, title: &str) -> Result<()> {
        let enc_url = self.encrypt(url)?;
        let enc_title = self.encrypt(title)?;
        self.conn.execute(
            "INSERT INTO history (url, title) VALUES (?1, ?2)",
            params![enc_url, enc_title],
        )?;
        Ok(())
    }

    pub fn get_history(&self) -> Result<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare("SELECT url, title, visited_at FROM history ORDER BY visited_at DESC LIMIT 50")?;
        let rows = stmt.query_map([], |row| {
            let u: Vec<u8> = row.get(0)?;
            let t: Vec<u8> = row.get(1)?;
            let time: String = row.get(2)?;
            Ok((u, t, time))
        })?;

        let mut history = Vec::new();
        for res in rows {
            let (u, t, time) = res?;
            history.push((self.decrypt(&u)?, self.decrypt(&t)?, time));
        }
        Ok(history)
    }

    pub fn clear_history(&self) -> Result<()> {
        self.conn.execute("DELETE FROM history", [])?;
        Ok(())
    }

    pub fn add_download(&self, url: &str, filename: &str, status: &str) -> Result<()> {
        let enc_url = self.encrypt(url)?;
        let enc_filename = self.encrypt(filename)?;
        self.conn.execute(
            "INSERT INTO downloads (url, filename, status) VALUES (?1, ?2, ?3)",
            params![enc_url, enc_filename, status],
        )?;
        Ok(())
    }

    pub fn get_downloads(&self) -> Result<Vec<(String, String, String, String)>> {
        let mut stmt = self.conn.prepare("SELECT url, filename, status, created_at FROM downloads ORDER BY created_at DESC")?;
        let rows = stmt.query_map([], |row| {
            let u: Vec<u8> = row.get(0)?;
            let f: Vec<u8> = row.get(1)?;
            let s: String = row.get(2)?;
            let time: String = row.get(3)?;
            Ok((u, f, s, time))
        })?;

        let mut downloads = Vec::new();
        for res in rows {
            let (u, f, s, time) = res?;
            downloads.push((self.decrypt(&u)?, self.decrypt(&f)?, s, time));
        }
        Ok(downloads)
    }

    pub fn save_tab_state(&self, tab_id: u32, state: &str) -> Result<()> {
        let enc_state = self.encrypt(state)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO tab_states (tab_id, state) VALUES (?1, ?2)",
            params![tab_id, enc_state],
        )?;
        Ok(())
    }

    pub fn get_tab_state(&self, tab_id: u32) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT state FROM tab_states WHERE tab_id = ?1")?;
        let mut rows = stmt.query(params![tab_id])?;

        if let Some(row) = rows.next()? {
            let enc_state: Vec<u8> = row.get(0)?;
            Ok(Some(self.decrypt(&enc_state)?))
        } else {
            Ok(None)
        }
    }

    pub fn add_shield_allowlist(&self, host: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO shield_allowlist (host) VALUES (?1)",
            params![host],
        )?;
        Ok(())
    }

    pub fn remove_shield_allowlist(&self, host: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM shield_allowlist WHERE host = ?1", params![host])?;
        Ok(())
    }

    pub fn is_shield_allowlisted(&self, host: &str) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare("SELECT 1 FROM shield_allowlist WHERE host = ?1 LIMIT 1")?;
        let mut rows = stmt.query(params![host])?;
        Ok(rows.next()?.is_some())
    }

    pub fn list_shield_allowlist(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT host FROM shield_allowlist ORDER BY host")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}
