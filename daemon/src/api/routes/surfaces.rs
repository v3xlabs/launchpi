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
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use crate::{
    api::error::ApiError,
    drivers::streamdeck::{
        model::{model_by_name, STREAM_DECK_NETWORK_DOCK, STREAM_DECK_STUDIO},
        studio,
    },
    identifiers::{PanelId, SurfaceId},
    panels::{
        control::Control, dial::PanelDial, rendered_state::RenderedState, Panel, PanelLayout,
    },
    rendering::context::RenderContext,
    state::AppState,
    surfaces::{
        command::KeyRendering,
        defaults::studio_capabilities,
        inventory::DeviceInventory,
        layout::{SurfaceCapabilities, SurfaceLayout},
        managed::{
            AddNetworkSurface, ManagedNetworkSurface, NetworkSurfaceStatus, UpdateNetworkSurface,
        },
    },
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct PanelRequest {
    name: String,
    layout: PanelLayout,
    #[serde(default)]
    capabilities: SurfaceCapabilities,
    controls: Vec<Control>,
    #[serde(default)]
    dials: Vec<PanelDial>,
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
            "/api/devices/:surface_id/presentation",
            get(device_presentation),
        )
        .route(
            "/api/discovered/:discovery_id/devices",
            post(add_discovered_device),
        )
        .route("/api/events", get(events))
        .route("/api/render-key", post(render_key))
        .route("/api/panels", get(list_panels).post(create_panel))
        .route(
            "/api/panels/:panel_id",
            patch(update_panel).delete(delete_panel),
        )
        .route(
            "/api/panels/:panel_id/config",
            get(export_panel_configuration),
        )
        .route("/api/config", post(save_configuration))
}

async fn list_devices(State(state): State<AppState>) -> Json<DeviceInventory> {
    let mut inventory = state.surfaces.inventory();
    inventory.plugin_instances = state.plugins.instances();
    Json(inventory)
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

#[derive(Deserialize)]
struct RenderKeyRequest {
    default_state: RenderedState,
    #[serde(default)]
    pressed_state: Option<RenderedState>,
    #[serde(default)]
    is_pressed: bool,
}

/// Draws a control exactly as a device would.
///
/// The browser sends the control's *unresolved* state, bindings intact, and the daemon resolves it
/// here through the same code that feeds the hardware. That keeps one implementation of what a
/// binding means, while still letting the editor preview a draft that has never been saved.
async fn render_key(
    State(state): State<AppState>,
    Json(request): Json<RenderKeyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let variables = state.surfaces.variables();
    let resolved = RenderContext::new(&variables).resolve_states(
        &request.default_state,
        request.pressed_state.as_ref(),
        request.is_pressed,
    );
    let rendering = KeyRendering {
        key_index: 0,
        layers: resolved.layers,
        is_dimmed: false,
    };
    let image = studio::render_key(&rendering, Some(&state.assets)).map_err(|error| ApiError {
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
    let model = request.kind.model();
    let surface = ManagedNetworkSurface {
        surface_id: state.surfaces.create_surface_id(),
        name: non_empty(request.name, "name")?,
        host: non_empty(request.host, "host")?,
        port: request.port.unwrap_or_else(studio::default_port),
        serial_number: request.serial_number,
        model: model.name.to_string(),
        layout: model.layout,
        capabilities: if is_network_dock {
            SurfaceCapabilities::default()
        } else {
            studio_capabilities()
        },
        active_panel_id: panel_for_layout(&state, model.layout),
        open_subpanels: Vec::new(),
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
    if let Some(existing) = state
        .surfaces
        .managed_by_endpoint(&discovered.host, discovered.port)
    {
        return Ok(Json(existing));
    }
    // Discovery only advertises a model name. Anything it does not name is assumed to be a Studio,
    // which is what it was before probing existed; connecting corrects the identity either way.
    let model = model_by_name(&discovered.model).unwrap_or(&STREAM_DECK_STUDIO);
    let is_network_dock = model.name == STREAM_DECK_NETWORK_DOCK.name;
    let surface = ManagedNetworkSurface {
        surface_id: state.surfaces.create_surface_id(),
        name: discovered.name,
        host: discovered.host,
        port: discovered.port,
        serial_number: discovered.serial_number,
        model: discovered.model,
        layout: model.layout,
        capabilities: if is_network_dock {
            SurfaceCapabilities::default()
        } else {
            studio_capabilities()
        },
        active_panel_id: panel_for_layout(&state, model.layout),
        open_subpanels: Vec::new(),
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

async fn device_presentation(
    State(state): State<AppState>,
    Path(surface_id): Path<String>,
) -> Result<Json<crate::surfaces::presentation::SurfacePresentation>, ApiError> {
    state
        .surfaces
        .presentation(&SurfaceId(surface_id))
        .map(Json)
        .ok_or_else(|| ApiError::not_found("device presentation"))
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
        dials: request.dials,
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
        dials: request.dials,
    };
    let panel = state
        .surfaces
        .upsert_panel(panel)
        .map_err(ApiError::bad_request)?;
    let _ = state.persist_configuration();
    Ok(Json(panel))
}
/// Devices running the panel fall back to another compatible panel, or to nothing at all when none
/// remains.
async fn delete_panel(
    State(state): State<AppState>,
    Path(panel_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .surfaces
        .remove_panel(&panel_id)
        .ok_or_else(|| ApiError::not_found("panel"))?;
    let _ = state.persist_configuration();
    Ok(StatusCode::NO_CONTENT)
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

/// A panel already configured for exactly this grid, without provisioning one.
fn panel_for_layout(state: &AppState, layout: SurfaceLayout) -> Option<PanelId> {
    let SurfaceLayout::Grid { columns, rows } = layout else {
        return None;
    };
    state
        .surfaces
        .panels()
        .into_iter()
        .find(|panel| panel.layout.columns == columns && panel.layout.rows == rows)
        .map(|panel| panel.panel_id)
}

fn non_empty(value: String, field_name: &str) -> Result<String, ApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(ApiError::bad_request(format!("{field_name} is required")))
    } else {
        Ok(value)
    }
}
