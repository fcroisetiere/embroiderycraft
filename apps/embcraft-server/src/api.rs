//! The HTTP API (`/api/v1/…`, JSON) and the web page that uses it. See docs/SERVER_API.md.

use crate::auth::{self, User};
use crate::backup;
use crate::card;
use crate::colourways;
use crate::connections::{self, Remote};
use crate::costing;
use crate::customers::{self, Placement};
use crate::db::{self, Search, ThreadColour};
use crate::jobs;
use crate::library::{self, Incoming, Store};
use crate::machines;
use crate::stick;
use crate::threads;
use axum::extract::{DefaultBodyLimit, FromRequestParts, Multipart, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header, request::Parts};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

pub struct App {
    pub db: Mutex<Connection>,
    pub store: Store,
}

type St = State<Arc<App>>;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

type R<T> = Result<T, ApiError>;

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
    fn bad(m: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, m)
    }
    fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not found.")
    }
    fn internal(m: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, m)
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        Self::internal(format!("database: {e}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

/// Run database and file work off the async threads.
async fn blocking<T: Send + 'static>(
    st: &Arc<App>,
    f: impl FnOnce(&Connection, &Store) -> R<T> + Send + 'static,
) -> R<T> {
    let st = st.clone();
    tokio::task::spawn_blocking(move || {
        let db = st
            .db
            .lock()
            .map_err(|_| ApiError::internal("database lock"))?;
        f(&db, &st.store)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
}

fn cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|c| c.trim().strip_prefix(auth::COOKIE)?.strip_prefix('='))
        .map(str::to_string)
}

/// The signed-in user, from an API token or the web page's session cookie.
impl FromRequestParts<Arc<App>> for User {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, st: &Arc<App>) -> R<Self> {
        let bearer = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|t| t.trim().to_string());
        let unauthorized = || ApiError::new(StatusCode::UNAUTHORIZED, "Sign in first.");
        if let Some(t) = bearer {
            return blocking(st, move |db, _| Ok(auth::by_token(db, &t)?))
                .await?
                .ok_or_else(unauthorized);
        }
        let Some(s) = cookie(&parts.headers) else {
            return Err(unauthorized());
        };
        // changes from a browser must come from this page's own script (a header other sites
        // cannot add), so another site cannot act with the cookie
        if !matches!(parts.method, Method::GET | Method::HEAD)
            && !parts.headers.contains_key("x-ec")
        {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "Missing the X-EC header.",
            ));
        }
        blocking(st, move |db, _| Ok(auth::by_session(db, &s)?))
            .await?
            .ok_or_else(unauthorized)
    }
}

struct Admin(User);

impl FromRequestParts<Arc<App>> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, st: &Arc<App>) -> R<Self> {
        let u = User::from_request_parts(parts, st).await?;
        if !u.admin {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "Only an administrator can do this.",
            ));
        }
        Ok(Admin(u))
    }
}

pub fn router(app: Arc<App>) -> Router {
    let api = Router::new()
        .route("/status", get(status))
        .route("/setup", post(setup))
        .route("/session", post(login).delete(logout))
        .route("/me", get(me))
        .route("/me/password", post(change_password))
        .route("/designs", get(list).post(upload))
        .route("/designs/{id}", get(one).patch(change).delete(remove))
        .route("/designs/{id}/files", post(add_files))
        .route("/designs/{id}/preview", get(preview))
        .route("/designs/{id}/machine-file", get(machine_file))
        .route("/designs/{id}/colourways", post(add_colourway))
        .route(
            "/colourways/{id}",
            patch(change_colourway).delete(delete_colourway),
        )
        .route("/settings", get(settings).patch(change_settings))
        .route(
            "/backup",
            get(backup_status).patch(change_backup).post(back_up_now),
        )
        .route("/files/{id}", get(download).delete(remove_file))
        .route("/tags", get(tags))
        .route("/customers", get(list_customers).post(add_customer))
        .route(
            "/customers/{id}",
            get(one_customer)
                .patch(change_customer)
                .delete(delete_customer),
        )
        .route("/customers/old-numbers", post(name_old_numbers))
        .route("/usages", get(list_usages).post(add_usage))
        .route("/usages/{id}", patch(change_usage).delete(delete_usage))
        .route("/users", get(users).post(add_user))
        .route("/users/{id}", delete(remove_user))
        .route("/tokens", get(tokens).post(add_token))
        .route("/tokens/{id}", delete(remove_token))
        .route("/thread-charts", get(thread_charts).post(add_chart))
        .route(
            "/thread-charts/{id}",
            patch(change_chart).delete(delete_chart),
        )
        .route("/thread-charts/{id}/import", post(import_chart))
        .route("/thread-charts/{id}/export", get(export_chart))
        .route("/threads", get(list_threads).post(add_thread))
        .route("/threads/{id}", patch(change_thread).delete(delete_thread))
        .route("/designs/{id}/match", post(match_design))
        .route("/machines", get(list_machines).post(add_machine))
        .route("/machines/key", get(machine_key))
        .route(
            "/machines/stick-card",
            get(stick_card_start).post(stick_card),
        )
        .route("/machines/stick-probe", post(stick_probe))
        .route(
            "/machines/{id}",
            patch(change_machine).delete(delete_machine),
        )
        .route(
            "/machines/{id}/stick",
            get(stick_status).post(send_to_stick),
        )
        .route("/machines/{id}/stick/remove", post(remove_from_stick))
        .route("/machines/{id}/stick/clear", post(clear_stick))
        .route("/machines/{id}/needle-plan", post(needle_plan))
        .route("/designs/{id}/published", get(design_published))
        .route("/designs/{id}/publish", post(publish_design))
        .route("/jobs", get(list_jobs).post(add_job))
        .route("/jobs/{id}", patch(change_job).delete(delete_job))
        .route("/jobs/{id}/load", post(load_job))
        .route(
            "/jobs/{id}/design",
            get(job_design_choices).post(set_job_design),
        )
        .route("/connections", get(list_connections).post(add_connection))
        .route(
            "/connections/{id}",
            patch(change_connection).delete(delete_connection),
        )
        .route("/connections/{id}/test", post(test_connection))
        .route("/connections/{id}/sync", post(sync_connection))
        .route(
            "/connections/{id}/machines",
            get(connection_machines).put(link_machines),
        )
        .route(
            "/connections/{id}/designs",
            get(connection_designs).put(link_design),
        )
        .route(
            "/connections/{id}/customers",
            get(connection_customers).put(link_customers),
        )
        .route(
            "/designs/{id}/needles-from-machine",
            post(needles_from_machine),
        )
        .layer(DefaultBodyLimit::max(1024 * 1024 * 1024));
    Router::new()
        .route(
            "/",
            get(|| page("text/html; charset=utf-8", include_str!("web/index.html"))),
        )
        .route(
            "/app.js",
            get(|| page("text/javascript; charset=utf-8", include_str!("web/app.js"))),
        )
        .route(
            "/style.css",
            get(|| page("text/css; charset=utf-8", include_str!("web/style.css"))),
        )
        .nest("/api/v1", api)
        .fallback(|| async { ApiError::not_found() })
        .with_state(app)
}

async fn page(kind: &'static str, body: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, kind),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response()
}

async fn status(State(st): St, headers: HeaderMap) -> R<Json<Value>> {
    let session = cookie(&headers);
    let (users, me) = blocking(&st, move |db, _| {
        let me = match session {
            Some(s) => auth::by_session(db, &s)?,
            None => None,
        };
        Ok((auth::user_count(db)?, me))
    })
    .await?;
    Ok(Json(json!({
        "product": "EmbroideryCraft Library",
        "version": env!("CARGO_PKG_VERSION"),
        "api": 1,
        "setup_needed": users == 0,
        "user": me,
    })))
}

