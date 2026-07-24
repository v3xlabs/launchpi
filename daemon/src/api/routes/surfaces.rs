use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;

use crate::{
    models::{
        control::Control,
        identifiers::PanelId,
        network_surface::{
            AddNetworkSurface, DeviceInventory, KeyRendering, ManagedNetworkSurface,
            NetworkSurfaceStatus, UpdateNetworkSurface,
        },
        panel::{Panel, PanelLayout},
        surface::SurfaceCapabilities,
    },
    state::{studio_capabilities, AppState},
    streamdeck::studio,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct PanelRequest {
    name: String,
    layout: PanelLayout,
    #[serde(default)]
    capabilities: SurfaceCapabilities,
    controls: Vec<Control>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct AssignPanelRequest {
    panel_id: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/devices", get(list_devices).post(add_device))
        .route(
            "/api/devices/:surface_id",
            patch(update_device).delete(remove_device),
        )
        .route(
            "/api/devices/:surface_id/active-panel",
            put(assign_active_panel),
        )
        .route(
            "/api/discovered/:discovery_id/devices",
            post(add_discovered_device),
        )
        .route("/api/events", get(events))
        .route("/api/render-key", post(render_key))
        .route("/api/panels", get(list_panels).post(create_panel))
        .route("/api/panels/:panel_id", patch(update_panel))
        .route(
            "/api/panels/:panel_id/config",
            get(export_panel_configuration),
        )
        .route("/api/config", post(save_configuration))
}

async fn list_devices(State(state): State<AppState>) -> Json<DeviceInventory> {
    Json(state.surfaces.inventory())
}

async fn events(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| stream_events(socket, state))
}

async fn stream_events(mut socket: WebSocket, state: AppState) {
    let mut receiver = state.surfaces.subscribe();
    loop {
        tokio::select! {
            event = receiver.recv() => match event {
                Ok(event) => {
                    let Ok(payload) = serde_json::to_string(&event) else {
                        continue;
                    };
                    if socket.send(Message::Text(payload)).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(_)) => {}
                _ => break,
            },
        }
    }
}
async fn list_panels(State(state): State<AppState>) -> Json<Vec<Panel>> {
    Json(state.surfaces.panels())
}

async fn render_key(Json(rendering): Json<KeyRendering>) -> Result<impl IntoResponse, ApiError> {
    let image = studio::render_key(&rendering).map_err(|error| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: error,
    })?;
    Ok(([(axum::http::header::CONTENT_TYPE, "image/jpeg")], image))
}

async fn add_device(
    State(state): State<AppState>,
    Json(request): Json<AddNetworkSurface>,
) -> Result<Json<ManagedNetworkSurface>, ApiError> {
    let is_network_dock = request.kind.is_network_dock();
    let surface = ManagedNetworkSurface {
        surface_id: state.surfaces.create_surface_id(),
        name: non_empty(request.name, "name")?,
        host: non_empty(request.host, "host")?,
        port: request.port.unwrap_or_else(studio::default_port),
        serial_number: request.serial_number,
        model: request.kind.model_name().to_string(),
        layout: if is_network_dock {
            crate::models::surface::SurfaceLayout::Freeform
        } else {
            crate::models::surface::SurfaceLayout::Grid {
                columns: 16,
                rows: 2,
            }
        },
        capabilities: if is_network_dock {
            SurfaceCapabilities::default()
        } else {
            studio_capabilities()
        },
        active_panel_id: (!is_network_dock)
            .then(|| {
                state
                    .surfaces
                    .panels()
                    .into_iter()
                    .find(|panel| panel.layout.columns == 16 && panel.layout.rows == 2)
                    .map(|panel| panel.panel_id)
            })
            .flatten(),
        is_enabled: true,
        parent_surface_id: None,
        status: NetworkSurfaceStatus::Connecting,
        last_error: None,
    };
    let surface = state.surfaces.add_managed(surface);
    let _ = state.persist_configuration();
    studio::start_connection_monitor(state, surface.clone());
    Ok(Json(surface))
}

