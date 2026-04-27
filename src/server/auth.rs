use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs;

use crate::extractor::extractor::DATA_DIR;

const SESSION_TOKEN_BYTES: usize = 32;
const OTP_LENGTH: usize = 16;

fn users_json_path() -> PathBuf {
    PathBuf::from(DATA_DIR).join("users.json")
}

fn sessions_json_path() -> PathBuf {
    PathBuf::from(DATA_DIR).join("sessions.json")
}

pub fn user_data_dir(username: &str) -> PathBuf {
    PathBuf::from(DATA_DIR).join("users").join(username)
}

pub fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn generate_otp() -> String {
    let mut rng = rand::rng();
    let chars: Vec<char> = "abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789"
        .chars()
        .collect();
    (0..OTP_LENGTH)
        .map(|_| chars[rng.random_range(0..chars.len())])
        .collect()
}

fn generate_session_token() -> String {
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..SESSION_TOKEN_BYTES).map(|_| rng.random()).collect();
    hex::encode(bytes)
}

// --- User types ---

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct User {
    pub username: String,
    pub password_hash: String,
    pub is_admin: bool,
    pub must_change_password: bool,
}

#[derive(Deserialize, Serialize, Default)]
pub struct UsersFile {
    pub users: HashMap<String, User>,
}

impl UsersFile {
    pub async fn load() -> Result<Self> {
        let path = users_json_path();
        match fs::read_to_string(&path).await {
            Ok(data) => Ok(serde_json::from_str(&data)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn save(&self) -> Result<()> {
        let path = users_json_path();
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json).await?;
        Ok(())
    }
}

// --- Session types ---

#[derive(Deserialize, Serialize, Clone)]
pub struct Session {
    pub username: String,
    pub created_at: u64,
}

#[derive(Deserialize, Serialize, Default)]
pub struct SessionsFile {
    pub sessions: HashMap<String, Session>,
}

impl SessionsFile {
    pub async fn load() -> Result<Self> {
        let path = sessions_json_path();
        match fs::read_to_string(&path).await {
            Ok(data) => Ok(serde_json::from_str(&data)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn save(&self) -> Result<()> {
        let path = sessions_json_path();
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json).await?;
        Ok(())
    }
}

// --- Book ownership ---

fn book_owners_path() -> PathBuf {
    PathBuf::from(DATA_DIR).join("book_owners.json")
}

#[derive(Deserialize, Serialize, Default)]
pub struct BookOwnersFile {
    pub books: HashMap<String, String>,
}

impl BookOwnersFile {
    pub async fn load() -> Result<Self> {
        let path = book_owners_path();
        match fs::read_to_string(&path).await {
            Ok(data) => Ok(serde_json::from_str(&data)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn save(&self) -> Result<()> {
        let path = book_owners_path();
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json).await?;
        Ok(())
    }

    pub async fn set_owner(book_path: &str, username: &str) -> Result<()> {
        let mut owners = Self::load().await?;
        owners.books.insert(book_path.to_string(), username.to_string());
        owners.save().await
    }

    pub async fn get_owner(book_path: &str) -> Option<String> {
        let owners = Self::load().await.ok()?;
        owners.books.get(book_path).cloned()
    }

    pub async fn books_by_user(username: &str) -> Vec<String> {
        let owners = Self::load().await.unwrap_or_default();
        owners
            .books
            .iter()
            .filter(|(_, owner)| *owner == username)
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub async fn update_path(old_path: &str, new_path: &str) -> Result<()> {
        let mut owners = Self::load().await?;
        if let Some(owner) = owners.books.remove(old_path) {
            owners.books.insert(new_path.to_string(), owner);
        }
        owners.save().await
    }
}

// --- Operations ---

pub async fn create_session(username: &str) -> Result<String> {
    let token = generate_session_token();
    let mut sessions = SessionsFile::load().await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    sessions.sessions.insert(
        token.clone(),
        Session {
            username: username.to_string(),
            created_at: now,
        },
    );
    sessions.save().await?;
    Ok(token)
}

pub async fn validate_session(token: &str) -> Option<User> {
    let sessions = SessionsFile::load().await.ok()?;
    let session = sessions.sessions.get(token)?;
    let users = UsersFile::load().await.ok()?;
    users.users.get(&session.username).cloned()
}

pub async fn delete_session(token: &str) -> Result<()> {
    let mut sessions = SessionsFile::load().await?;
    sessions.sessions.remove(token);
    sessions.save().await?;
    Ok(())
}

pub async fn authenticate(username: &str, password: &str) -> Option<User> {
    let users = UsersFile::load().await.ok()?;
    let user = users.users.get(username)?;
    if user.password_hash == hash_password(password) {
        Some(user.clone())
    } else {
        None
    }
}

pub async fn change_password(username: &str, new_password: &str) -> Result<()> {
    let mut users = UsersFile::load().await?;
    let user = users
        .users
        .get_mut(username)
        .ok_or_else(|| anyhow::anyhow!("User not found"))?;
    user.password_hash = hash_password(new_password);
    user.must_change_password = false;
    users.save().await?;
    Ok(())
}

/// Create a new user with a generated OTP. Returns the plaintext OTP.
pub async fn create_user(username: &str, is_admin: bool) -> Result<String> {
    let mut users = UsersFile::load().await?;
    if users.users.contains_key(username) {
        anyhow::bail!("User '{}' already exists", username);
    }
    let otp = generate_otp();
    let user = User {
        username: username.to_string(),
        password_hash: hash_password(&otp),
        is_admin,
        must_change_password: true,
    };
    users.users.insert(username.to_string(), user);
    users.save().await?;
    fs::create_dir_all(user_data_dir(username)).await?;
    Ok(otp)
}

/// Bootstrap: create the initial admin if users.json doesn't exist or is empty.
/// Returns Some(otp) if created, None if admin already exists.
pub async fn bootstrap_admin() -> Result<Option<String>> {
    let users = UsersFile::load().await?;
    if !users.users.is_empty() {
        return Ok(None);
    }
    let otp = create_user("admin", true).await?;
    Ok(Some(otp))
}

/// Check if a relative path is inside the private area and extract the owner username.
/// e.g. "private/alice/My Book" -> Some("alice")
pub fn private_path_owner(rel_path: &str) -> Option<String> {
    let clean = rel_path.trim_matches('/');
    let mut parts = clean.splitn(3, '/');
    let first = parts.next()?;
    if first != "private" {
        return None;
    }
    let username = parts.next()?;
    if username.is_empty() {
        return None;
    }
    Some(username.to_string())
}

/// Walk EPUBS_DIR and assign ownership to "admin" for any book not already tracked.
pub async fn backfill_owners() -> Result<()> {
    use walkdir::WalkDir;
    use crate::extractor::extractor::EPUBS_DIR;

    let epubs_root = std::path::Path::new(EPUBS_DIR);
    if !epubs_root.exists() {
        return Ok(());
    }

    let mut owners = BookOwnersFile::load().await?;
    let mut changed = false;

    for entry in WalkDir::new(epubs_root) {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "epub") {
            let rel = path.strip_prefix(EPUBS_DIR)?;
            let book_key = rel.with_extension("").to_string_lossy().to_string();
            if let std::collections::hash_map::Entry::Vacant(e) = owners.books.entry(book_key.clone()) {
                let owner = private_path_owner(&book_key).unwrap_or_else(|| "admin".to_string());
                e.insert(owner);
                changed = true;
            }
        }
    }

    if changed {
        owners.save().await?;
    }
    Ok(())
}
