use argh::FromArgs;
use axum::Form;
use axum::Router;
use axum::extract::State;
use axum::http::response::Response;
use axum::http::status::StatusCode;
use axum::response::Redirect;
use axum::routing::get;
use axum::routing::post;
use axum::routing::put;
use axum::serve::ListenerExt;
use core::net::Ipv4Addr;
use duckdb::Connection;
use duckdb::params;
use http::init_tls_certs;
use serde::Deserialize;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::SystemTime;
use tokio::net::TcpListener;
use tokio::time::timeout;
use url::Url;
use uuid::Uuid;

use crate::html::HtmlWriter;
use crate::http::Document;
use crate::http::request_document;
use crate::rss::RssWriter;

mod html;
mod http;
mod rss;

type DbConnection = Arc<Mutex<Connection>>;

/// Paket: read before it goes away
#[derive(Clone, FromArgs)]
#[argh(help_triggers("-h", "--help"))]
struct Args {
    /// feed name
    #[argh(option, short = 'n', default = "String::from(\"My Paket\")")]
    name: String,

    /// feed description
    #[argh(option, short = 'd', default = "String::from(\"My links\")")]
    desc: String,

    /// feed HTTP url
    #[argh(option, short = 'l', from_str_fn(parse_http_url))]
    link: String,

    /// database file
    #[argh(option, default = "String::from(\"paket.duckdb\")")]
    db: String,

    /// server port
    #[argh(option, short = 'p', default = "8080")]
    port: u16,

    /// time to live in days
    #[argh(option, default = "60")]
    ttl: u32,
}

fn parse_http_url(url_str: &str) -> Result<String, String> {
    let url = Url::parse(url_str).map_err(|_| String::from("invalid link"))?;

    let scheme = url.scheme();
    if scheme != "https" && scheme != "http" {
        return Err(String::from("invalid link"));
    }

    Ok(url.into())
}

#[derive(Clone)]
struct App {
    args: Arc<Args>,
    db_connection: DbConnection,
}

fn main() -> anyhow::Result<()> {
    let args: Args = argh::from_env();
    let args = Arc::new(args);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()?;

    runtime.block_on(async move { serve(args).await })?;
    Ok(())
}

async fn serve(args: Arc<Args>) -> anyhow::Result<()> {
    init_tls_certs();

    let db_connection = Connection::open(&args.db)?;
    db_connection.execute(
        "CREATE TABLE IF NOT EXISTS articles (
            timestamp TIMESTAMP WITH TIME ZONE NOT NULL,
            title TEXT NOT NULL,
            link TEXT NOT NULL,
            guid TEXT NOT NULL)",
        [],
    )?;
    let db_connection = Arc::new(Mutex::new(db_connection));

    let port = args.port;
    let tcp_listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))
        .await?
        .tap_io(|tcp_stream| {
            let _ = tcp_stream.set_nodelay(true);
        });

    let router = Router::new()
        .route("/save", put(handle_save_article))
        .route("/delete", post(handle_delete_article))
        .route("/feed.xml", get(handle_get_feed_xml))
        .route("/feed.html", get(handle_get_feed_html))
        .with_state(App {
            args,
            db_connection,
        });

    println!("Serving at {port} ...");
    axum::serve(tcp_listener, router).await?;

    Ok(())
}

