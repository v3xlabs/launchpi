use axum::{
    http::{header, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../web/dist/"]
struct Assets;

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let asset = if path.is_empty() { "index.html" } else { path };

    if asset.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some((served_asset, content)) =
        Assets::get(asset)
            .map(|content| (asset, content))
            .or_else(|| {
                if asset.contains('.') {
                    None
                } else {
                    Assets::get("index.html").map(|content| ("index.html", content))
                }
            })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let content_type = mime_guess::from_path(served_asset)
        .first_or_octet_stream()
        .essence_str()
        .parse::<HeaderValue>()
        .expect("MIME types are valid HTTP header values");

    (
        [(header::CONTENT_TYPE, content_type)],
        content.data.into_owned().to_vec(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_spa_route_serves_html() {
        let response = serve("/devices".parse().unwrap()).await;

        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html"
        );
    }
}