#[derive(Deserialize)]
struct Login {
    name: String,
    password: String,
}

fn session_cookie(secret: &str, max_age: i64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{}={secret}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}",
        auth::COOKIE
    ))
    .unwrap_or(HeaderValue::from_static(""))
}

/// The first user, an administrator; only while there is no user yet.
async fn setup(State(st): St, Json(b): Json<Login>) -> R<Response> {
    let secret = blocking(&st, move |db, _| {
        if auth::user_count(db)? > 0 {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "The server is already set up.",
            ));
        }
        auth::add_user(db, &b.name, &b.password, true).map_err(ApiError::bad)?;
        auth::login(db, &b.name, &b.password)?
            .ok_or_else(|| ApiError::internal("could not sign in"))
    })
    .await?;
    Ok((
        [(header::SET_COOKIE, session_cookie(&secret, 30 * 24 * 3600))],
        Json(json!({"ok": true})),
    )
        .into_response())
}

async fn login(State(st): St, Json(b): Json<Login>) -> R<Response> {
    let s = blocking(&st, move |db, _| Ok(auth::login(db, &b.name, &b.password)?)).await?;
    match s {
        Some(secret) => Ok((
            [(header::SET_COOKIE, session_cookie(&secret, 30 * 24 * 3600))],
            Json(json!({"ok": true})),
        )
            .into_response()),
        None => {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "Wrong name or password.",
            ))
        }
    }
}

async fn logout(State(st): St, headers: HeaderMap) -> R<Response> {
    if let Some(s) = cookie(&headers) {
        blocking(&st, move |db, _| Ok(auth::logout(db, &s)?)).await?;
    }
    Ok((
        [(header::SET_COOKIE, session_cookie("", 0))],
        Json(json!({"ok": true})),
    )
        .into_response())
}

async fn me(u: User) -> Json<User> {
    Json(u)
}

#[derive(Deserialize)]
struct NewPassword {
    old: String,
    new: String,
}

async fn change_password(State(st): St, u: User, Json(b): Json<NewPassword>) -> R<Json<Value>> {
    blocking(&st, move |db, _| {
        if auth::login(db, &u.name, &b.old)?.is_none() {
            return Err(ApiError::bad("The current password is wrong."));
        }
        auth::set_password(db, &u.name, &b.new).map_err(ApiError::bad)?;
        Ok(())
    })
    .await?;
    Ok(Json(json!({"ok": true})))
}

async fn list(State(st): St, _u: User, Query(s): Query<Search>) -> R<Json<Value>> {
    let (total, items) = blocking(&st, move |db, _| Ok(db::search(db, &s)?)).await?;
    Ok(Json(json!({ "total": total, "items": items })))
}

async fn one(State(st): St, _u: User, Path(id): Path<i64>) -> R<Json<db::Design>> {
    let d = blocking(&st, move |db, _| Ok(db::design(db, id)?)).await?;
    d.map(Json).ok_or_else(ApiError::not_found)
}

/// Files and the placement fields (`tags`, `customer_id`, `usage`, `kind`) from a multipart
/// form.
async fn read_upload(mut mp: Multipart) -> R<(Vec<Incoming>, Placement)> {
    let mut files = Vec::new();
    let mut place = Placement::default();
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?
    {
        match field.name().unwrap_or("") {
            "file" | "files" => {
                let name = field.file_name().unwrap_or("file").to_string();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::bad(e.to_string()))?;
                files.push(Incoming {
                    name,
                    bytes: bytes.to_vec(),
                });
            }
            name @ ("tags" | "customer_id" | "usage" | "kind") => {
                let name = name.to_string();
                let t = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad(e.to_string()))?;
                let t = t.trim();
                match name.as_str() {
                    "tags" => place.tags = db::split_tags(t),
                    "customer_id" if !t.is_empty() => {
                        place.customer = Some(
                            t.parse()
                                .map_err(|_| ApiError::bad("customer_id is a number."))?,
                        )
                    }
                    "usage" => place.usage = t.to_string(),
                    "kind" => place.kind = kind(t)?,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if files.is_empty() {
        return Err(ApiError::bad("No files sent (form field \"file\")."));
    }
    Ok((files, place))
}

fn kind(s: &str) -> R<Option<&'static str>> {
    match s {
        "" => Ok(None),
        "artwork" => Ok(Some("artwork")),
        "name" => Ok(Some("name")),
        _ => Err(ApiError::bad("kind is \"artwork\" or \"name\".")),
    }
}

fn check_customer(db: &Connection, id: Option<i64>) -> R<()> {
    if let Some(c) = id {
        if customers::customer(db, c)?.is_none() {
            return Err(ApiError::bad("No such customer."));
        }
    }
    Ok(())
}

async fn upload(State(st): St, _u: User, mp: Multipart) -> R<Json<library::Report>> {
    let (files, place) = read_upload(mp).await?;
    let r = blocking(&st, move |db, store| {
        check_customer(db, place.customer)?;
        Ok(library::import(db, store, files, &place, None)?)
    })
    .await?;
    Ok(Json(r))
}

async fn add_files(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    mp: Multipart,
) -> R<Json<library::Report>> {
    let (files, place) = read_upload(mp).await?;
    let r = blocking(&st, move |db, store| {
        if db::design(db, id)?.is_none() {
            return Err(ApiError::not_found());
        }
        Ok(library::import(
            db,
            store,
            files,
            &Placement::tags(&place.tags),
            Some(id),
        )?)
    })
    .await?;
    Ok(Json(r))
}

#[derive(Deserialize)]
struct Change {
    name: Option<String>,
    notes: Option<String>,
    tags: Option<Vec<String>>,
    threads: Option<Vec<ThreadColour>>,
    /// The customers using the design, in order; `[]` for none.
    customer_ids: Option<Vec<i64>>,
    kind: Option<String>,
    usage: Option<String>,
    archived: Option<bool>,
    /// The needle for each colour (empty: chosen when the file is made).
    needles: Option<Vec<u8>>,
    /// Stitches per minute; `null` for the shop's default.
    #[serde(default, with = "double_option_u32")]
    speed: Option<Option<u32>>,
    /// The fabric of the main colours, `#rrggbb`, or "" for none.
    background: Option<String>,
}

mod double_option_u32 {
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<u32>>, D::Error> {
        Ok(Some(Option::deserialize(d)?))
    }
}

fn is_rgb(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

async fn change(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(c): Json<Change>,
) -> R<Json<db::Design>> {
    let d = blocking(&st, move |db, store| {
        let Some(d) = db::design(db, id)? else {
            return Err(ApiError::not_found());
        };
        let tx = db.unchecked_transaction()?;
        if let Some(name) = c.name {
            let name = name.trim();
            if name.is_empty() {
                return Err(ApiError::bad("The name can't be empty."));
            }
            tx.execute(
                "UPDATE designs SET name = ? WHERE id = ?",
                params![name, id],
            )?;
        }
        if let Some(notes) = c.notes {
            tx.execute(
                "UPDATE designs SET notes = ? WHERE id = ?",
                params![notes, id],
            )?;
        }
        if let Some(tags) = c.tags {
            db::set_tags(&tx, id, &tags)?;
        }
        if let Some(ids) = c.customer_ids {
            let mut seen = Vec::new();
            for c in ids {
                check_customer(&tx, Some(c))?;
                if !seen.contains(&c) {
                    seen.push(c);
                }
            }
            customers::set_customers(&tx, id, &seen)?;
        }
        if let Some(k) = c.kind {
            let k = kind(k.trim())?.unwrap_or("artwork");
            tx.execute("UPDATE designs SET kind = ? WHERE id = ?", params![k, id])?;
        }
        if let Some(u) = c.usage {
            tx.execute(
                "UPDATE designs SET usage = ? WHERE id = ?",
                params![u.trim(), id],
            )?;
        }
        if let Some(a) = c.archived {
            tx.execute(
                "UPDATE designs SET archived = ? WHERE id = ?",
                params![a, id],
            )?;
        }
        if let Some(b) = &c.background {
            let b = colourways::check_background(b).map_err(ApiError::bad)?;
            tx.execute(
                "UPDATE designs SET background = ? WHERE id = ?",
                params![b, id],
            )?;
        }
        if let Some(sp) = c.speed {
            if sp.is_some_and(|s| !(50..=3000).contains(&s)) {
                return Err(ApiError::bad(
                    "Speed: between 50 and 3000 stitches a minute.",
                ));
            }
            tx.execute("UPDATE designs SET speed = ? WHERE id = ?", params![sp, id])?;
        }
        if let Some(n) = c.needles {
            colourways::check_needles(&n, d.threads.len(), colourways::needle_count(&tx)?)
                .map_err(ApiError::bad)?;
            let json = serde_json::to_string(&n).map_err(|e| ApiError::internal(e.to_string()))?;
            tx.execute(
                "UPDATE designs SET needles = ? WHERE id = ?",
                params![json, id],
            )?;
        }
        if let Some(mut threads) = c.threads {
            if threads.len() != d.threads.len() {
                return Err(ApiError::bad(format!(
                    "The design has {} colours, not {}.",
                    d.threads.len(),
                    threads.len()
                )));
            }
            for t in &mut threads {
                t.rgb = t.rgb.to_lowercase();
                if !is_rgb(&t.rgb) {
                    return Err(ApiError::bad(format!(
                        "\"{}\" is not a colour like #1a2b3c.",
                        t.rgb
                    )));
                }
            }
            let json =
                serde_json::to_string(&threads).map_err(|e| ApiError::internal(e.to_string()))?;
            tx.execute(
                "UPDATE designs SET threads = ? WHERE id = ?",
                params![json, id],
            )?;
            store.clear_thumbs(id);
        }
        db::touch(&tx, id)?;
        tx.commit()?;
        db::design(db, id)?.ok_or_else(ApiError::not_found)
    })
    .await?;
    Ok(Json(d))
}

async fn remove(State(st): St, _u: User, Path(id): Path<i64>) -> R<Json<Value>> {
    let gone = blocking(&st, move |db, store| {
        Ok(library::remove_design(db, store, id)?)
    })
    .await?;
    if !gone {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok": true})))
}

async fn remove_file(State(st): St, _u: User, Path(id): Path<i64>) -> R<Json<Value>> {
    let kept = blocking(&st, move |db, store| {
        Ok(library::remove_file(db, store, id)?)
    })
    .await?;
    match kept {
        Some(design_kept) => Ok(Json(json!({"ok": true, "design_kept": design_kept}))),
        None => Err(ApiError::not_found()),
    }
}

#[derive(Deserialize)]
struct Size {
    size: Option<u32>,
    colourway: Option<i64>,
    /// Show this colour (0-based) and fade the others.
    focus: Option<usize>,
}

async fn preview(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Query(s): Query<Size>,
) -> R<Response> {
    let size = s.size.unwrap_or(library::SIZES[0]);
    let png = blocking(&st, move |db, store| {
        Ok(library::preview(db, store, id, size, s.colourway, s.focus)?)
    })
    .await?;
    let png = png.ok_or_else(ApiError::not_found)?;
    // the page asks with ?v=<last change>, so a changed design gets a new address
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "private, max-age=86400"),
        ],
        png,
    )
        .into_response())
}

