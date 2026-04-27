use std::path::{Component, Path, PathBuf};

use axum::{
    Json, Router,
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::extractor::extractor::{EPUBS_DIR, HTML_DIR};
use crate::extractor::read_position::{ReadPosition, ReadPositionFileData};
use crate::server::auth::{self, BookOwnersFile};
use crate::server::endpoints;
use crate::server::middleware::AuthUser;

pub fn router() -> Router {
    Router::new()
        .route(endpoints::UPDATE_READ_POSITION, post(update_read_position))
        .route(endpoints::GET_READ_POSITION, get(get_read_position))
        .route(endpoints::TOGGLE_PRIVATE, post(toggle_private))
        .route(endpoints::CATEGORIES, get(list_categories))
}

pub async fn get_categories() -> Vec<String> {
    let mut categories = Vec::new();
    let Ok(mut entries) = fs::read_dir(EPUBS_DIR).await else {
        return categories;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        if !meta.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(String::from) else {
            continue;
        };
        if name.starts_with('.') || name == "private" {
            continue;
        }
        categories.push(name);
    }
    categories.sort();
    categories
}

async fn list_categories(AuthUser(_user): AuthUser) -> impl IntoResponse {
    Json(get_categories().await)
}

fn parse_section_path(url_path: &str) -> Option<(String, String)> {
    let decoded = percent_decode_str(url_path).decode_utf8().ok()?;
    let stripped = decoded.trim_start_matches('/');
    let rel = Path::new(stripped);
    if rel.components().any(|c| matches!(c, Component::ParentDir)) {
        return None;
    }
    let stem = rel.file_stem()?.to_str()?.to_string();
    if !stem.starts_with("section_") {
        return None;
    }
    let parent = rel.parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }
    Some((parent.to_string_lossy().to_string(), stem))
}

fn user_read_position_path(username: &str, book_path: &str) -> PathBuf {
    auth::user_data_dir(username)
        .join("read_positions")
        .join(format!(
            "{}.json",
            book_path.replace('/', std::path::MAIN_SEPARATOR_STR)
        ))
}

async fn load_user_positions(username: &str, book_path: &str) -> ReadPositionFileData {
    let path = user_read_position_path(username, book_path);
    match fs::read_to_string(&path).await {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => ReadPositionFileData::default(),
    }
}

async fn save_user_positions(
    username: &str,
    book_path: &str,
    data: &ReadPositionFileData,
) -> anyhow::Result<()> {
    let path = user_read_position_path(username, book_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_string_pretty(data)?;
    fs::write(path, json).await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct UpdateReadPositionRequest {
    pub path: String,
    pub node_path: Vec<usize>,
    pub offset: usize,
}

#[derive(Serialize)]
pub struct UpdateReadPositionResponse {
    pub success: bool,
}

async fn update_read_position(
    AuthUser(user): AuthUser,
    Json(payload): Json<UpdateReadPositionRequest>,
) -> impl IntoResponse {
    let Some((book_path, section_stem)) = parse_section_path(&payload.path) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(UpdateReadPositionResponse { success: false }),
        );
    };

    let mut data = load_user_positions(&user.username, &book_path).await;
    data.read_position.insert(
        section_stem.clone(),
        ReadPosition {
            file_name: section_stem,
            node_path: payload.node_path,
            offset: payload.offset,
        },
    );

    if let Err(e) = save_user_positions(&user.username, &book_path, &data).await {
        eprintln!("update_read_position: save error: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(UpdateReadPositionResponse { success: false }),
        );
    }

    (
        StatusCode::OK,
        Json(UpdateReadPositionResponse { success: true }),
    )
}

#[derive(Deserialize)]
pub struct GetReadPositionQuery {
    pub path: String,
}

#[derive(Serialize)]
pub struct GetReadPositionResponse {
    pub node_path: Vec<usize>,
    pub offset: usize,
}

async fn get_read_position(
    AuthUser(user): AuthUser,
    Query(params): Query<GetReadPositionQuery>,
) -> impl IntoResponse {
    let default_response = Json(GetReadPositionResponse {
        node_path: vec![],
        offset: 0,
    });

    let Some((book_path, section_stem)) = parse_section_path(&params.path) else {
        return (StatusCode::OK, default_response);
    };

    let data = load_user_positions(&user.username, &book_path).await;

    match data.read_position.get(&section_stem) {
        Some(pos) => (
            StatusCode::OK,
            Json(GetReadPositionResponse {
                node_path: pos.node_path.clone(),
                offset: pos.offset,
            }),
        ),
        None => (StatusCode::OK, default_response),
    }
}

// --- Toggle private: moves epub between category and private/<owner>/ ---

#[derive(Deserialize)]
pub struct TogglePrivateRequest {
    pub book_path: String,
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Serialize)]
pub struct TogglePrivateResponse {
    pub success: bool,
    pub is_private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_path: Option<String>,
}

async fn toggle_private(
    AuthUser(user): AuthUser,
    Json(payload): Json<TogglePrivateRequest>,
) -> impl IntoResponse {
    let book_path = payload.book_path.trim_matches('/').to_string();
    if book_path.contains("..") || book_path.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(TogglePrivateResponse {
                success: false,
                is_private: false,
                new_path: None,
            }),
        );
    }

    let owner = BookOwnersFile::get_owner(&book_path).await;
    let is_allowed = owner.as_deref() == Some(&user.username) || user.is_admin;
    if !is_allowed {
        return (
            StatusCode::FORBIDDEN,
            Json(TogglePrivateResponse {
                success: false,
                is_private: false,
                new_path: None,
            }),
        );
    }

    let actual_owner = owner.unwrap_or_else(|| user.username.clone());
    let currently_private = auth::private_path_owner(&book_path).is_some();

    let (new_book_path, new_is_private) = if currently_private {
        // Move to public: need a category
        let book_name = Path::new(&book_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let category = payload.category.unwrap_or_default();
        let category = category.trim().trim_matches('/');
        if category.is_empty() || category.contains("..") {
            return (
                StatusCode::BAD_REQUEST,
                Json(TogglePrivateResponse {
                    success: false,
                    is_private: true,
                    new_path: None,
                }),
            );
        }
        (format!("{category}/{book_name}"), false)
    } else {
        // Move to private
        let book_name = Path::new(&book_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        (format!("private/{actual_owner}/{book_name}"), true)
    };

    let old_epub = PathBuf::from(EPUBS_DIR).join(format!("{book_path}.epub"));
    let new_epub = PathBuf::from(EPUBS_DIR).join(format!("{new_book_path}.epub"));
    let old_html = PathBuf::from(HTML_DIR).join(&book_path);

    if let Some(parent) = new_epub.parent()
        && let Err(e) = fs::create_dir_all(parent).await
    {
        eprintln!("toggle_private: mkdir error: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(TogglePrivateResponse {
                success: false,
                is_private: currently_private,
                new_path: None,
            }),
        );
    }

    if let Err(e) = fs::rename(&old_epub, &new_epub).await {
        eprintln!("toggle_private: move error: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(TogglePrivateResponse {
                success: false,
                is_private: currently_private,
                new_path: None,
            }),
        );
    }

    // Delete old HTML so extractor re-converts at new location
    let _ = fs::remove_dir_all(&old_html).await;

    if let Err(e) = BookOwnersFile::update_path(&book_path, &new_book_path).await {
        eprintln!("toggle_private: update owners error: {e}");
    }

    crate::extractor::extractor::trigger_extract();

    (
        StatusCode::OK,
        Json(TogglePrivateResponse {
            success: true,
            is_private: new_is_private,
            new_path: Some(new_book_path),
        }),
    )
}
