use serde::{Deserialize, Serialize};
use tera::{Context, Tera};

use super::auth::User;

#[derive(Serialize)]
pub struct Breadcrumb {
    pub url: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct EntryInfo {
    pub is_dir: bool,
    pub is_book: bool,
    pub url: String,
    pub name: String,
    pub uploaded_by: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct BookIndex {
    pub book_name: String,
    pub sections: Vec<BookSection>,
}

#[derive(Serialize, Deserialize)]
pub struct BookSection {
    pub title: String,
    pub filename: String,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub username: String,
    pub is_admin: bool,
}

impl From<&User> for UserInfo {
    fn from(u: &User) -> Self {
        Self {
            username: u.username.clone(),
            is_admin: u.is_admin,
        }
    }
}

pub fn render_directory_view(
    current_path: &str,
    breadcrumbs: Vec<Breadcrumb>,
    parent_url: Option<String>,
    entries: Vec<EntryInfo>,
    user: &User,
    categories: &[String],
) -> anyhow::Result<String> {
    let template_src = include_str!("assets/directory_view.html");
    let mut tera = Tera::default();
    tera.add_raw_template("directory_view.html", template_src)?;

    let mut ctx = Context::new();
    ctx.insert("current_path", current_path);
    ctx.insert("breadcrumbs", &breadcrumbs);
    ctx.insert("parent_url", &parent_url);
    ctx.insert("entries", &entries);
    ctx.insert("user", &UserInfo::from(user));
    ctx.insert("categories", categories);

    Ok(tera.render("directory_view.html", &ctx)?)
}

pub fn render_book_view(
    book_index: &BookIndex,
    breadcrumbs: Vec<Breadcrumb>,
    user: &User,
    book_path: &str,
    is_private: bool,
    can_toggle: bool,
    uploaded_by: Option<&str>,
    categories: &[String],
) -> anyhow::Result<String> {
    let template_src = include_str!("assets/book_view.html");
    let mut tera = Tera::default();
    tera.add_raw_template("book_view.html", template_src)?;

    let mut ctx = Context::new();
    ctx.insert("book_name", &book_index.book_name);
    ctx.insert("breadcrumbs", &breadcrumbs);
    ctx.insert("sections", &book_index.sections);
    ctx.insert("user", &UserInfo::from(user));
    ctx.insert("book_path", book_path);
    ctx.insert("is_private", &is_private);
    ctx.insert("can_toggle", &can_toggle);
    ctx.insert("uploaded_by", &uploaded_by);
    ctx.insert("categories", categories);

    Ok(tera.render("book_view.html", &ctx)?)
}

pub fn render_section_view(
    book_name: &str,
    breadcrumbs: Vec<Breadcrumb>,
    iframe_src: &str,
    prev_url: Option<&str>,
    next_url: Option<&str>,
) -> anyhow::Result<String> {
    let template_src = include_str!("assets/section_view.html");
    let mut tera = Tera::default();
    tera.add_raw_template("section_view.html", template_src)?;

    let mut ctx = Context::new();
    ctx.insert("book_name", book_name);
    ctx.insert("breadcrumbs", &breadcrumbs);
    ctx.insert("iframe_src", iframe_src);
    ctx.insert("prev_url", &prev_url);
    ctx.insert("next_url", &next_url);

    Ok(tera.render("section_view.html", &ctx)?)
}

pub fn render_login(error: Option<&str>) -> anyhow::Result<String> {
    let template_src = include_str!("assets/login.html");
    let mut tera = Tera::default();
    tera.add_raw_template("login.html", template_src)?;

    let mut ctx = Context::new();
    ctx.insert("error", &error);

    Ok(tera.render("login.html", &ctx)?)
}

pub fn render_change_password(forced: bool, error: Option<&str>) -> anyhow::Result<String> {
    let template_src = include_str!("assets/change_password.html");
    let mut tera = Tera::default();
    tera.add_raw_template("change_password.html", template_src)?;

    let mut ctx = Context::new();
    ctx.insert("forced", &forced);
    ctx.insert("error", &error);

    Ok(tera.render("change_password.html", &ctx)?)
}

#[derive(Serialize)]
pub struct AdminUserEntry {
    pub username: String,
    pub is_admin: bool,
    pub must_change_password: bool,
}

pub fn render_admin(
    users: Vec<AdminUserEntry>,
    categories: &[String],
    error: Option<&str>,
    created_user: Option<&str>,
    created_otp: Option<&str>,
) -> anyhow::Result<String> {
    let template_src = include_str!("assets/admin.html");
    let mut tera = Tera::default();
    tera.add_raw_template("admin.html", template_src)?;

    let mut ctx = Context::new();
    ctx.insert("users", &users);
    ctx.insert("categories", categories);
    ctx.insert("error", &error);
    ctx.insert("created_user", &created_user);
    ctx.insert("created_otp", &created_otp);

    Ok(tera.render("admin.html", &ctx)?)
}

#[derive(Serialize)]
pub struct ProfileBookEntry {
    pub book_path: String,
    pub book_name: String,
    pub is_private: bool,
    pub url: String,
    pub category: String,
}

pub fn render_profile(
    user: &User,
    books: Vec<ProfileBookEntry>,
) -> anyhow::Result<String> {
    let template_src = include_str!("assets/profile.html");
    let mut tera = Tera::default();
    tera.add_raw_template("profile.html", template_src)?;

    let mut ctx = Context::new();
    ctx.insert("user", &UserInfo::from(user));
    ctx.insert("books", &books);

    Ok(tera.render("profile.html", &ctx)?)
}