async fn add_discovered_device(
    State(state): State<AppState>,
    Path(discovery_id): Path<String>,
) -> Result<Json<ManagedNetworkSurface>, ApiError> {
    let discovered = state
        .surfaces
        .discovered(&discovery_id)
        .ok_or_else(|| ApiError::not_found("discovered Stream Deck Studio"))?;
    let is_network_dock = discovered.model == "Stream Deck Network Dock";
    let surface = ManagedNetworkSurface {
        surface_id: state.surfaces.create_surface_id(),
        name: discovered.name,
        host: discovered.host,
        port: discovered.port,
        serial_number: discovered.serial_number,
        model: discovered.model,
        layout: if is_network_dock {
            crate::models::surface::SurfaceLayout::Freeform
        } else {
            crate::models::surface::SurfaceLayout::Grid {
                columns: 16,
                rows: 2,
            }
        },
        capabilities: if is_network_dock {
            SurfaceCapabilities::default()
        } else {
            studio_capabilities()
        },
        active_panel_id: (!is_network_dock)
            .then(|| {
                state
                    .surfaces
                    .panels()
                    .into_iter()
                    .find(|panel| panel.layout.columns == 16 && panel.layout.rows == 2)
                    .map(|panel| panel.panel_id)
            })
            .flatten(),
        is_enabled: true,
        parent_surface_id: None,
        status: NetworkSurfaceStatus::Connecting,
        last_error: None,
    };
    let surface = state.surfaces.add_managed(surface);
    let _ = state.persist_configuration();
    studio::start_connection_monitor(state, surface.clone());
    Ok(Json(surface))
}

async fn update_device(
    State(state): State<AppState>,
    Path(surface_id): Path<String>,
    Json(request): Json<UpdateNetworkSurface>,
) -> Result<Json<ManagedNetworkSurface>, ApiError> {
    let surface = state
        .surfaces
        .set_enabled(&surface_id, request.is_enabled)
        .ok_or_else(|| ApiError::not_found("managed device"))?;
    let _ = state.persist_configuration();
    if request.is_enabled {
        studio::start_connection_monitor(state, surface.clone());
    } else {
        state.surfaces.deactivate(&surface_id);
    }
    Ok(Json(surface))
}
async fn remove_device(
    State(state): State<AppState>,
    Path(surface_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .surfaces
        .remove_managed(&surface_id)
        .ok_or_else(|| ApiError::not_found("managed device"))?;
    let _ = state.persist_configuration();
    Ok(StatusCode::NO_CONTENT)
}
async fn assign_active_panel(
    State(state): State<AppState>,
    Path(surface_id): Path<String>,
    Json(request): Json<AssignPanelRequest>,
) -> Result<Json<ManagedNetworkSurface>, ApiError> {
    let device = state
        .surfaces
        .assign_active_panel(&surface_id, &request.panel_id)
        .map_err(ApiError::bad_request)?;
    let _ = state.persist_configuration();
    Ok(Json(device))
}

async fn create_panel(
    State(state): State<AppState>,
    Json(request): Json<PanelRequest>,
) -> Result<Json<Panel>, ApiError> {
    let panel = Panel {
        panel_id: state.surfaces.create_panel_id(),
        name: non_empty(request.name, "name")?,
        layout: request.layout,
        capabilities: request.capabilities,
        controls: request.controls,
    };
    let panel = state
        .surfaces
        .upsert_panel(panel)
        .map_err(ApiError::bad_request)?;
    let _ = state.persist_configuration();
    Ok(Json(panel))
}
async fn update_panel(
    State(state): State<AppState>,
    Path(panel_id): Path<String>,
    Json(request): Json<PanelRequest>,
) -> Result<Json<Panel>, ApiError> {
    if state.surfaces.panel(&panel_id).is_none() {
        return Err(ApiError::not_found("panel"));
    }
    let panel = Panel {
        panel_id: PanelId(panel_id),
        name: non_empty(request.name, "name")?,
        layout: request.layout,
        capabilities: request.capabilities,
        controls: request.controls,
    };
    let panel = state
        .surfaces
        .upsert_panel(panel)
        .map_err(ApiError::bad_request)?;
    let _ = state.persist_configuration();
    Ok(Json(panel))
}
async fn export_panel_configuration(
    State(state): State<AppState>,
    Path(panel_id): Path<String>,
) -> Result<String, ApiError> {
    state
        .export_panel_configuration(&panel_id)
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("panel"))
}
async fn save_configuration(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    state.persist_configuration().map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

fn non_empty(value: String, field_name: &str) -> Result<String, ApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(ApiError::bad_request(format!("{field_name} is required")))
    } else {
        Ok(value)
    }
}
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}
struct ApiError {
    status: StatusCode,
    message: String,
}
impl ApiError {
    fn bad_request(message: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message,
        }
    }
    fn not_found(resource: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: format!("{resource} was not found"),
        }
    }
    fn internal(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}