#[derive(Deserialize)]
struct MachineFile {
    /// "tbf" or "dst" (default).
    #[serde(default)]
    format: String,
    colourway: Option<i64>,
}

/// The design as a machine file: TBF with the needles of the main colours or of a colourway, or
/// DST.
async fn machine_file(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Query(q): Query<MachineFile>,
) -> R<Response> {
    let tbf = match q.format.to_lowercase().as_str() {
        "tbf" => true,
        "" | "dst" => false,
        _ => return Err(ApiError::bad("format is \"tbf\" or \"dst\".")),
    };
    let (name, bytes) = blocking(&st, move |db, store| {
        colourways::machine_file(db, store, id, q.colourway, tbf, None, None).map_err(ApiError::bad)
    })
    .await?
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "This design has no stitch file."))?;
    Ok(attachment(&name, bytes))
}

/// A download under `name`.
fn attachment(name: &str, bytes: Vec<u8>) -> Response {
    let plain: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let disposition = format!(
        "attachment; filename=\"{plain}\"; filename*=UTF-8''{}",
        encode_name(name)
    );
    let mut res = bytes.into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Ok(v) = HeaderValue::from_str(&disposition) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    res
}

#[derive(Deserialize)]
struct NewColourway {
    name: String,
    #[serde(default)]
    background: String,
}

async fn add_colourway(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<NewColourway>,
) -> R<Json<db::Design>> {
    let d = blocking(&st, move |db, _| {
        let Some(d) = db::design(db, id)? else {
            return Err(ApiError::not_found());
        };
        colourways::add(db, &d, &b.name, &b.background).map_err(ApiError::bad)?;
        db::touch(db, id)?;
        db::design(db, id)?.ok_or_else(ApiError::not_found)
    })
    .await?;
    Ok(Json(d))
}

async fn change_colourway(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<colourways::Change>,
) -> R<Json<db::Design>> {
    let d = blocking(&st, move |db, _| {
        let Some((_, design)) = colourways::get(db, id)? else {
            return Err(ApiError::not_found());
        };
        colourways::change(db, id, b).map_err(ApiError::bad)?;
        db::touch(db, design)?;
        db::design(db, design)?.ok_or_else(ApiError::not_found)
    })
    .await?;
    Ok(Json(d))
}

async fn delete_colourway(State(st): St, _u: User, Path(id): Path<i64>) -> R<Json<db::Design>> {
    let d = blocking(&st, move |db, _| {
        let Some((_, design)) = colourways::get(db, id)? else {
            return Err(ApiError::not_found());
        };
        colourways::delete(db, id)?;
        db::touch(db, design)?;
        db::design(db, design)?.ok_or_else(ApiError::not_found)
    })
    .await?;
    Ok(Json(d))
}

fn settings_json(db: &Connection) -> R<Value> {
    Ok(json!({
        "needle_count": colourways::needle_count(db)?,
        "costing": costing::get(db)?,
    }))
}

async fn settings(State(st): St, _u: User) -> R<Json<Value>> {
    Ok(Json(blocking(&st, |db, _| settings_json(db)).await?))
}

#[derive(Deserialize)]
struct SettingsChange {
    needle_count: Option<u8>,
    costing: Option<costing::Costing>,
}

async fn change_settings(
    State(st): St,
    _a: Admin,
    Json(b): Json<SettingsChange>,
) -> R<Json<Value>> {
    let n = blocking(&st, move |db, _| {
        if let Some(n) = b.needle_count {
            colourways::set_needle_count(db, n).map_err(ApiError::bad)?;
        }
        if let Some(c) = &b.costing {
            costing::set(db, c).map_err(ApiError::bad)?;
        }
        settings_json(db)
    })
    .await?;
    Ok(Json(n))
}

fn backup_json(db: &Connection, store: &Store) -> R<Value> {
    let p = backup::plan(db)?;
    let copies = if p.folder.is_empty() {
        Vec::new()
    } else {
        backup::copies(std::path::Path::new(&p.folder))
    };
    Ok(json!({
        "plan": p,
        "last": backup::last(db)?,
        "last_good": backup::last_good(db)?,
        "running": backup::running(),
        "copies": copies,
        "library": store.root().display().to_string(),
    }))
}

async fn backup_status(State(st): St, _a: Admin) -> R<Json<Value>> {
    Ok(Json(blocking(&st, backup_json).await?))
}

