mod axum_htmx;

use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use axum_htmx::HtmxPostRequest;
use serde::Deserialize;
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize, Debug, PartialEq)]
enum Rating {
    Good,
    TooSoon,
    Bad,
}

#[derive(Deserialize, Debug)]
struct RatedCombo {
    rating: Rating,
    combo: WeightedCombo,
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

async fn load(State(pool): State<Arc<PgPool>>) -> impl IntoResponse {
    let combo = combo(pool).await;
    todo!()
}

// async fn rating(
//     State(pool): State<Arc<PgPool>>,
//     rated: HtmxPostRequest<Option<RatedCombo>>,
// ) -> impl IntoResponse {
//     // 0. user_id and days
//     let (user_id, days) = user_id_and_days().await;
//     // 1. `spawn` shorts
//     let shorts_pool = Arc::clone(&pool);
//     let shorts =
//         tokio::spawn(async move { clothes(shorts_pool.as_ref(), "shorts", user_id, days).await });
//     // 2. `await` shirts
//     let Ok(shirts) = clothes(pool.as_ref(), "shirts", user_id, days).await else {
//         todo!()
//     };
//
//     // 3. `await` shirt
//     let Ok(shirt) = shirt(pool.as_ref(), shirts).await else {
//         todo!()
//     };
//
//     // 4. `await` combos
//     let Ok(combos) = combos(pool.as_ref(), shirt.id).await else {
//         todo!()
//     };
//
//     // 5. `await` shorts `await` short
//     let Ok(shorts) = shorts.await else {
//         todo!()
//     };
//     todo!()
// }

async fn index() -> impl IntoResponse {
    Html(include_str!("../index.html"))
}

async fn all_combos(pool: &PgPool) -> Vec<WeightedCombo> {
    sqlx::query_as(
        r#"
            SELECT shirt_id, short_id, weight FROM combos
            "#,
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn all_combos_handler(State(pool): State<Arc<PgPool>>) -> impl IntoResponse {
    let mut string = String::with_capacity(100);
    for combo in all_combos(pool.as_ref()).await {
        string.push_str(&format!("{combo:?}"))
    }
    string
}

#[shuttle_runtime::main]
async fn axum(
    #[shuttle_secrets::Secrets] secret_store: shuttle_secrets::SecretStore,
) -> shuttle_axum::ShuttleAxum {
    let pool = PgPool::connect(&secret_store.get("DB_URL").expect("url"))
        .await
        .expect("sad postgres");

    let app = Router::new()
        .route("/", get(index))
        .route("/load", get(load))
        // .route("/rating", post(rating))
        .route("/all_combos", get(all_combos_handler))
        .with_state(Arc::new(pool));

    Ok(app.into())
}
