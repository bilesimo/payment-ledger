use axum::{Router, response::Html, routing::get};

const INDEX_HTML: &str = include_str!("index.html");

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/", get(index))
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}