async fn change_backup(State(st): St, _a: Admin, Json(b): Json<backup::Plan>) -> R<Json<Value>> {
    let v = blocking(&st, move |db, store| {
        backup::set_plan(db, store.root(), &b).map_err(ApiError::bad)?;
        backup_json(db, store)
    })
    .await?;
    Ok(Json(v))
}

/// Start a backup now, in the background; `GET /backup` tells when it is done.
async fn back_up_now(State(st): St, _a: Admin) -> R<Json<Value>> {
    let v = blocking(&st, move |db, store| {
        if backup::plan(db)?.folder.is_empty() {
            return Err(ApiError::bad("Choose a backup folder first."));
        }
        if backup::running() {
            return Err(ApiError::bad("A backup is already running."));
        }
        let data = store.root().to_path_buf();
        std::thread::spawn(move || {
            if let Err(e) = backup::run_planned(&data) {
                eprintln!("Backup failed: {e}");
            }
        });
        let mut v = backup_json(db, store)?;
        v["running"] = json!(true);
        Ok(v)
    })
    .await?;
    Ok(Json(v))
}

/// `filename*` form of a file name (RFC 5987), so accents and spaces survive.
fn encode_name(name: &str) -> String {
    let mut s = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

async fn download(State(st): St, _u: User, Path(id): Path<i64>) -> R<Response> {
    let (f, bytes) = blocking(&st, move |db, store| {
        let Some((f, _)) = db::file(db, id)? else {
            return Err(ApiError::not_found());
        };
        let bytes = store
            .get(&f.sha256)
            .map_err(|e| ApiError::internal(format!("the stored file is missing: {e}")))?;
        Ok((f, bytes))
    })
    .await?;
    Ok(attachment(&f.name, bytes))
}

async fn tags(State(st): St, _u: User) -> R<Json<Value>> {
    let t = blocking(&st, |db, _| Ok(db::all_tags(db)?)).await?;
    Ok(Json(json!(
        t.into_iter()
            .map(|(tag, count)| json!({"tag": tag, "count": count}))
            .collect::<Vec<_>>()
    )))
}

// ---------- customers and usages ----------

async fn list_customers(State(st): St, _u: User) -> R<Json<Vec<customers::Customer>>> {
    Ok(Json(
        blocking(&st, |db, _| Ok(customers::customers(db)?)).await?,
    ))
}

async fn one_customer(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
) -> R<Json<customers::Customer>> {
    blocking(&st, move |db, _| Ok(customers::customer(db, id)?))
        .await?
        .map(Json)
        .ok_or_else(ApiError::not_found)
}

#[derive(Deserialize)]
struct NewCustomer {
    #[serde(default)]
    name: String,
    #[serde(default)]
    old_number: String,
    parent_id: Option<i64>,
    #[serde(default)]
    notes: String,
}

async fn add_customer(
    State(st): St,
    _u: User,
    Json(b): Json<NewCustomer>,
) -> R<Json<customers::Customer>> {
    let c = blocking(&st, move |db, _| {
        let id = customers::add_customer(db, &b.name, &b.old_number, b.parent_id, &b.notes)
            .map_err(ApiError::bad)?;
        customers::customer(db, id)?.ok_or_else(ApiError::not_found)
    })
    .await?;
    Ok(Json(c))
}

async fn change_customer(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<customers::CustomerChange>,
) -> R<Json<customers::Customer>> {
    let c = blocking(&st, move |db, _| {
        if !customers::change_customer(db, id, b).map_err(ApiError::bad)? {
            return Err(ApiError::not_found());
        }
        customers::customer(db, id)?.ok_or_else(ApiError::not_found)
    })
    .await?;
    Ok(Json(c))
}

async fn delete_customer(State(st): St, _a: Admin, Path(id): Path<i64>) -> R<Json<Value>> {
    if !blocking(&st, move |db, _| Ok(customers::delete_customer(db, id)?)).await? {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct Lines {
    text: String,
}

async fn name_old_numbers(
    State(st): St,
    _u: User,
    Json(b): Json<Lines>,
) -> R<Json<customers::NamesReport>> {
    Ok(Json(
        blocking(&st, move |db, _| {
            Ok(customers::name_old_numbers(db, &b.text)?)
        })
        .await?,
    ))
}

async fn list_usages(State(st): St, _u: User) -> R<Json<Vec<customers::Usage>>> {
    Ok(Json(
        blocking(&st, |db, _| Ok(customers::usages(db)?)).await?,
    ))
}

#[derive(Deserialize)]
struct NewUsage {
    name: String,
    #[serde(default)]
    words: String,
    #[serde(default)]
    names: bool,
}

async fn add_usage(State(st): St, _u: User, Json(b): Json<NewUsage>) -> R<Json<Value>> {
    let id = blocking(&st, move |db, _| {
        customers::add_usage(db, &b.name, &b.words, b.names).map_err(ApiError::bad)
    })
    .await?;
    Ok(Json(json!({"id": id})))
}

async fn change_usage(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<customers::UsageChange>,
) -> R<Json<Value>> {
    if !blocking(&st, move |db, _| {
        customers::change_usage(db, id, b).map_err(ApiError::bad)
    })
    .await?
    {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok": true})))
}

async fn delete_usage(State(st): St, _a: Admin, Path(id): Path<i64>) -> R<Json<Value>> {
    if !blocking(&st, move |db, _| Ok(customers::delete_usage(db, id)?)).await? {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok": true})))
}

async fn users(State(st): St, _a: Admin) -> R<Json<Vec<User>>> {
    Ok(Json(blocking(&st, |db, _| Ok(auth::users(db)?)).await?))
}

#[derive(Deserialize)]
struct NewUser {
    name: String,
    password: String,
    #[serde(default)]
    admin: bool,
}

async fn add_user(State(st): St, _a: Admin, Json(b): Json<NewUser>) -> R<Json<Value>> {
    let id = blocking(&st, move |db, _| {
        auth::add_user(db, &b.name, &b.password, b.admin).map_err(ApiError::bad)
    })
    .await?;
    Ok(Json(json!({"id": id})))
}

async fn remove_user(State(st): St, Admin(me): Admin, Path(id): Path<i64>) -> R<Json<Value>> {
    if id == me.id {
        return Err(ApiError::bad("You can't remove yourself."));
    }
    let n = blocking(&st, move |db, _| {
        Ok(db.execute("DELETE FROM users WHERE id = ?", [id])?)
    })
    .await?;
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok": true})))
}

async fn tokens(State(st): St, _a: Admin) -> R<Json<Vec<auth::TokenInfo>>> {
    Ok(Json(blocking(&st, |db, _| Ok(auth::tokens(db)?)).await?))
}

#[derive(Deserialize)]
struct NewToken {
    name: String,
}

async fn add_token(State(st): St, Admin(me): Admin, Json(b): Json<NewToken>) -> R<Json<Value>> {
    if b.name.trim().is_empty() {
        return Err(ApiError::bad(
            "Name the token after the program that uses it.",
        ));
    }
    let (id, token) = blocking(&st, move |db, _| Ok(auth::add_token(db, me.id, &b.name)?)).await?;
    Ok(Json(json!({"id": id, "token": token})))
}

async fn remove_token(State(st): St, _a: Admin, Path(id): Path<i64>) -> R<Json<Value>> {
    let n = blocking(&st, move |db, _| {
        Ok(db.execute("DELETE FROM tokens WHERE id = ?", [id])?)
    })
    .await?;
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok": true})))
}

// ---------- thread charts ----------

async fn thread_charts(State(st): St, _u: User) -> R<Json<Vec<threads::ChartInfo>>> {
    Ok(Json(blocking(&st, |db, _| Ok(threads::charts(db)?)).await?))
}

#[derive(Deserialize)]
struct NewChart {
    name: String,
    #[serde(default)]
    brand: String,
    #[serde(default)]
    notes: String,
}

async fn add_chart(
    State(st): St,
    _u: User,
    Json(b): Json<NewChart>,
) -> R<Json<threads::ChartInfo>> {
    let c = blocking(&st, move |db, _| {
        let id = threads::add_chart(db, &b.name, &b.brand, &b.notes).map_err(ApiError::bad)?;
        threads::chart(db, id)?.ok_or_else(ApiError::not_found)
    })
    .await?;
    Ok(Json(c))
}

async fn change_chart(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<threads::ChartChange>,
) -> R<Json<threads::ChartInfo>> {
    let c = blocking(&st, move |db, _| {
        if threads::chart(db, id)?.is_none() {
            return Err(ApiError::not_found());
        }
        threads::change_chart(db, id, b).map_err(ApiError::bad)?;
        threads::chart(db, id)?.ok_or_else(ApiError::not_found)
    })
    .await?;
    Ok(Json(c))
}

async fn delete_chart(State(st): St, _a: Admin, Path(id): Path<i64>) -> R<Json<Value>> {
    let gone = blocking(&st, move |db, _| {
        threads::delete_chart(db, id).map_err(ApiError::bad)
    })
    .await?;
    if !gone {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct CsvText {
    csv: String,
}

async fn import_chart(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<CsvText>,
) -> R<Json<threads::ImportReport>> {
    let r = blocking(&st, move |db, _| {
        threads::import(db, id, &b.csv).map_err(ApiError::bad)
    })
    .await?;
    Ok(Json(r))
}

async fn export_chart(State(st): St, _u: User, Path(id): Path<i64>) -> R<Response> {
    let (name, csv) = blocking(&st, move |db, _| Ok(threads::export(db, id)?))
        .await?
        .ok_or_else(ApiError::not_found)?;
    let file = format!("{name}.csv");
    let mut res = csv.into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    if let Ok(v) = HeaderValue::from_str(&format!(
        "attachment; filename=\"chart.csv\"; filename*=UTF-8''{}",
        encode_name(&file)
    )) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

async fn list_threads(
    State(st): St,
    _u: User,
    Query(q): Query<threads::ThreadQuery>,
) -> R<Json<Vec<threads::ThreadInfo>>> {
    Ok(Json(
        blocking(&st, move |db, _| Ok(threads::list(db, &q)?)).await?,
    ))
}

#[derive(Deserialize)]
struct NewThread {
    chart_id: i64,
    code: String,
    #[serde(default)]
    name: String,
    rgb: String,
}

async fn add_thread(State(st): St, _u: User, Json(b): Json<NewThread>) -> R<Json<Value>> {
    let id = blocking(&st, move |db, _| {
        threads::add_thread(db, b.chart_id, &b.code, &b.name, &b.rgb).map_err(ApiError::bad)
    })
    .await?;
    Ok(Json(json!({"id": id})))
}

async fn change_thread(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<threads::ThreadChange>,
) -> R<Json<Value>> {
    let found = blocking(&st, move |db, _| {
        threads::change_thread(db, id, b).map_err(ApiError::bad)
    })
    .await?;
    if !found {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok": true})))
}

async fn delete_thread(State(st): St, _u: User, Path(id): Path<i64>) -> R<Json<Value>> {
    if !blocking(&st, move |db, _| Ok(threads::delete_thread(db, id)?)).await? {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct MatchTo {
    /// The chart to match to; the shop's default chart if left out.
    chart_id: Option<i64>,
}

/// Give every colour of the design the closest thread of a chart.
async fn match_design(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<MatchTo>,
) -> R<Json<db::Design>> {
    let d = blocking(&st, move |db, store| {
        let Some(d) = db::design(db, id)? else {
            return Err(ApiError::not_found());
        };
        let chart = match b.chart_id {
            Some(c) => c,
            None => threads::default_chart(db)?.ok_or_else(|| {
                ApiError::bad("Choose a default thread chart first (Threads page).")
            })?,
        };
        let mut out = Vec::with_capacity(d.threads.len());
        for t in &d.threads {
            let Some(n) = threads::nearest(db, chart, &t.rgb)? else {
                return Err(ApiError::bad("That chart has no threads."));
            };
            out.push(ThreadColour {
                rgb: n.rgb,
                name: n.name,
                code: n.code,
                chart: n.chart,
            });
        }
        let json = serde_json::to_string(&out).map_err(|e| ApiError::internal(e.to_string()))?;
        db.execute(
            "UPDATE designs SET threads = ? WHERE id = ?",
            params![json, id],
        )?;
        db::touch(db, id)?;
        store.clear_thumbs(id);
        db::design(db, id)?.ok_or_else(ApiError::not_found)
    })
    .await?;
    Ok(Json(d))
}

// ---------- machines and their design sticks ----------

async fn list_machines(State(st): St, _u: User) -> R<Json<Vec<machines::Machine>>> {
    Ok(Json(blocking(&st, |db, _| Ok(machines::list(db)?)).await?))
}

async fn add_machine(
    State(st): St,
    _a: Admin,
    Json(b): Json<machines::Change>,
) -> R<Json<machines::Machine>> {
    let m = blocking(&st, move |db, _| {
        let id = machines::add(db, b).map_err(ApiError::bad)?;
        machines::get(db, id)?.ok_or_else(ApiError::not_found)
    })
    .await?;
    Ok(Json(m))
}

async fn change_machine(
    State(st): St,
    _a: Admin,
    Path(id): Path<i64>,
    Json(b): Json<machines::Change>,
) -> R<Json<machines::Machine>> {
    let m = blocking(&st, move |db, _| {
        if !machines::change(db, id, b).map_err(ApiError::bad)? {
            return Err(ApiError::not_found());
        }
        machines::get(db, id)?.ok_or_else(ApiError::not_found)
    })
    .await?;
    Ok(Json(m))
}

async fn delete_machine(State(st): St, _a: Admin, Path(id): Path<i64>) -> R<Json<Value>> {
    let gone = blocking(&st, move |db, _| Ok(machines::delete(db, id)?)).await?;
    if !gone {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"ok": true})))
}

/// The library's ssh key, to add to each design stick once.
async fn machine_key(State(st): St, _a: Admin) -> R<Json<Value>> {
    let data = st.store.root().to_path_buf();
    let key = tokio::task::spawn_blocking(move || stick::public_key(&data))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "key": key })))
}

/// What the "prepare a design stick" window starts with: the next free name.
async fn stick_card_start(State(st): St, _a: Admin) -> R<Json<Value>> {
    let sticks = blocking(&st, |db, _| {
        Ok(machines::list(db)?
            .into_iter()
            .map(|m| m.stick)
            .collect::<Vec<_>>())
    })
    .await?;
    Ok(Json(
        json!({ "name": card::next_name(&sticks), "user": "shop" }),
    ))
}

/// The files for a new stick's card, as a zip. The Wi-Fi password is only put in the zip.
async fn stick_card(State(st): St, _a: Admin, Json(b): Json<card::Request>) -> R<Response> {
    let data = st.store.root().to_path_buf();
    let name = b.name.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        let key = stick::public_key(&data).map_err(ApiError::internal)?;
        let zip = card::zip(&b, &key).map_err(ApiError::bad)?;
        // a new card has a new host key: the old one under that name would be refused
        stick::forget_host(&data, &format!("{}.local", b.name));
        Ok::<_, ApiError>(zip)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))??;
    let mut res = attachment(&format!("{name}-card.zip"), bytes);
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}

#[derive(Deserialize)]
struct Probe {
    target: String,
    #[serde(default)]
    via: String,
}

/// Does a stick answer at this address (a new one, before its machine is made)?
async fn stick_probe(State(st): St, _a: Admin, Json(b): Json<Probe>) -> R<Json<stick::Status>> {
    let target = machines::check_ssh_target(&b.target, "Design stick").map_err(ApiError::bad)?;
    let via = machines::check_ssh_target(&b.via, "Go through").map_err(ApiError::bad)?;
    let link = stick::Link { target, via };
    Ok(Json(
        on_stick(&st, move |data| stick::status(data, &link)).await?,
    ))
}

/// The machine and how to reach its stick.
async fn machine_link(st: &Arc<App>, id: i64) -> R<(machines::Machine, stick::Link)> {
    let m = blocking(st, move |db, _| Ok(machines::get(db, id)?))
        .await?
        .ok_or_else(ApiError::not_found)?;
    let link = stick::Link {
        target: m.stick.clone(),
        via: m.stick_via.clone(),
    };
    Ok((m, link))
}

/// Run ssh work without holding the database.
async fn on_stick<T: Send + 'static>(
    st: &Arc<App>,
    f: impl FnOnce(&std::path::Path) -> Result<T, String> + Send + 'static,
) -> R<T> {
    let data = st.store.root().to_path_buf();
    tokio::task::spawn_blocking(move || f(&data))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, e))
}

