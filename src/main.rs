mod axum_htmx;

use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Redirect},
    routing::post,
    Json,
};
use axum::{handler::HandlerWithoutStateExt, http::StatusCode, routing::get, Router};
use axum_htmx::HtmxPostRequest;
use serde::Deserialize;
use serde::Serialize;
use shuttle_runtime::tracing::info;
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

#[derive(Deserialize)]
struct RatedCombo {
    rating: Rating,
    shirt_color: i32,
    short_color: i32,
}

#[derive(Template)]
#[template(path = "combo.html")]
struct Combo {
    shirt_color: i32,
    short_color: i32,
}

#[derive(FromRow, Debug)]
struct WeightedShort {
    short_color: i32,
    weight: f32,
}

#[derive(FromRow, Serialize, Clone, Debug)]
struct ClotheColor {
    color: i32,
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error(transparent)]
    Render(#[from] askama::Error),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error("failed to get user id and days")]
    UserIdAndDays,
    #[error("failed to find any {0}")]
    EmptyClothes(&'static str),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Uuid(#[from] uuid::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        use Error::*;
        match self {
            Render(_) | Sqlx(_) | Join(_) | Uuid(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
            }
            UserIdAndDays => Redirect::permanent("/login").into_response(),
            EmptyClothes(_) => Redirect::permanent("/closet").into_response(),
        }
    }
}

async fn user_id_and_days() -> Option<(Uuid, i32)> {
    // TODO: get user from cookies and days from redis
    Some((
        Uuid::parse_str("9ab4649c-9a8e-4fc0-b696-a598327f2a70").ok()?,
        5,
    ))
}

async fn shirts(pool: &PgPool, user_id: Uuid, days: i32) -> sqlx::Result<Vec<ClotheColor>> {
    sqlx::query_as(
        r#"
        SELECT
            color
        FROM
            shirts
        WHERE
            user_id = $1
            AND last_use < NOW() - INTERVAL '$1 days'
        "#,
    )
    .bind(user_id)
    .bind(days)
    .fetch_all(pool)
    .await
}

async fn shorts(pool: &PgPool, user_id: Uuid, days: i32) -> sqlx::Result<Vec<ClotheColor>> {
    sqlx::query_as(
        r#"
        SELECT
            color
        FROM
            shorts
        WHERE
            user_id = $1
            AND last_use < NOW() - INTERVAL '$1 days'
        "#,
    )
    .bind(user_id)
    .bind(days)
    .fetch_all(pool)
    .await
}

async fn shirt(shirts: Vec<ClotheColor>) -> Option<ClotheColor> {
    use rand::seq::SliceRandom;
    shirts.choose(&mut rand::thread_rng()).map(|x| x.clone())
}

async fn combos(
    pool: &PgPool,
    user_id: Uuid,
    ClotheColor { color }: ClotheColor,
) -> sqlx::Result<Vec<WeightedShort>> {
    sqlx::query_as(
        r#"
        SELECT 
            short_color,
            weight
        FROM 
            combos 
        WHERE 
            short_user_id = $1
            AND shirt_color = $2
        "#,
    )
    .bind(user_id)
    .bind(color)
    .fetch_all(pool)
    .await
}

async fn combo(pool: Arc<PgPool>) -> Result<impl IntoResponse, Error> {
    // // 0. user_id and days
    let (user_id, days) = user_id_and_days()
        .await
        .ok_or_else(|| Error::UserIdAndDays)?;

    // 1. `spawn` shorts
    let shorts_pool = Arc::clone(&pool);
    let shorts = tokio::spawn(async move { shorts(shorts_pool.as_ref(), user_id, days).await });

    // // 2. `await` shirts
    let shirts = shirts(pool.as_ref(), user_id, days).await?;
    info!(?shirts);

    // 3. `await` shirt
    let shirt = shirt(shirts)
        .await
        .ok_or_else(|| Error::EmptyClothes("shirts"))?;
    info!(?shirt);

    // 4. `await` combos
    let combos = combos(pool.as_ref(), user_id, shirt.clone()).await?;
    info!(?combos);

    // 5. `await` shorts `await` short
    let shorts = shorts.await??;
    info!(?shorts);

    // the crossover point of linnear seach and a hash map should be crossed
    let apply_weight = |short_color: i32| WeightedShort {
        short_color,
        weight: combos
            .iter()
            .find(|x| x.short_color == short_color)
            .map(|x| x.weight)
            .unwrap_or(1.0),
    };

    use rand::distributions::WeightedIndex;
    use rand::prelude::*;

    let dist = WeightedIndex::new(
        shorts
            .iter()
            .map(|ClotheColor { color }| apply_weight(*color))
            .map(|WeightedShort { weight, .. }| weight),
    )
    .unwrap();
    info!(?dist);

    let short = shorts[dist.sample(&mut thread_rng())].clone();
    info!(?short);

    Ok(Html(
        Combo {
            short_color: short.color,
            shirt_color: shirt.color,
        }
        .render()?,
    ))
}

async fn load(State(pool): State<Arc<PgPool>>) -> impl IntoResponse {
    combo(pool).await
}

async fn rating(
    State(pool): State<Arc<PgPool>>,
    HtmxPostRequest(RatedCombo {
        shirt_color,
        short_color,
        rating,
    }): HtmxPostRequest<RatedCombo>,
) -> impl IntoResponse {
    info!(?rating, ?shirt_color, ?short_color);
    // TODO: use raiting
    combo(pool).await
}

#[shuttle_runtime::main]
async fn axum(
    #[shuttle_secrets::Secrets] secret_store: shuttle_secrets::SecretStore,
    #[shuttle_static_folder::StaticFolder] static_folder: PathBuf,
) -> shuttle_axum::ShuttleAxum {
    let pool = PgPool::connect(
        &secret_store
            .get("DB_URL")
            .ok_or_else(|| shuttle_runtime::Error::BuildPanic("Missing DB_URL".to_string()))?,
    )
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
