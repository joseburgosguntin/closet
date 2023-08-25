mod axum_htmx;

use axum::{
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use axum_htmx::{HtmxPostRequest, HtmxResponse};
use serde::Deserialize;
use silkenweb::prelude::{html::div, ParentElement};

async fn index() -> impl IntoResponse {
    Html(include_str!("../index.html"))
}

#[derive(Deserialize)]
struct Name {
    first: String,
    last: String,
}

async fn form_submit(
    HtmxPostRequest(Name { first, last }): HtmxPostRequest<Name>,
) -> impl IntoResponse {
    HtmxResponse::new(div().text(format!("Hello, {} {}!", first, last)))
}

#[shuttle_runtime::main]
async fn axum() -> shuttle_axum::ShuttleAxum {
    let app = Router::new()
        .route("/", get(index))
        .route("/form-submit", post(form_submit));

    Ok(app.into())
}
