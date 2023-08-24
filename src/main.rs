use axum::{extract::Form, response::Html, routing::get, Router};
use serde::Deserialize;

#[shuttle_runtime::main]
async fn axum() -> shuttle_axum::ShuttleAxum {
    // build our application with some routes
    let app = Router::new().route("/", get(show_form).post(accept_form));

    Ok(app.into())
}

async fn show_form() -> Html<&'static str> {
    Html(
        r#"
        <!doctype html>
        <html>
            <head></head>
            <body>
                <form action="/" method="post">
                    <label for="name">
                        Enter your name:
                        <input type="text" name="name">
                    </label>

                    <label>
                        Enter your email:
                        <input type="text" name="email">
                    </label>

                    <input type="submit" value="Subscribe!">
                </form>
            </body>
        </html>
        "#,
    )
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct Input {
    name: String,
    email: String,
}

async fn accept_form(Form(input): Form<Input>) {
    dbg!(&input);
}
