use axum::{routing::get, Json, Router};
use fontconfig::{list_fonts, Fontconfig, Pattern};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/fonts", get(fonts))
}

async fn fonts() -> Json<Vec<String>> {
    let Some(fontconfig) = Fontconfig::new() else {
        return Json(Vec::new());
    };

    let mut families = list_fonts(&Pattern::new(&fontconfig), None)
        .iter()
        .filter_map(|font| font.name().map(str::to_string))
        .collect::<Vec<_>>();
    families.sort_unstable();
    families.dedup();
    Json(families)
}