async fn stick_status(State(st): St, _u: User, Path(id): Path<i64>) -> R<Json<stick::Status>> {
    let (_, link) = machine_link(&st, id).await?;
    Ok(Json(
        on_stick(&st, move |data| stick::status(data, &link)).await?,
    ))
}

#[derive(Deserialize)]
struct SendItem {
    design: i64,
    colourway: Option<i64>,
    /// The needle of each colour, as checked before sending; without them every colour's
    /// thread must be on the machine.
    #[serde(default)]
    needles: Option<Vec<u8>>,
}

#[derive(Deserialize)]
struct SendDesigns {
    designs: Vec<SendItem>,
    /// Instead of what is on the stick, rather than added to it.
    #[serde(default)]
    replace: bool,
}

/// Send designs to the machine's stick, in the machine's format.
async fn send_to_stick(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<SendDesigns>,
) -> R<Json<Value>> {
    if b.designs.is_empty() || b.designs.len() > 100 {
        return Err(ApiError::bad("Send between 1 and 100 designs."));
    }
    let (m, link) = machine_link(&st, id).await?;
    let files = blocking(&st, move |db, store| {
        let items: Vec<machines::ToSend> = b
            .designs
            .iter()
            .map(|it| machines::ToSend {
                design: it.design,
                colourway: it.colourway,
                needles: it.needles.clone(),
            })
            .collect();
        machines::stick_files(db, store, &m, &items, true)
            .map(|(files, _)| files)
            .map_err(ApiError::bad)
    })
    .await?;
    let sent: Vec<String> = files
        .iter()
        .map(|(n, _)| n.clone())
        .filter(|n| !stick::is_companion(n))
        .collect();
    let status = on_stick(&st, move |data| stick::send(data, &link, &files, b.replace)).await?;
    Ok(Json(json!({ "sent": sent, "stick": status })))
}

