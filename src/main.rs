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
use duckdb::Transaction;
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
use crate::http::PlainOrTls;
use crate::http::request_document;
use crate::rss::RssWriter;

use log::error;
use log::info;

mod html;
mod http;
mod rss;

type DbConnection = Arc<Mutex<Connection>>;

/// Paket: read before it goes away
#[derive(Debug, Clone, FromArgs)]
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

    env_logger::init();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?;

    runtime.block_on(async move { serve(args).await })?;
    Ok(())
}

async fn serve(args: Arc<Args>) -> anyhow::Result<()> {
    init_tls_certs();

    let mut db_connection = Connection::open(&args.db)?;

    setup_tables(&mut db_connection)?;
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
            args: args.clone(),
            db_connection,
        });

    info!("Serving {args:?}");
    axum::serve(tcp_listener, router).await?;

    Ok(())
}

async fn handle_save_article(State(state): State<App>, Form(save): Form<SaveForm>) -> StatusCode {
    info!("save_article: {save:?}");

    if let Err(err) = add_article(&save.url, state.db_connection).await {
        error!("{err}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    StatusCode::OK
}

async fn handle_delete_article(
    State(state): State<App>,
    Form(delete): Form<DeleteForm>,
) -> Redirect {
    info!("delete_article: {delete:?}");

    let mut db_lock = state.db_connection.lock().unwrap();

    if let Err(err) = delete_article(&mut db_lock, &delete.guid) {
        error!("{err}");
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
    info!("get_feed");

    let result = {
        let mut db_lock = state.db_connection.lock().unwrap();

        delete_old_articles(&mut db_lock, &state.args)
            .and_then(|_| fetch_feed(&mut db_lock))
            .and_then(|feed_items| {
                fetch_weekly_stats(&mut db_lock).map(|weekly_items| (feed_items, weekly_items))
            })
    };

    let (feed_items, weekly_items) = match result {
        Ok(items) => items,
        Err(err) => {
            error!("{err}");
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(String::new())
                .unwrap();
        }
    };

    let feed = build_feed::<T>(feed_items, weekly_items, &state.args);

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

struct WeeklyItem {
    articles_count: i64,
}

async fn add_article(url: &str, db_connection: DbConnection) -> anyhow::Result<()> {
    let fetch_and_extract = async {
        let document = request_document(url).await?;
        extract_article(document).await
    };

    let article = timeout(Duration::from_secs(5), fetch_and_extract).await??;

    let mut db_lock = db_connection.lock().unwrap();
    store_article(&mut db_lock, article)?;

    Ok(())
}

async fn extract_article(document: Document<PlainOrTls>) -> anyhow::Result<Article> {
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
            let title = title.unwrap_or_else(|| "[NO TITLE]".to_string());
            (url, title)
        }
    };

    Ok(Article { url, title })
}

fn setup_tables(db_connection: &mut Connection) -> anyhow::Result<()> {
    db_connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS articles (
            timestamp TIMESTAMP WITH TIME ZONE NOT NULL,
            title TEXT NOT NULL,
            link TEXT NOT NULL,
            guid TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS stats_per_week_of_year (
            week_of_year INT64 NOT NULL PRIMARY KEY,
            articles_count INT64 NOT NULL);",
    )?;

    Ok(())
}

fn store_article(db_connection: &mut Connection, article: Article) -> anyhow::Result<()> {
    let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, article.url.as_str().as_bytes());
    let guid = uuid.to_string();

    let tx = db_connection.transaction()?;
    tx.execute("DELETE FROM articles WHERE guid = ?", [&guid])?;
    tx.execute(
        "INSERT INTO articles 
        (title, link, guid, timestamp)
        VALUES
        (?, ?, ?, current_timestamp)",
        params![article.title, article.url.as_str(), &guid],
    )?;
    update_weekly_stats(&tx)?;
    tx.commit()?;

    Ok(())
}

fn delete_article(db_connection: &mut Connection, guid: &str) -> anyhow::Result<()> {
    let tx = db_connection.transaction()?;
    tx.execute("DELETE FROM articles WHERE guid = ?", [guid])?;
    update_weekly_stats(&tx)?;
    tx.commit()?;
    Ok(())
}

fn delete_old_articles(db_connection: &mut Connection, args: &Args) -> anyhow::Result<()> {
    let tx = db_connection.transaction()?;
    tx.execute(
        "DELETE FROM articles WHERE (current_timestamp AT TIME ZONE 'UTC' - timestamp AT TIME ZONE 'UTC') > INTERVAL (?) DAY",
        [args.ttl],
    )?;
    update_weekly_stats(&tx)?;
    tx.commit()?;
    Ok(())
}

fn update_weekly_stats(tx: &Transaction) -> anyhow::Result<()> {
    tx.execute_batch(
        "
        INSERT OR REPLACE INTO stats_per_week_of_year (week_of_year, articles_count) 
        VALUES (weekofyear(current_timestamp), (SELECT count(*) FROM articles));

        DELETE FROM stats_per_week_of_year WHERE week_of_year > weekofyear(current_timestamp)",
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

fn fetch_weekly_stats(db_connection: &mut Connection) -> anyhow::Result<Vec<WeeklyItem>> {
    let mut select_stmt = db_connection.prepare(
        "SELECT 
        articles_count
        FROM stats_per_week_of_year
        ORDER BY week_of_year ASC",
    )?;

    let mut rows = select_stmt.query([])?;
    let count = rows.as_ref().unwrap().row_count();

    let mut items = Vec::with_capacity(count);
    while let Some(row) = rows.next()? {
        let item = WeeklyItem {
            articles_count: i64::max(0, row.get(0)?),
        };
        items.push(item);
    }

    Ok(items)
}

// TODO the rss writer doesn't write weekly items. so api is dubious. type state writer?
fn build_feed<T: FeedWriter>(
    feed_items: Vec<FeedItem>,
    weekly_items: Vec<WeeklyItem>,
    args: &Args,
) -> String {
    let mut writer = T::new(
        &args.name,
        &args.desc,
        args.link.as_str(),
        SystemTime::now(),
    );
    writer.write_weekly_items(weekly_items);
    writer.write_feed_items(feed_items);
    writer.finish()
}

trait FeedWriter {
    const CONTENT_TYPE: &str;

    fn new(title: &str, description: &str, link: &str, time: SystemTime) -> Self;

    fn write_weekly_items(&mut self, items: Vec<WeeklyItem>);
    fn write_feed_items(&mut self, items: Vec<FeedItem>);

    fn finish(self) -> String;
}