async fn handle_save_article(State(state): State<App>, Form(save): Form<SaveForm>) -> StatusCode {
    if let Err(err) = add_article(&save.url, state.db_connection).await {
        eprintln!("{err}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    StatusCode::OK
}

async fn handle_delete_article(
    State(state): State<App>,
    Form(delete): Form<DeleteForm>,
) -> Redirect {
    let mut db_lock = state.db_connection.lock().unwrap();

    if let Err(err) = delete_article(&mut db_lock, &delete.guid) {
        eprintln!("{err}");
    }

    Redirect::to("/feed.html")
}

async fn handle_get_feed_xml(State(state): State<App>) -> Response<String> {
    handle_get_feed::<RssWriter>(state).await
}

async fn handle_get_feed_html(State(state): State<App>) -> Response<String> {
    handle_get_feed::<HtmlWriter>(state).await
}

async fn handle_get_feed<T: FeedWriter>(state: App) -> Response<String> {
    let result = {
        let mut db_lock = state.db_connection.lock().unwrap();

        delete_old_articles(&mut db_lock, &state.args).and_then(|_| fetch_feed(&mut db_lock))
    };

    let items = match result {
        Ok(items) => items,
        Err(err) => {
            eprintln!("{err}");
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(String::new())
                .unwrap();
        }
    };

    let feed = build_feed::<T>(items.into_iter(), &state.args);

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", T::CONTENT_TYPE)
        .body(feed)
        .unwrap()
}

#[derive(Debug, Deserialize)]
struct DeleteForm {
    guid: String,
}

#[derive(Debug, Deserialize)]
struct SaveForm {
    url: String,
}

#[derive(Debug)]
struct Article {
    url: Url,
    title: String,
}

struct FeedItem {
    title: String,
    link: String,
    pub_date: String,
    guid: String,
}

async fn add_article(url: &str, db_connection: DbConnection) -> anyhow::Result<()> {
    let document = request_document(url).await?;
    let article = timeout(Duration::from_secs(10), extract_article(document)).await??;

    let mut db_lock = db_connection.lock().unwrap();
    store_article(&mut db_lock, article)?;

    Ok(())
}

async fn extract_article(document: Document) -> anyhow::Result<Article> {
    let (url, title) = match document {
        Document::Unsupported(url) => {
            let title = format!("[???] {url}");
            (url, title)
        }
        Document::Pdf(url) => {
            let title = url
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .map(ToString::to_string)
                .unwrap_or_else(|| url.to_string());
            let title = format!("[PDF] {title}");
            (url, title)
        }
        Document::Html(url, mut http_body_reader) => {
            let title = http_body_reader.extract_title().await?;
            (url, title)
        }
    };

    Ok(Article { url, title })
}

fn store_article(db_connection: &mut Connection, article: Article) -> anyhow::Result<()> {
    let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, article.url.as_str().as_bytes());
    let guid = uuid.to_string();

    db_connection.execute("DELETE FROM articles WHERE guid = ?", [&guid])?;

    db_connection.execute(
        "INSERT INTO articles 
        (title, link, guid, timestamp)
        VALUES
        (?, ?, ?, current_timestamp)",
        params![article.title, article.url.as_str(), &guid],
    )?;

    Ok(())
}

fn delete_article(db_connection: &mut Connection, guid: &str) -> anyhow::Result<()> {
    db_connection.execute("DELETE FROM articles WHERE guid = ?", [guid])?;
    Ok(())
}

fn delete_old_articles(db_connection: &mut Connection, args: &Args) -> anyhow::Result<()> {
    db_connection.execute(
        "DELETE FROM articles WHERE (current_timestamp AT TIME ZONE 'UTC' - timestamp AT TIME ZONE 'UTC') > INTERVAL (?) DAY",
        [args.ttl],
    )?;
    Ok(())
}

fn fetch_feed(db_connection: &mut Connection) -> anyhow::Result<Vec<FeedItem>> {
    let mut select_stmt = db_connection.prepare(
        "SELECT 
        title, link, guid, strftime(timestamp AT TIME ZONE 'GMT', '%a, %d %b %Y %X GMT') 
        FROM articles
        ORDER BY timestamp DESC",
    )?;

    let mut rows = select_stmt.query([])?;
    let count = rows.as_ref().unwrap().row_count();

    let mut items = Vec::with_capacity(count);
    while let Some(row) = rows.next()? {
        let item = FeedItem {
            title: row.get(0)?,
            link: row.get(1)?,
            guid: row.get(2)?,
            pub_date: row.get(3)?,
        };
        items.push(item);
    }

    Ok(items)
}

fn build_feed<T: FeedWriter>(items: impl Iterator<Item = FeedItem>, args: &Args) -> String {
    let mut writer = T::new(
        &args.name,
        &args.desc,
        args.link.as_str(),
        SystemTime::now(),
    );
    writer.write_items(items);
    writer.finish()
}

trait FeedWriter {
    const CONTENT_TYPE: &str;

    fn new(title: &str, description: &str, link: &str, time: SystemTime) -> Self;

    fn write_items(&mut self, items: impl Iterator<Item = FeedItem>);

    fn finish(self) -> String;
}