#[derive(Deserialize)]
struct Names {
    names: Vec<String>,
}

async fn remove_from_stick(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<Names>,
) -> R<Json<stick::Status>> {
    let (_, link) = machine_link(&st, id).await?;
    Ok(Json(
        on_stick(&st, move |data| stick::remove(data, &link, &b.names)).await?,
    ))
}

async fn clear_stick(State(st): St, _u: User, Path(id): Path<i64>) -> R<Json<stick::Status>> {
    let (_, link) = machine_link(&st, id).await?;
    Ok(Json(
        on_stick(&st, move |data| stick::clear(data, &link)).await?,
    ))
}

#[derive(Deserialize)]
struct FromMachine {
    machine: i64,
    colourway: Option<i64>,
}

/// Choose the needles of the main colours (or a colourway) from a machine's needle threads.

#[derive(Deserialize)]
struct PlanFor {
    design: i64,
    colourway: Option<i64>,
}

/// The needle of each colour of a design on this machine, and whether its thread is there.
async fn needle_plan(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<PlanFor>,
) -> R<Json<Value>> {
    blocking(&st, move |db, _| {
        let m = machines::get(db, id)?.ok_or_else(ApiError::not_found)?;
        let (threads, set) = colourways::colours(db, b.design, b.colourway)
            .map_err(ApiError::bad)?
            .ok_or_else(ApiError::not_found)?;
        let steps = machines::plan(&m, &threads, &set);
        let ok = steps.iter().all(|s| s.ok);
        Ok(Json(json!({ "steps": steps, "ok": ok })))
    })
    .await
}

async fn needles_from_machine(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<FromMachine>,
) -> R<Json<db::Design>> {
    let d = blocking(&st, move |db, _| {
        let m = machines::get(db, b.machine)?.ok_or_else(ApiError::not_found)?;
        let Some(d) = db::design(db, id)? else {
            return Err(ApiError::not_found());
        };
        match b.colourway {
            Some(c) => {
                let Some((cw, of)) = colourways::get(db, c)? else {
                    return Err(ApiError::not_found());
                };
                if of != id {
                    return Err(ApiError::bad("That colourway is not one of this design's."));
                }
                let needles = machines::assign_needles(&m, &cw.threads);
                colourways::change(
                    db,
                    c,
                    colourways::Change {
                        needles: Some(needles),
                        ..Default::default()
                    },
                )
                .map_err(ApiError::bad)?;
            }
            None => {
                let needles = machines::assign_needles(&m, &d.threads);
                db.execute(
                    "UPDATE designs SET needles = ? WHERE id = ?",
                    params![
                        serde_json::to_string(&needles).unwrap_or_else(|_| "[]".into()),
                        id
                    ],
                )?;
            }
        }
        db::touch(db, id)?;
        db::design(db, id)?.ok_or_else(ApiError::not_found)
    })
    .await?;
    Ok(Json(d))
}

// ---------- jobs ----------

#[derive(Deserialize)]
struct JobsQuery {
    /// Also the jobs finished in the last this many days (default 7).
    days: Option<i64>,
}

async fn list_jobs(State(st): St, _u: User, Query(q): Query<JobsQuery>) -> R<Json<Vec<jobs::Job>>> {
    let days = q.days.unwrap_or(7).clamp(0, 3650);
    Ok(Json(
        blocking(&st, move |db, _| Ok(jobs::list(db, days)?)).await?,
    ))
}

async fn add_job(State(st): St, _u: User, Json(c): Json<jobs::Change>) -> R<Json<jobs::Job>> {
    Ok(Json(
        blocking(&st, move |db, _| {
            if c.name.is_none() {
                return Err(ApiError::bad("Give the job a name."));
            }
            let id = jobs::add(db, &c).map_err(ApiError::bad)?;
            jobs::get(db, id)?.ok_or_else(ApiError::not_found)
        })
        .await?,
    ))
}

async fn change_job(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(c): Json<jobs::Change>,
) -> R<Json<jobs::Job>> {
    Ok(Json(
        blocking(&st, move |db, _| {
            jobs::get(db, id)?.ok_or_else(ApiError::not_found)?;
            jobs::change(db, id, &c).map_err(ApiError::bad)?;
            jobs::get(db, id)?.ok_or_else(ApiError::not_found)
        })
        .await?,
    ))
}

async fn delete_job(State(st): St, _a: Admin, Path(id): Path<i64>) -> R<Json<Value>> {
    let gone = blocking(&st, move |db, _| Ok(jobs::delete(db, id)?)).await?;
    if !gone {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({ "deleted": id })))
}

/// Work that talks to another system or a stick: on its own database connection, so the rest of
/// the library is not held up while it waits.
async fn away<T: Send + 'static>(
    st: &Arc<App>,
    f: impl FnOnce(&Connection, &Store) -> Result<T, String> + Send + 'static,
) -> R<T> {
    let root = st.store.root().to_path_buf();
    tokio::task::spawn_blocking(move || {
        let store = Store::open(&root).map_err(|e| e.to_string())?;
        let db = db::open(&root.join("library.db")).map_err(|e| e.to_string())?;
        f(&db, &store)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, e))
}

