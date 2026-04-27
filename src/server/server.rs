use axum::{
    Form, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use std::collections::HashMap;
use std::path::{Component, PathBuf};
use tokio::fs;

use crate::extractor::extractor::{EPUBS_DIR, HTML_DIR};
use crate::server::api_server;
use crate::server::auth::{self, BookOwnersFile, User, UsersFile};
use crate::server::middleware::{AuthUser, MaybeUser, SESSION_COOKIE_NAME};
use crate::server::previous_path::PreviousPage;
use crate::server::templates::{
    self, AdminUserEntry, BookIndex, Breadcrumb, EntryInfo, ProfileBookEntry,
};

pub fn router() -> Router {
    Router::new()
        .nest("/api", api_server::router())
        .route("/static/{*path}", get(serve_static))
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", post(logout))
        .route(
            "/change-password",
            get(change_password_page).post(change_password_submit),
        )
        .route("/admin", get(admin_page).post(admin_create_user))
        .route("/admin/category", post(admin_create_category))
        .route("/upload", post(handle_upload).layer(DefaultBodyLimit::max(100 * 1024 * 1024)))
        .route("/profile", get(profile_page))
        .route("/previous", get(serve_previous))
        .route("/", get(serve_path_root))
        .route("/{*path}", get(serve_path))
}

// --- Auth pages ---

async fn login_page(MaybeUser(user): MaybeUser) -> Response {
    if user.is_some() {
        return Redirect::to("/").into_response();
    }
    match templates::render_login(None) {
        Ok(html) => Html(html).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(serde::Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

async fn login_submit(Form(form): Form<LoginForm>) -> Response {
    let Some(_user) = auth::authenticate(&form.username, &form.password).await else {
        let html =
            templates::render_login(Some("Invalid username or password")).unwrap_or_default();
        return Html(html).into_response();
    };

    let Ok(token) = auth::create_session(&form.username).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let cookie = format!("{SESSION_COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax");
    (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to("/"),
    )
        .into_response()
}

async fn logout(AuthUser(_user): AuthUser, headers: axum::http::HeaderMap) -> Response {
    if let Some(token) = extract_token_from_headers(&headers) {
        let _ = auth::delete_session(&token).await;
    }
    let cookie = format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to("/login"),
    )
        .into_response()
}

fn extract_token_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_header = headers.get("cookie")?.to_str().ok()?;
    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(SESSION_COOKIE_NAME) {
            let value = value.strip_prefix('=')?;
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

async fn change_password_page(AuthUser(user): AuthUser) -> Response {
    match templates::render_change_password(user.must_change_password, None) {
        Ok(html) => Html(html).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(serde::Deserialize)]
struct ChangePasswordForm {
    new_password: String,
    confirm_password: String,
}

async fn change_password_submit(
    AuthUser(user): AuthUser,
    Form(form): Form<ChangePasswordForm>,
) -> Response {
    if form.new_password != form.confirm_password {
        let html = templates::render_change_password(
            user.must_change_password,
            Some("Passwords do not match"),
        )
        .unwrap_or_default();
        return Html(html).into_response();
    }
    if form.new_password.len() < 4 {
        let html = templates::render_change_password(
            user.must_change_password,
            Some("Password must be at least 4 characters"),
        )
        .unwrap_or_default();
        return Html(html).into_response();
    }
    if let Err(e) = auth::change_password(&user.username, &form.new_password).await {
        eprintln!("change_password error: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Redirect::to("/").into_response()
}

// --- Admin ---

async fn admin_page(AuthUser(user): AuthUser) -> Response {
    if !user.is_admin {
        return StatusCode::FORBIDDEN.into_response();
    }
    render_admin_page(None, None, None).await
}

async fn render_admin_page(
    error: Option<&str>,
    created_user: Option<&str>,
    created_otp: Option<&str>,
) -> Response {
    let users_file = UsersFile::load().await.unwrap_or_default();
    let mut user_list: Vec<AdminUserEntry> = users_file
        .users
        .values()
        .map(|u| AdminUserEntry {
            username: u.username.clone(),
            is_admin: u.is_admin,
            must_change_password: u.must_change_password,
        })
        .collect();
    user_list.sort_by(|a, b| a.username.cmp(&b.username));

    let categories = api_server::get_categories().await;

    match templates::render_admin(user_list, &categories, error, created_user, created_otp) {
        Ok(html) => Html(html).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(serde::Deserialize)]
struct AdminCreateForm {
    username: String,
    #[serde(default)]
    is_admin: Option<String>,
}

async fn admin_create_user(
    AuthUser(user): AuthUser,
    Form(form): Form<AdminCreateForm>,
) -> Response {
    if !user.is_admin {
        return StatusCode::FORBIDDEN.into_response();
    }

    let username = form.username.trim().to_lowercase();
    if username.is_empty()
        || username.contains(std::path::MAIN_SEPARATOR)
        || username.contains("..")
    {
        return render_admin_page(Some("Invalid username"), None, None).await;
    }

    let is_admin = form.is_admin.as_deref() == Some("true");
    match auth::create_user(&username, is_admin).await {
        Ok(otp) => render_admin_page(None, Some(&username), Some(&otp)).await,
        Err(e) => render_admin_page(Some(&e.to_string()), None, None).await,
    }
}

// --- Admin: Create Category ---

#[derive(serde::Deserialize)]
struct AdminCategoryForm {
    category: String,
}

async fn admin_create_category(
    AuthUser(user): AuthUser,
    Form(form): Form<AdminCategoryForm>,
) -> Response {
    if !user.is_admin {
        return StatusCode::FORBIDDEN.into_response();
    }

    let name = form.category.trim().to_string();
    if name.is_empty() || name.contains('/') || name.contains("..") || name.starts_with('.') {
        return render_admin_page(Some("Invalid category name"), None, None).await;
    }

    let dir = PathBuf::from(EPUBS_DIR).join(&name);
    if dir.exists() {
        return render_admin_page(Some("Category already exists"), None, None).await;
    }

    if let Err(e) = fs::create_dir_all(&dir).await {
        eprintln!("create_category: mkdir error: {e}");
        return render_admin_page(Some("Failed to create category"), None, None).await;
    }

    Redirect::to("/admin").into_response()
}

// --- Upload ---

async fn handle_upload(AuthUser(user): AuthUser, mut multipart: Multipart) -> Response {
    let mut file_data: Option<(String, Vec<u8>)> = None;
    let mut category = String::new();
    let mut is_private = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let filename = field
                    .file_name()
                    .unwrap_or("upload.epub")
                    .to_string();
                if let Ok(bytes) = field.bytes().await {
                    file_data = Some((filename, bytes.to_vec()));
                }
            }
            "category" => {
                if let Ok(text) = field.text().await {
                    category = text;
                }
            }
            "private" => {
                if let Ok(text) = field.text().await {
                    is_private = text == "true";
                }
            }
            _ => {}
        }
    }

    let Some((filename, bytes)) = file_data else {
        return (StatusCode::BAD_REQUEST, "No file uploaded").into_response();
    };

    if !filename.ends_with(".epub") {
        return (StatusCode::BAD_REQUEST, "Only .epub files allowed").into_response();
    }

    let category = category.trim().trim_matches('/').to_string();
    if category.contains("..") || category.contains('/') {
        return (StatusCode::BAD_REQUEST, "Invalid category").into_response();
    }

    let book_stem = filename.strip_suffix(".epub").unwrap_or(&filename);

    let (dest_path, book_key) = if is_private {
        let dir = PathBuf::from(EPUBS_DIR)
            .join("private")
            .join(&user.username);
        (dir.join(&filename), format!("private/{}/{book_stem}", user.username))
    } else {
        if category.is_empty() {
            return (StatusCode::BAD_REQUEST, "Category required for public uploads")
                .into_response();
        }
        let valid_categories = api_server::get_categories().await;
        if !valid_categories.contains(&category) {
            return (StatusCode::BAD_REQUEST, "Category does not exist. Ask an admin to create it.")
                .into_response();
        }
        let dir = PathBuf::from(EPUBS_DIR).join(&category);
        (dir.join(&filename), format!("{category}/{book_stem}"))
    };

    if let Some(parent) = dest_path.parent()
        && let Err(e) = fs::create_dir_all(parent).await
    {
        eprintln!("upload: mkdir error: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if let Err(e) = fs::write(&dest_path, &bytes).await {
        eprintln!("upload: write error: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if let Err(e) = BookOwnersFile::set_owner(&book_key, &user.username).await {
        eprintln!("upload: set_owner error: {e}");
    }

    crate::extractor::extractor::trigger_extract();

    let redirect_to = if is_private {
        "/profile?uploaded=true".to_string()
    } else {
        format!("/{category}/?uploaded=true")
    };
    Redirect::to(&redirect_to).into_response()
}

// --- Profile ---

async fn profile_page(AuthUser(user): AuthUser) -> Response {
    let user_books = BookOwnersFile::books_by_user(&user.username).await;
    let mut entries: Vec<ProfileBookEntry> = user_books
        .into_iter()
        .map(|book_path| {
            let is_private = auth::private_path_owner(&book_path).is_some();
            let book_name = std::path::Path::new(&book_path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let category = if is_private {
                "private".to_string()
            } else {
                std::path::Path::new(&book_path)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("")
                    .to_string()
            };
            let url = format!("{book_path}/");
            ProfileBookEntry {
                book_path,
                book_name,
                is_private,
                url,
                category,
            }
        })
        .collect();
    entries.sort_by(|a, b| a.book_name.cmp(&b.book_name));

    match templates::render_profile(&user, entries) {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            eprintln!("Profile render error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// --- Content serving ---

async fn serve_previous(AuthUser(user): AuthUser) -> Redirect {
    match PreviousPage::get(&user.username).await {
        Some(previous) => Redirect::to(&previous),
        None => Redirect::to("/"),
    }
}

async fn serve_static(Path(path): Path<String>) -> Response {
    match path.as_str() {
        "common.css" => {
            let css = include_str!("assets/common.css");
            (
                [(axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8")],
                css,
            )
                .into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn serve_path_root(user: MaybeUser) -> Response {
    let Some(user) = require_auth(user) else {
        return Redirect::to("/login").into_response();
    };
    if user.must_change_password {
        return Redirect::to("/change-password").into_response();
    }
    serve_path_impl("", false, &user).await
}

async fn serve_path(
    user: MaybeUser,
    Path(path): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let Some(user) = require_auth(user) else {
        return Redirect::to("/login").into_response();
    };
    if user.must_change_password {
        return Redirect::to("/change-password").into_response();
    }
    let raw = params.contains_key("raw");
    serve_path_impl(&path, raw, &user).await
}

fn require_auth(maybe: MaybeUser) -> Option<User> {
    maybe.0
}

async fn serve_path_impl(rel_path: &str, raw: bool, user: &User) -> Response {
    let rel = std::path::Path::new(rel_path);
    if rel.components().any(|c| matches!(c, Component::ParentDir)) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let full_path = PathBuf::from(HTML_DIR).join(rel_path);

    if let Some(deny) = check_private_access(rel_path, user) {
        return deny;
    }

    let index_json_path = full_path.join("index.json");
    if let Ok(bytes) = fs::read(&index_json_path).await {
        return render_book_index(&bytes, rel_path, user).await;
    }
    if let Ok(entries) = fs::read_dir(&full_path).await {
        return render_directory(entries, rel_path, &full_path, user).await;
    }
    match fs::read(&full_path).await {
        Ok(bytes) => {
            if !raw
                && let Some(resp) = try_render_section_view(rel_path, &full_path).await
            {
                let _ = PreviousPage::set(&user.username, rel_path).await;
                return resp;
            }
            serve_file(&full_path, bytes)
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Path-based private access check. If the path is under private/<username>/,
/// only that user and admins can access it.
fn check_private_access(rel_path: &str, user: &User) -> Option<Response> {
    let owner = auth::private_path_owner(rel_path)?;
    if user.username == owner || user.is_admin {
        None
    } else {
        Some(StatusCode::FORBIDDEN.into_response())
    }
}

async fn render_book_index(json_bytes: &[u8], rel_path: &str, user: &User) -> Response {
    let book_index: BookIndex = match serde_json::from_slice(json_bytes) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Failed to parse index.json: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let breadcrumbs = build_breadcrumbs(rel_path);
    let book_path = rel_path.trim_matches('/');
    let is_private = auth::private_path_owner(book_path).is_some();
    let uploaded_by = BookOwnersFile::get_owner(book_path).await;
    let can_toggle =
        uploaded_by.as_deref() == Some(&user.username) || user.is_admin;
    let categories = api_server::get_categories().await;

    match templates::render_book_view(
        &book_index,
        breadcrumbs,
        user,
        book_path,
        is_private,
        can_toggle,
        uploaded_by.as_deref(),
        &categories,
    ) {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            eprintln!("Template render error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn try_render_section_view(rel_path: &str, full_path: &std::path::Path) -> Option<Response> {
    let file_name = full_path.file_name()?.to_str()?;
    if !file_name.starts_with("section_") || !file_name.ends_with(".html") {
        return None;
    }

    let parent = full_path.parent()?;
    let index_json_path = parent.join("index.json");
    let json_bytes = fs::read(&index_json_path).await.ok()?;
    let book_index: BookIndex = serde_json::from_slice(&json_bytes).ok()?;

    let current_idx = book_index
        .sections
        .iter()
        .position(|s| s.filename == file_name)?;

    let prev_url = if current_idx > 0 {
        Some(book_index.sections[current_idx - 1].filename.clone())
    } else {
        None
    };
    let next_url = if current_idx + 1 < book_index.sections.len() {
        Some(book_index.sections[current_idx + 1].filename.clone())
    } else {
        None
    };

    let parent_rel = std::path::Path::new(rel_path)
        .parent()
        .unwrap_or(std::path::Path::new(""));
    let breadcrumbs = build_breadcrumbs(parent_rel.to_str().unwrap_or(""));

    let iframe_src = format!("{file_name}?raw=true");

    match templates::render_section_view(
        &book_index.book_name,
        breadcrumbs,
        &iframe_src,
        prev_url.as_deref(),
        next_url.as_deref(),
    ) {
        Ok(html) => Some(Html(html).into_response()),
        Err(e) => {
            eprintln!("Section view render error: {e}");
            None
        }
    }
}

async fn render_directory(
    mut entries: fs::ReadDir,
    rel_path: &str,
    full_path: &std::path::Path,
    user: &User,
) -> Response {
    let mut items: Vec<(String, bool)> = Vec::new();

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(String::from) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        let is_dir = meta.is_dir();
        items.push((name, is_dir));
    }

    let owners = BookOwnersFile::load().await.unwrap_or_default();

    items.retain(|(name, is_dir)| {
        if !is_dir {
            return true;
        }
        let child_path = if rel_path.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", rel_path.trim_matches('/'), name)
        };

        // Hide root-level "private" folder from non-admins
        if rel_path.is_empty() && name == "private" && !user.is_admin {
            return false;
        }

        // Inside private/, only show the user's own folder (admins see all)
        if let Some(owner) = auth::private_path_owner(&child_path) {
            return owner == user.username || user.is_admin;
        }

        true
    });

    items.sort_by(|a, b| match (a.1, b.1) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.0.cmp(&b.0),
    });

    let breadcrumbs = build_breadcrumbs(rel_path);

    let current_path = if rel_path.is_empty() {
        "/".to_string()
    } else {
        format!("/{}/", rel_path)
    };

    let parent_url = if rel_path.is_empty() {
        None
    } else {
        Some("../".to_string())
    };

    let entry_infos: Vec<EntryInfo> = items
        .into_iter()
        .map(|(name, is_dir)| {
            let is_book = is_dir && full_path.join(&name).join("index.json").exists();
            let url = if is_dir {
                format!("{name}/")
            } else {
                name.clone()
            };

            let uploaded_by = if is_book {
                let book_key = if rel_path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", rel_path.trim_matches('/'), name)
                };
                owners.books.get(&book_key).cloned()
            } else {
                None
            };

            EntryInfo {
                url,
                is_dir,
                is_book,
                name,
                uploaded_by,
            }
        })
        .collect();

    let categories = api_server::get_categories().await;

    match templates::render_directory_view(&current_path, breadcrumbs, parent_url, entry_infos, user, &categories)
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            eprintln!("Template render error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn build_breadcrumbs(rel_path: &str) -> Vec<Breadcrumb> {
    let mut breadcrumbs = vec![Breadcrumb {
        url: "/".to_string(),
        name: "root".to_string(),
    }];
    if !rel_path.is_empty() {
        let mut accumulated = String::new();
        for segment in rel_path.split('/') {
            if segment.is_empty() {
                continue;
            }
            accumulated.push('/');
            accumulated.push_str(segment);
            accumulated.push('/');
            breadcrumbs.push(Breadcrumb {
                url: accumulated.clone(),
                name: segment.to_string(),
            });
        }
    }
    breadcrumbs
}

fn serve_file(file_path: &std::path::Path, bytes: Vec<u8>) -> Response {
    let content_type = guess_content_type(file_path);
    ([(axum::http::header::CONTENT_TYPE, content_type)], bytes).into_response()
}

fn guess_content_type(path: &std::path::Path) -> &'static str {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| match ext {
            "html" | "htm" => Some("text/html; charset=utf-8"),
            "css" => Some("text/css; charset=utf-8"),
            "js" => Some("application/javascript"),
            "json" => Some("application/json"),
            "png" => Some("image/png"),
            "jpg" | "jpeg" => Some("image/jpeg"),
            "gif" => Some("image/gif"),
            "svg" => Some("image/svg+xml"),
            "txt" => Some("text/plain; charset=utf-8"),
            "pdf" => Some("application/pdf"),
            "woff" => Some("font/woff"),
            "woff2" => Some("font/woff2"),
            _ => None,
        })
        .unwrap_or("application/octet-stream")
}
