// Protected photo access and original-image delivery.
use axum::{extract::{Path as AxumPath, State}, http::{header, HeaderValue, StatusCode}, response::IntoResponse, Json};
use crate::{config::Config, handlers::video::user_has_view_access, sessions};
use sqlx::PgPool;
use tower_cookies::Cookies;

#[derive(Clone)]
pub struct PhotoState {
    pub pool: PgPool,
    pub cfg: Config,
}

async fn photo_filename(pool: &PgPool, photo_id: &str) -> Option<String> {
    sqlx::query_scalar(
        "SELECT filename FROM videos WHERE id=$1 AND COALESCE(media_type,'video')='photo' LIMIT 1"
    ).bind(photo_id).fetch_optional(pool).await.ok().flatten()
}

async fn can_view(st: &PhotoState, cookies: &Cookies, photo_id: &str) -> Result<(), (StatusCode, &'static str)> {
    let (user_id, _) = sessions::current_user_id(&st.pool, &st.cfg, cookies).await
        .ok_or((StatusCode::UNAUTHORIZED, "not logged in"))?;
    match user_has_view_access(&st.pool, photo_id, &user_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err((StatusCode::FORBIDDEN, "no access")),
        Err(_) => Err((StatusCode::INTERNAL_SERVER_ERROR, "access check failed")),
    }
}

pub async fn photo_access(
    State(st): State<PhotoState>,
    cookies: Cookies,
    AxumPath(photo_id): AxumPath<String>,
) -> impl IntoResponse {
    if photo_filename(&st.pool, &photo_id).await.is_none() {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"ok": false}))).into_response();
    }
    match can_view(&st, &cookies, &photo_id).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true, "access": true}))).into_response(),
        Err((StatusCode::FORBIDDEN, _)) => (StatusCode::OK, Json(serde_json::json!({"ok": true, "access": false}))).into_response(),
        Err((status, _)) => (status, Json(serde_json::json!({"ok": false}))).into_response(),
    }
}

pub async fn serve_photo(
    State(st): State<PhotoState>,
    cookies: Cookies,
    AxumPath(photo_id): AxumPath<String>,
) -> impl IntoResponse {
    if let Err((status, message)) = can_view(&st, &cookies, &photo_id).await {
        return (status, message).into_response();
    }
    let Some(filename) = photo_filename(&st.pool, &photo_id).await else {
        return (StatusCode::NOT_FOUND, "photo not found").into_response();
    };
    if filename.is_empty() || filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return (StatusCode::FORBIDDEN, "invalid photo path").into_response();
    }
    let root = if st.cfg.upload_dir.is_empty() { &st.cfg.storage_dir } else { &st.cfg.upload_dir };
    let path = std::path::Path::new(root).join(&filename);
    let bytes = match tokio::fs::read(&path).await {
        Ok(data) => data,
        Err(_) => return (StatusCode::NOT_FOUND, "photo file missing").into_response(),
    };
    if bytes.len() > 40 * 1024 * 1024 {
        return (StatusCode::PAYLOAD_TOO_LARGE, "photo exceeds delivery limit").into_response();
    }
    let mime = match std::path::Path::new(&filename).extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "avif" => "image/avif",
        _ => "application/octet-stream",
    };
    let mut response = (StatusCode::OK, bytes).into_response();
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}