/// Put the job's design on its machine's stick now.
async fn load_job(State(st): St, _u: User, Path(id): Path<i64>) -> R<Json<Value>> {
    blocking(&st, move |db, _| {
        jobs::get(db, id)?
            .ok_or_else(ApiError::not_found)
            .map(|_| ())
    })
    .await?;
    let message = away(&st, move |db, store| connections::load(db, store, id)).await?;
    Ok(Json(json!({ "message": message })))
}

// ---------- connections to other systems ----------

async fn list_connections(State(st): St, _a: Admin) -> R<Json<Vec<connections::Info>>> {
    Ok(Json(
        blocking(&st, |db, _| Ok(connections::list(db)?)).await?,
    ))
}

async fn add_connection(
    State(st): St,
    _a: Admin,
    Json(c): Json<connections::Change>,
) -> R<Json<connections::Info>> {
    Ok(Json(
        blocking(&st, move |db, _| {
            let id = connections::save(db, None, &c).map_err(ApiError::bad)?;
            Ok(connections::get(db, id)?.ok_or_else(ApiError::not_found)?.0)
        })
        .await?,
    ))
}

async fn change_connection(
    State(st): St,
    _a: Admin,
    Path(id): Path<i64>,
    Json(c): Json<connections::Change>,
) -> R<Json<connections::Info>> {
    Ok(Json(
        blocking(&st, move |db, _| {
            connections::get(db, id)?.ok_or_else(ApiError::not_found)?;
            connections::save(db, Some(id), &c).map_err(ApiError::bad)?;
            Ok(connections::get(db, id)?.ok_or_else(ApiError::not_found)?.0)
        })
        .await?,
    ))
}

async fn delete_connection(State(st): St, _a: Admin, Path(id): Path<i64>) -> R<Json<Value>> {
    if !blocking(&st, move |db, _| Ok(connections::delete(db, id)?)).await? {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({ "deleted": id })))
}

/// The connection and its connector, for a call to the other system.
fn connector_of(db: &Connection, id: i64) -> Result<Box<dyn connections::Connector>, String> {
    let (info, secret) = connections::get(db, id)
        .map_err(|e| e.to_string())?
        .ok_or("No such connection.")?;
    connections::connector(&info, &secret)
}

async fn test_connection(State(st): St, _a: Admin, Path(id): Path<i64>) -> R<Json<Value>> {
    let message = away(&st, move |db, _| connector_of(db, id)?.test()).await?;
    Ok(Json(json!({ "message": message })))
}

async fn sync_connection(
    State(st): St,
    _a: Admin,
    Path(id): Path<i64>,
) -> R<Json<connections::Report>> {
    Ok(Json(
        away(&st, move |db, store| {
            let r = connections::sync(db, store, id);
            if let Err(e) = &r {
                let _ = db.execute(
                    "UPDATE connections SET last_error = ? WHERE id = ?",
                    params![e, id],
                );
            }
            r
        })
        .await?,
    ))
}

