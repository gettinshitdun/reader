use serde::{Deserialize, Serialize};
use tera::{Context, Tera};

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

pub fn render_directory_view(
    current_path: &str,
    breadcrumbs: Vec<Breadcrumb>,
    parent_url: Option<String>,
    entries: Vec<EntryInfo>,
) -> anyhow::Result<String> {
    let template_src = include_str!("assets/directory_view.html");
    let mut tera = Tera::default();
    tera.add_raw_template("directory_view.html", template_src)?;

    let mut ctx = Context::new();
    ctx.insert("current_path", current_path);
    ctx.insert("breadcrumbs", &breadcrumbs);
    ctx.insert("parent_url", &parent_url);
    ctx.insert("entries", &entries);

    Ok(tera.render("directory_view.html", &ctx)?)
}

pub fn render_book_view(
    book_index: &BookIndex,
    breadcrumbs: Vec<Breadcrumb>,
) -> anyhow::Result<String> {
    let template_src = include_str!("assets/book_view.html");
    let mut tera = Tera::default();
    tera.add_raw_template("book_view.html", template_src)?;

    let mut ctx = Context::new();
    ctx.insert("book_name", &book_index.book_name);
    ctx.insert("breadcrumbs", &breadcrumbs);
    ctx.insert("sections", &book_index.sections);

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
