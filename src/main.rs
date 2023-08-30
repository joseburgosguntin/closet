mod axum_htmx;

use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::post,
};
use axum::{handler::HandlerWithoutStateExt, http::StatusCode, routing::get, Router};
use axum_htmx::HtmxPostRequest;
use serde::Deserialize;
use sqlx::{FromRow, PgPool};
use std::{fmt::Display, path::PathBuf, sync::Arc};
use tower_http::services::ServeDir;
use uuid::Uuid;

#[derive(Deserialize, Debug, PartialEq)]
enum Rating {
    Good,
    TooSoon,
    Bad,
}

impl Display for Rating {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Deserialize, Debug, Template)]
#[template(path = "combo.html")]
struct RatedCombo {
    rating: Rating,
    shirt_id: i64,
    short_id: i64,
    weight: f64,
}

#[derive(Debug, Clone, Deserialize, FromRow)]
struct WeightedCombo {
    shirt_id: i64,
    short_id: i64,
    weight: f64,
}

#[derive(Debug, FromRow)]
struct SvgWithId {
    id: i64,
    svg: String,
}

#[derive(FromRow)]
struct ClotheId {
    id: i64,
}

async fn user_id_and_days() -> (Uuid, u32) {
    todo!("get user from cookies and days from redis")
}

async fn clothes(
    pool: &PgPool,
    clothes: &'static str,
    user_id: Uuid,
    days: u32,
) -> sqlx::Result<Vec<ClotheId>> {
    sqlx::query_as(
        r#"
        SELECT id FROM ? WHERE user_id = ? AND last_use < now() - INTERVAL ?
        "#,
    )
    .bind(clothes)
    .bind(user_id)
    .bind(format!("{days} days"))
    .fetch_all(pool)
    .await
}

async fn shirt(pool: &PgPool, shirts: Vec<ClotheId>) -> sqlx::Result<SvgWithId> {
    use rand::seq::SliceRandom;
    let Some(id) = shirts.choose(&mut rand::thread_rng()).map(|x| x.id) else {
        todo!()
    };

    sqlx::query_as(
        r#"
        SELECT svg FROM shirts WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
}

async fn combos(pool: &PgPool, shirt_id: i64) -> sqlx::Result<Vec<WeightedCombo>> {
    sqlx::query_as(
        r#"
        SELECT short_id FROM combos WHERE shirt_id = ?
        "#,
    )
    .bind(shirt_id)
    .fetch_all(pool)
    .await
}

async fn combo(pool: Arc<PgPool>) -> sqlx::Result<SvgWithId> {
    // 0. user_id and days
    let (user_id, days) = user_id_and_days().await;
    // 1. `spawn` shorts
    let shorts_pool = Arc::clone(&pool);
    let shorts =
        tokio::spawn(async move { clothes(shorts_pool.as_ref(), "shorts", user_id, days).await });
    // 2. `await` shirts
    let shirts = clothes(pool.as_ref(), "shirts", user_id, days).await?;

    // 3. `await` shirt
    let shirt = shirt(pool.as_ref(), shirts).await?;

    // 4. `await` combos
    let combos = combos(pool.as_ref(), shirt.id).await?;

    // 5. `await` shorts `await` short
    let shorts = shorts.await.map_err(|_| sqlx::Error::WorkerCrashed)?;
    todo!()
}

async fn rating(
    State(pool): State<Arc<PgPool>>,
    HtmxPostRequest(rated): HtmxPostRequest<RatedCombo>,
) -> impl IntoResponse {
    let combo = combo(pool);
    Html(rated.render().unwrap())
    // Html(format!(
    //     r#"
    //     <h1> rating {rated:?} </h1>
    //     <input name="shirt_id" type="hidden" value="2"/>
    //     <input name="short_id" type="hidden" value="2"/>
    //     <input name="weight" type="hidden" value="1.0"/>
    //     "#,
    // ))
}

async fn load(State(pool): State<Arc<PgPool>>) -> impl IntoResponse {
    let combo = combo(pool);
    Html(
        r#"
        <h1> load </h1>
        <input name="shirt_id" type="hidden" value="2"/>
        <input name="short_id" type="hidden" value="2"/>
        <input name="weight" type="hidden" value="1.0"/>
        "#,
    )
}

#[shuttle_runtime::main]
async fn axum(
    #[shuttle_secrets::Secrets] secret_store: shuttle_secrets::SecretStore,
    #[shuttle_static_folder::StaticFolder] static_folder: PathBuf,
) -> shuttle_axum::ShuttleAxum {
    let pool = PgPool::connect(&secret_store.get("DB_URL").expect("url"))
        .await
        .map_err(|e| shuttle_runtime::Error::Database(e.to_string()))?;

    async fn handle_404() -> (StatusCode, &'static str) {
        (StatusCode::NOT_FOUND, "Not found")
    }

    let service = handle_404.into_service();

    let serve_dir = ServeDir::new(static_folder).not_found_service(service);

    let app = Router::new()
        .route("/load", get(load))
        .route("/rating", post(rating))
        .with_state(Arc::new(pool))
        .fallback_service(serve_dir);

    Ok(app.into())
}