/// The other system's machines (work centers) and the library machine each is linked to.
async fn connection_machines(State(st): St, _a: Admin, Path(id): Path<i64>) -> R<Json<Value>> {
    Ok(Json(
        away(&st, move |db, _| {
            let remote = connector_of(db, id)?.machines()?;
            let links = connections::links(db, id, "machine").map_err(|e| e.to_string())?;
            let rows: Vec<Value> = remote
                .iter()
                .map(|r| {
                    let machine = links
                        .iter()
                        .find(|l| l.model == r.model && l.external_id == r.id)
                        .map(|l| l.local_id);
                    json!({ "remote": r, "machine_id": machine })
                })
                .collect();
            Ok(json!({ "machines": rows }))
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct MachineLink {
    model: String,
    external_id: String,
    label: String,
    machine_id: Option<i64>,
}

#[derive(Deserialize)]
struct MachineLinks {
    links: Vec<MachineLink>,
}

async fn link_machines(
    State(st): St,
    _a: Admin,
    Path(id): Path<i64>,
    Json(b): Json<MachineLinks>,
) -> R<Json<Vec<connections::Link>>> {
    Ok(Json(
        blocking(&st, move |db, _| {
            connections::get(db, id)?.ok_or_else(ApiError::not_found)?;
            for l in &b.links {
                if let Some(m) = l.machine_id {
                    machines::get(db, m)?.ok_or_else(|| ApiError::bad("No such machine."))?;
                }
                let r = Remote {
                    model: l.model.clone(),
                    id: l.external_id.clone(),
                    label: l.label.clone(),
                };
                connections::set_link(db, id, "machine", &r, l.machine_id)?;
            }
            Ok(connections::links(db, id, "machine")?)
        })
        .await?,
    ))
}

/// The library's customers beside the other system's: linked, surely the same, or likely.
async fn connection_customers(State(st): St, _a: Admin, Path(id): Path<i64>) -> R<Json<Value>> {
    Ok(Json(
        away(&st, move |db, _| {
            let mut remote = connector_of(db, id)?.customers()?;
            remote.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then(b.rank.cmp(&a.rank))
            });
            let matches =
                connections::match_customers(db, id, &remote).map_err(|e| e.to_string())?;
            Ok(json!({ "matches": matches, "remote": remote }))
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct CustomerLink {
    customer_id: i64,
    /// None unlinks.
    remote: Option<connections::RemoteCustomer>,
    /// Also rename the library customer to the other system's name.
    #[serde(default)]
    take_name: bool,
}

#[derive(Deserialize)]
struct CustomerLinks {
    links: Vec<CustomerLink>,
}

async fn link_customers(
    State(st): St,
    _a: Admin,
    Path(id): Path<i64>,
    Json(b): Json<CustomerLinks>,
) -> R<Json<Value>> {
    blocking(&st, move |db, _| {
        connections::get(db, id)?.ok_or_else(ApiError::not_found)?;
        let tx = db.unchecked_transaction()?;
        for l in &b.links {
            connections::link_customer(&tx, id, l.customer_id, l.remote.as_ref(), l.take_name)
                .map_err(ApiError::bad)?;
        }
        tx.commit()?;
        Ok(Json(json!({ "ok": true, "changed": b.links.len() })))
    })
    .await
}

/// What names a design in the other system (operations on the linked work centers), with the
/// design each is linked to, or designs whose name shares a code with it.
async fn connection_designs(State(st): St, _a: Admin, Path(id): Path<i64>) -> R<Json<Value>> {
    Ok(Json(
        away(&st, move |db, _| {
            let machines: Vec<String> = connections::links(db, id, "machine")
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|l| l.external_id)
                .collect();
            if machines.is_empty() {
                return Ok(json!({ "designs": [] }));
            }
            let remote = connector_of(db, id)?.design_sources(&machines)?;
            let links = connections::links(db, id, "design").map_err(|e| e.to_string())?;
            let mut rows = Vec::new();
            for r in &remote {
                let linked = links
                    .iter()
                    .find(|l| l.model == r.model && l.external_id == r.id)
                    .map(|l| l.local_id);
                let design = match linked {
                    Some(d) => db::design(db, d)
                        .map_err(|e| e.to_string())?
                        .map(|d| json!({ "id": d.id, "name": d.name })),
                    None => None,
                };
                let suggestions: Vec<Value> = if design.is_none() {
                    connections::suggest_designs(db, &r.label)
                        .map_err(|e| e.to_string())?
                        .into_iter()
                        .map(|(id, name)| json!({ "id": id, "name": name }))
                        .collect()
                } else {
                    Vec::new()
                };
                rows.push(json!({ "remote": r, "design": design, "suggestions": suggestions }));
            }
            Ok(json!({ "designs": rows }))
        })
        .await?,
    ))
}

#[derive(Deserialize)]
struct DesignLink {
    model: String,
    external_id: String,
    label: String,
    design_id: Option<i64>,
}

async fn link_design(
    State(st): St,
    _a: Admin,
    Path(id): Path<i64>,
    Json(l): Json<DesignLink>,
) -> R<Json<Value>> {
    blocking(&st, move |db, _| {
        connections::get(db, id)?.ok_or_else(ApiError::not_found)?;
        if let Some(d) = l.design_id {
            db::design(db, d)?.ok_or_else(|| ApiError::bad("No such design."))?;
        }
        let r = Remote {
            model: l.model,
            id: l.external_id,
            label: l.label,
        };
        connections::set_link(db, id, "design", &r, l.design_id)?;
        // jobs waiting for this name get the design
        if let Some(d) = l.design_id {
            db.execute(
                "UPDATE jobs SET design_id = ? WHERE design_id IS NULL AND design_hint = ? AND status IN ('waiting', 'problem')",
                params![d, r.label.split(" — ").next().unwrap_or(&r.label)],
            )?;
        }
        Ok(Json(json!({ "linked": l.design_id })))
    })
    .await
}

/// Designs whose name shares a code with the job's outside name, to choose its design.
async fn job_design_choices(State(st): St, _u: User, Path(id): Path<i64>) -> R<Json<Value>> {
    blocking(&st, move |db, _| {
        let job = jobs::get(db, id)?.ok_or_else(ApiError::not_found)?;
        let label = if job.design_hint.is_empty() {
            &job.name
        } else {
            &job.design_hint
        };
        let mut s: Vec<Value> = Vec::new();
        for (id, _) in connections::suggest_designs(db, label)? {
            if let Some(d) = db::design(db, id)? {
                s.push(serde_json::to_value(d).unwrap_or_default());
            }
        }
        // the customer's own designs (and its franchises'), newest first
        let mut mine: Vec<Value> = Vec::new();
        if let Some(c) = job.customer_id {
            let ids = customers::with_children(db, c)?;
            let marks = vec!["?"; ids.len()].join(",");
            let mut q = db.prepare(&format!(
                "SELECT DISTINCT d.id FROM designs d JOIN design_customers l ON l.design_id = d.id
                 WHERE l.customer_id IN ({marks}) AND d.stitch_file IS NOT NULL AND d.archived = 0
                 ORDER BY d.updated DESC LIMIT 24"
            ))?;
            let found: Vec<i64> = q
                .query_map(rusqlite::params_from_iter(ids.iter()), |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for d in found {
                if let Some(d) = db::design(db, d)? {
                    mine.push(serde_json::to_value(d).unwrap_or_default());
                }
            }
        }
        Ok(Json(
            json!({ "suggestions": s, "customer_designs": mine, "customer": job.customer_label,
            "can_link": !job.design_source.is_empty() && job.source.is_some() }),
        ))
    })
    .await
}

#[derive(Deserialize)]
struct JobDesign {
    design_id: Option<i64>,
    /// One of the design's colourways; none for its main colours.
    #[serde(default)]
    colourway_id: Option<i64>,
    /// Also for every job of the same outside name (link the operation to the design).
    #[serde(default)]
    every: bool,
}

async fn set_job_design(
    State(st): St,
    _u: User,
    Path(id): Path<i64>,
    Json(b): Json<JobDesign>,
) -> R<Json<Value>> {
    blocking(&st, move |db, _| {
        let job = jobs::get(db, id)?.ok_or_else(ApiError::not_found)?;
        if let Some(d) = b.design_id {
            db::design(db, d)?.ok_or_else(|| ApiError::bad("No such design."))?;
        }
        let colourway = match (b.design_id, b.colourway_id) {
            (Some(d), Some(c)) => match colourways::get(db, c)? {
                Some((_, of)) if of == d => Some(c),
                _ => {
                    return Err(ApiError::bad(
                        "That colourway is not one of this design's.",
                    ));
                }
            },
            _ => None,
        };
        jobs::change(
            db,
            id,
            &jobs::Change {
                design_id: Some(b.design_id),
                colourway_id: Some(colourway),
                // a problem about the design is solved by choosing one
                status: (job.status == "problem" && b.design_id.is_some()).then(|| "waiting".to_string()),
                message: (job.status == "problem").then(String::new),
                ..Default::default()
            },
        )
        .map_err(ApiError::bad)?;
        let mut others = 0;
        if b.every {
            let conn: Option<i64> = db
                .query_row(
                    "SELECT connection_id FROM links WHERE entity = 'job' AND local_id = ?",
                    [id],
                    |r| r.get(0),
                )
                .ok();
            let (Some(conn), Some((model, ext))) = (conn, job.design_source.split_once('|')) else {
                return Err(ApiError::bad("This job did not come from a connection."));
            };
            let r = Remote {
                model: model.into(),
                id: ext.into(),
                label: job.design_hint.clone(),
            };
            connections::set_design_link(db, conn, &r, b.design_id, colourway)?;
            others = db.execute(
                "UPDATE jobs SET design_id = ?, colourway_id = ? WHERE design_source = ? AND id <> ? AND status IN ('waiting', 'problem')",
                params![b.design_id, colourway, job.design_source, id],
            )?;
        }
        Ok(Json(json!({ "job": jobs::get(db, id)?, "others": others })))
    })
    .await
}

/// Where a design is published (Odoo products), and the connections it could go to.
async fn design_published(State(st): St, _u: User, Path(id): Path<i64>) -> R<Json<Value>> {
    blocking(&st, move |db, _| {
        let conns: Vec<Value> = connections::list(db)?
            .into_iter()
            .filter(|c| c.kind == "odoo")
            .map(|c| json!({ "id": c.id, "name": c.name, "enabled": c.enabled }))
            .collect();
        let mut q = db.prepare(
            "SELECT connection_id, colourway_id, external_id, label FROM links
             WHERE entity = 'design' AND local_id = ? AND model = 'product.template'",
        )?;
        let products: Vec<Value> = q
            .query_map([id], |r| {
                Ok(json!({
                    "connection_id": r.get::<_, i64>(0)?,
                    "colourway_id": r.get::<_, Option<i64>>(1)?,
                    "id": r.get::<_, String>(2)?,
                    "label": r.get::<_, String>(3)?,
                }))
            })?
            .collect::<Result<_, _>>()?;
        Ok(Json(json!({ "connections": conns, "products": products })))
    })
    .await
}

#[derive(Deserialize)]
struct PublishDesign {
    /// Which connection; the first Odoo one when missing.
    connection_id: Option<i64>,
}

/// Put the design in Odoo as a product to choose in nomenclatures (a variant per colourway, with
/// the library's cost and price), or bring the one made before up to date.
async fn publish_design(
    State(st): St,
    _a: Admin,
    Path(id): Path<i64>,
    Json(b): Json<PublishDesign>,
) -> R<Json<Value>> {
    let conn = blocking(&st, move |db, _| {
        db::design(db, id)?.ok_or_else(ApiError::not_found)?;
        let all = connections::list(db)?;
        let c = match b.connection_id {
            Some(c) => all.into_iter().find(|x| x.id == c),
            None => all.into_iter().find(|x| x.kind == "odoo"),
        };
        c.map(|c| c.id)
            .ok_or_else(|| ApiError::bad("Connect to Odoo first (Jobs › Connections)."))
    })
    .await?;
    let r = away(&st, move |db, store| {
        connections::publish(db, store, conn, id)
    })
    .await?;
    let variants: Vec<Value> = r
        .variants
        .iter()
        .map(|(cw, v)| json!({ "colourway_id": cw, "label": v.label }))
        .collect();
    Ok(Json(
        json!({ "product": r.product.label, "variants": variants }),
    ))
}
