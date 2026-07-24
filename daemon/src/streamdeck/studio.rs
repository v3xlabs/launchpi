use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use ab_glyph::{point, Font, FontRef, Glyph, PxScale, ScaleFont};
use jpeg_encoder::{ColorType, Encoder};
use lazy_static::lazy_static;
use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{interval, sleep, timeout},
};
use tracing::{debug, info};

use crate::{
    models::{
        identifiers::SurfaceId,
        network_surface::{
            DiscoveredNetworkSurface, KeyIcon, KeyRendering, ManagedNetworkSurface,
            NetworkSurfaceStatus, SurfaceCommand,
        },
        rendered_state::RgbaColor,
        surface::SurfaceLayout,
    },
    state::AppState,
};

const ELGATO_VENDOR_ID: u16 = 0x0fd9;
const STREAM_DECK_STUDIO_PRODUCT_ID: u16 = 0x00aa;
const NETWORK_DOCK_DEVICE_TYPE: u16 = 215;
const NETWORK_DOCK_PRODUCT_ID: u16 = 0xffff;
const DEFAULT_STREAM_DECK_PORT: u16 = 5343;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RECONNECT_DELAY: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(8);
const LEGACY_PACKET_SIZE: usize = 512;
const LEGACY_RESPONSE_SIZE: usize = 1024;
const CORA_MAGIC: [u8; 4] = [0x43, 0x93, 0x8a, 0x41];
const CORA_HEADER_SIZE: usize = 16;
const MAX_CORA_PAYLOAD_SIZE: usize = 1024 * 1024;
const CORA_ACK_NAK: u16 = 0x0200;
const CORA_VERBATIM: u16 = 0x8000;
const CORA_WRITE: u8 = 0x00;
const CORA_GET_REPORT: u8 = 0x02;
const CORA_PRIMARY_INFO_MESSAGE_ID: u32 = 1;
const CORA_CHILD_INFO_MESSAGE_ID: u32 = 4;
const STUDIO_KEY_IMAGE_SIZE: usize = 96;
const IMAGE_REPORT_HEADER_SIZE: usize = 8;
const KEY_TEXT_PADDING: f32 = 6.0;
const KEY_TEXT_MAX_PX: f32 = 40.0;
const KEY_TEXT_MIN_PX: f32 = 8.0;

const KEY_FONT_BYTES: &[u8] = include_bytes!("../../assets/DejaVuSans.ttf");

lazy_static! {
    static ref KEY_FONT: FontRef<'static> =
        FontRef::try_from_slice(KEY_FONT_BYTES).expect("embedded key font is valid");
}

#[derive(Clone, Copy)]
enum TransportMode {
    Cora,
    Legacy,
}

pub fn start_discovery(state: AppState) -> Result<(), mdns_sd::Error> {
    let daemon = ServiceDaemon::new()?;
    let receiver = daemon.browse("_elg._tcp.local.")?;

    std::thread::spawn(move || {
        let _daemon = daemon;

        while let Ok(event) = receiver.recv() {
            match event {
                ServiceEvent::ServiceResolved(service) => {
                    if let Some(surface) = discovered_surface(&service) {
                        info!(
                            name = surface.name,
                            host = surface.host,
                            "discovered Stream Deck Studio"
                        );
                        state.surfaces.upsert_discovered(surface);
                    }
                }
                ServiceEvent::ServiceRemoved(_, discovery_id) => {
                    state.surfaces.remove_discovered(&discovery_id);
                }
                _ => {}
            }
        }
    });

    Ok(())
}

pub fn start_connection_monitor(state: AppState, surface: ManagedNetworkSurface) {
    let (is_active, mut commands) = state.surfaces.activate(&surface.surface_id);

    tokio::spawn(async move {
        while is_active.load(Ordering::Acquire) {
            state.update_status(&surface.surface_id, NetworkSurfaceStatus::Connecting, None);

            match timeout(
                CONNECT_TIMEOUT,
                TcpStream::connect((surface.host.as_str(), surface.port)),
            )
            .await
            {
                Ok(Ok(stream)) => {
                    if let Err(error) = stream.set_nodelay(true) {
                        state.update_status(
                            &surface.surface_id,
                            NetworkSurfaceStatus::Unavailable,
                            Some(error.to_string()),
                        );
                    } else if let Err(error) =
                        handle_connection(&state, &surface, stream, &is_active, &mut commands).await
                    {
                        state.update_status(
                            &surface.surface_id,
                            NetworkSurfaceStatus::Unavailable,
                            Some(error),
                        );
                    }
                }
                Ok(Err(error)) => state.update_status(
                    &surface.surface_id,
                    NetworkSurfaceStatus::Unavailable,
                    Some(error.to_string()),
                ),
                Err(_) => state.update_status(
                    &surface.surface_id,
                    NetworkSurfaceStatus::Unavailable,
                    Some("connection timed out".to_string()),
                ),
            }

            if is_active.load(Ordering::Acquire) {
                sleep(RECONNECT_DELAY).await;
            }
        }
    });
}

async fn handle_connection(
    state: &AppState,
    surface: &ManagedNetworkSurface,
    mut stream: TcpStream,
    is_active: &AtomicBool,
    commands: &mut tokio::sync::mpsc::Receiver<SurfaceCommand>,
) -> Result<(), String> {
    let transport = timeout(
        READ_TIMEOUT,
        wait_for_handshake(state, &surface.surface_id, &mut stream),
    )
    .await
    .map_err(|_| "no Stream Deck handshake received before timeout".to_string())??;
    let is_network_dock = surface.model == "Stream Deck Network Dock";

    if is_network_dock {
        request_device_info(&mut stream, transport).await?;
    }

    if !is_network_dock {
        for rendering in state.surfaces.active_key_renderings(&surface.surface_id) {
            send_command(
                &mut stream,
                transport,
                SurfaceCommand::RenderKey(rendering),
                surface.model == "Stream Deck XL",
            )
            .await?;
        }
    }

    let mut child_query_interval = interval(Duration::from_secs(5));
    child_query_interval.tick().await;

    while is_active.load(Ordering::Acquire) {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else {
                    return Ok(());
                };
                send_command(
                    &mut stream,
                    transport,
                    command,
                    surface.model == "Stream Deck XL",
                )
                .await?;
            }
            result = timeout(READ_TIMEOUT, read_transport_packet(state, &surface.surface_id, &mut stream)) => {
                match result {
                    Ok(result) => {
                        let _ = result?;
                    }
                    Err(_) => return Err("no Stream Deck data received before timeout".to_string()),
                }
            }
            _ = child_query_interval.tick(), if is_network_dock => {
                request_child_device(&mut stream, transport, CORA_CHILD_INFO_MESSAGE_ID).await?;
            }
        }
    }

    Ok(())
}

async fn wait_for_handshake(
    state: &AppState,
    surface_id: &SurfaceId,
    stream: &mut TcpStream,
) -> Result<TransportMode, String> {
    loop {
        if let Some(transport) = read_transport_packet(state, surface_id, stream).await? {
            return Ok(transport);
        }
    }
}

async fn read_transport_packet(
    state: &AppState,
    surface_id: &SurfaceId,
    stream: &mut TcpStream,
) -> Result<Option<TransportMode>, String> {
    let mut first_bytes = [0_u8; 4];
    stream
        .read_exact(&mut first_bytes)
        .await
        .map_err(|error| error.to_string())?;

    if first_bytes == CORA_MAGIC {
        handle_cora_packet(state, surface_id, stream).await
    } else {
        handle_legacy_packet(state, surface_id, stream, first_bytes).await
    }
}

async fn handle_legacy_packet(
    state: &AppState,
    surface_id: &SurfaceId,
    stream: &mut TcpStream,
    first_bytes: [u8; 4],
) -> Result<Option<TransportMode>, String> {
    let mut packet = [0_u8; LEGACY_PACKET_SIZE];
    packet[..first_bytes.len()].copy_from_slice(&first_bytes);
    stream
        .read_exact(&mut packet[first_bytes.len()..])
        .await
        .map_err(|error| error.to_string())?;

    if packet[0] != 1 {
        debug!(header = ?&packet[..packet.len().min(8)], "received Stream Deck TCP report");
        if let Some((vendor_id, product_id)) = parse_primary_device_info(&packet) {
            let is_dock = vendor_id == ELGATO_VENDOR_ID && product_id == NETWORK_DOCK_PRODUCT_ID;
            apply_probed_identity(state, surface_id, is_dock, product_id);
            if is_dock {
                request_child_device(stream, TransportMode::Legacy, CORA_CHILD_INFO_MESSAGE_ID)
                    .await?;
            }
        }
        register_network_dock_child(state, surface_id, &packet);
        return Ok(None);
    }

    if packet[1] == 0x0b && is_network_dock(state, surface_id) {
        register_network_dock_child(state, surface_id, &packet);
        return Ok(None);
    }

    if packet[1] != 10 {
        record_key_events(state, surface_id, &packet);
        return Ok(None);
    }

    let mut acknowledgement = [0_u8; LEGACY_RESPONSE_SIZE];
    acknowledgement[0] = 3;
    acknowledgement[1] = 26;
    acknowledgement[2] = packet[5];
    stream
        .write_all(&acknowledgement)
        .await
        .map_err(|error| error.to_string())?;
    state
        .surfaces
        .update_status(surface_id, NetworkSurfaceStatus::Connected, None);

    Ok(Some(TransportMode::Legacy))
}

async fn handle_cora_packet(
    state: &AppState,
    surface_id: &SurfaceId,
    stream: &mut TcpStream,
) -> Result<Option<TransportMode>, String> {
    let mut header = [0_u8; CORA_HEADER_SIZE - CORA_MAGIC.len()];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|error| error.to_string())?;

    let flags = u16::from_le_bytes([header[0], header[1]]);
    let is_verbatim = flags & CORA_VERBATIM != 0;
    let hid_operation = header[2];
    let message_id = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    let payload_size = u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as usize;

    if payload_size > MAX_CORA_PAYLOAD_SIZE {
        return Err(format!(
            "Cora payload exceeded {MAX_CORA_PAYLOAD_SIZE} bytes"
        ));
    }

    let mut payload = vec![0_u8; payload_size];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|error| error.to_string())?;

    if payload.is_empty() {
        return Ok(None);
    }

    if payload.len() <= 5 || payload[0] != 1 {
        debug!(header = ?&payload[..payload.len().min(8)], "received Stream Deck TCP report");
        if !is_verbatim {
            if let Some((vendor_id, product_id)) = parse_primary_device_info(&payload) {
                let is_dock =
                    vendor_id == ELGATO_VENDOR_ID && product_id == NETWORK_DOCK_PRODUCT_ID;
                apply_probed_identity(state, surface_id, is_dock, product_id);
                if is_dock {
                    request_child_device(stream, TransportMode::Cora, CORA_CHILD_INFO_MESSAGE_ID)
                        .await?;
                }
            }
        }
        register_network_dock_child(state, surface_id, &payload);
        return Ok(None);
    }

    if payload[1] != 10 {
        if payload[1] == 0x0b && is_network_dock(state, surface_id) {
            register_network_dock_child(state, surface_id, &payload);
            return Ok(None);
        }
        record_key_events(state, surface_id, &payload);
        return Ok(None);
    }

    let mut acknowledgement = [0_u8; 32];
    acknowledgement[0] = 3;
    acknowledgement[1] = 26;
    acknowledgement[2] = payload[5];
    write_cora_message(
        stream,
        CORA_ACK_NAK,
        hid_operation,
        message_id,
        &acknowledgement,
    )
    .await?;
    state
        .surfaces
        .update_status(surface_id, NetworkSurfaceStatus::Connected, None);

    Ok(Some(TransportMode::Cora))
}

async fn request_device_info(
    stream: &mut TcpStream,
    transport: TransportMode,
) -> Result<(), String> {
    match transport {
        TransportMode::Legacy => {
            let mut primary_packet = [0_u8; LEGACY_RESPONSE_SIZE];
            primary_packet[0] = 0x03;
            primary_packet[1] = 0x80;
            let mut secondary_packet = [0_u8; LEGACY_RESPONSE_SIZE];
            secondary_packet[0] = 0x08;
            let mut mini_packet = [0_u8; LEGACY_RESPONSE_SIZE];
            mini_packet[0] = 0xa1;
            stream
                .write_all(&primary_packet)
                .await
                .map_err(|error| error.to_string())?;
            stream
                .write_all(&secondary_packet)
                .await
                .map_err(|error| error.to_string())?;
            stream
                .write_all(&mini_packet)
                .await
                .map_err(|error| error.to_string())
        }
        TransportMode::Cora => {
            write_cora_message(
                stream,
                0,
                CORA_GET_REPORT,
                CORA_PRIMARY_INFO_MESSAGE_ID,
                &[0x03, 0x80],
            )
            .await?;
            write_cora_message(stream, CORA_VERBATIM, CORA_GET_REPORT, 2, &[0x08]).await?;
            write_cora_message(stream, CORA_VERBATIM, CORA_GET_REPORT, 3, &[0xa1]).await
        }
    }
}

async fn request_child_device(
    stream: &mut TcpStream,
    transport: TransportMode,
    message_id: u32,
) -> Result<(), String> {
    match transport {
        TransportMode::Legacy => {
            let mut packet = [0_u8; LEGACY_RESPONSE_SIZE];
            packet[0] = 0x03;
            packet[1] = 0x1c;
            stream
                .write_all(&packet)
                .await
                .map_err(|error| error.to_string())
        }
        TransportMode::Cora => {
            write_cora_message(stream, 0, CORA_GET_REPORT, message_id, &[0x03, 0x1c]).await
        }
    }
}

fn register_network_dock_child(state: &AppState, parent_surface_id: &SurfaceId, packet: &[u8]) {
    let Some(parent) = state.surfaces.managed(parent_surface_id) else {
        return;
    };

    if parent.model != "Stream Deck Network Dock" || packet.len() < 128 {
        return;
    }

    let Some((product_id, child_port, serial_number)) = parse_child_device_info(packet) else {
        return;
    };

    if child_port == 0 {
        state.surfaces.deactivate_children_of(parent_surface_id);
        return;
    }

    if state
        .surfaces
        .has_managed_endpoint(&parent.host, child_port)
    {
        return;
    }

    let layout = stream_deck_layout(product_id);
    let active_panel_id = state.surfaces.ensure_default_panel_for_layout(&layout);
    let child = ManagedNetworkSurface {
        surface_id: state.surfaces.create_surface_id(),
        name: format!("{} child", parent.name),
        host: parent.host,
        port: child_port,
        serial_number,
        model: stream_deck_model_name(product_id).to_string(),
        is_enabled: true,
        status: NetworkSurfaceStatus::Connecting,
        last_error: None,
        layout,
        capabilities: crate::state::studio_capabilities(),
        active_panel_id,
        parent_surface_id: Some(parent_surface_id.clone()),
    };
    let child = state.surfaces.add_managed_child(parent_surface_id, child);

    start_connection_monitor(state.clone(), child);
}

fn is_network_dock(state: &AppState, surface_id: &SurfaceId) -> bool {
    state
        .surfaces
        .managed(surface_id)
        .is_some_and(|surface| surface.model == "Stream Deck Network Dock")
}

fn parse_primary_device_info(payload: &[u8]) -> Option<(u16, u16)> {
    if payload.len() < 16 || payload[0] != 0x03 || payload[1] != 0x80 {
        return None;
    }
    let vendor_id = u16::from_le_bytes([payload[12], payload[13]]);
    let product_id = u16::from_le_bytes([payload[14], payload[15]]);
    Some((vendor_id, product_id))
}

fn parse_child_device_info(payload: &[u8]) -> Option<(u16, u16, Option<String>)> {
    if payload.len() < 128 {
        return None;
    }

    let is_child_info =
        (payload[0] == 0x03 && payload[1] == 0x1c) || (payload[0] == 0x01 && payload[1] == 0x0b);
    if !is_child_info || payload[4] != 0x02 {
        return None;
    }

    let vendor_id = u16::from_le_bytes([payload[26], payload[27]]);
    if vendor_id != ELGATO_VENDOR_ID {
        return None;
    }

    let product_id = u16::from_le_bytes([payload[28], payload[29]]);
    let child_port = u16::from_le_bytes([payload[126], payload[127]]);
    let serial_number = String::from_utf8_lossy(&payload[94..125])
        .trim_end_matches('\0')
        .to_string();

    Some((
        product_id,
        child_port,
        (!serial_number.is_empty()).then_some(serial_number),
    ))
}

fn apply_probed_identity(state: &AppState, surface_id: &SurfaceId, is_dock: bool, product_id: u16) {
    if is_dock {
        state.surfaces.set_identity(
            surface_id,
            "Stream Deck Network Dock".to_string(),
            SurfaceLayout::Freeform,
        );
    } else {
        state.surfaces.set_identity(
            surface_id,
            stream_deck_model_name(product_id).to_string(),
            stream_deck_layout(product_id),
        );
    }
}

fn stream_deck_model_name(product_id: u16) -> &'static str {
    match product_id {
        0x0060 | 0x006d => "Stream Deck",
        0x0063 | 0x0090 | 0x00b3 => "Stream Deck Mini",
        0x006c | 0x008f => "Stream Deck XL",
        0x0080 | 0x00a5 => "Stream Deck Mk.2",
        0x0084 => "Stream Deck Plus",
        0x009a => "Stream Deck Neo",
        0x00aa => "Stream Deck Studio",
        NETWORK_DOCK_PRODUCT_ID => "Stream Deck Network Dock",
        _ => "Stream Deck",
    }
}

fn stream_deck_layout(product_id: u16) -> SurfaceLayout {
    match product_id {
        0x006c | 0x008f => SurfaceLayout::Grid {
            columns: 8,
            rows: 4,
        },
        0x0063 | 0x0090 | 0x00b3 => SurfaceLayout::Grid {
            columns: 3,
            rows: 2,
        },
        0x0084 | 0x009a => SurfaceLayout::Grid {
            columns: 4,
            rows: 2,
        },
        0x00aa => SurfaceLayout::Grid {
            columns: 16,
            rows: 2,
        },
        _ => SurfaceLayout::Grid {
            columns: 5,
            rows: 3,
        },
    }
}

fn record_key_events(state: &AppState, surface_id: &SurfaceId, packet: &[u8]) {
    if packet.len() < 2 {
        return;
    }

    match packet[1] {
        0 if packet.len() >= 36 => {
            for key_index in 0..32 {
                let is_pressed = packet[4 + key_index as usize] != 0;
                let did_change = state
                    .surfaces
                    .record_key_state(surface_id, key_index, is_pressed);
                if is_pressed && did_change {
                    info!(
                        surface_id = surface_id.0,
                        key_index, "Stream Deck key pressed"
                    );
                }
            }
        }
        3 if packet.len() >= 6 && packet[4] == 0 => {
            for dial_index in 0..2 {
                if packet[5 + dial_index] != 0 {
                    info!(
                        surface_id = surface_id.0,
                        dial_index, "Stream Deck dial pressed"
                    );
                }
            }
        }
        3 if packet.len() >= 6 && packet[4] == 1 => {
            for dial_index in 0..2 {
                let delta = packet[5 + dial_index] as i8;
                if delta != 0 {
                    info!(
                        surface_id = surface_id.0,
                        dial_index, delta, "Stream Deck dial turned"
                    );
                }
            }
        }
        4 if packet.len() >= 4 => {
            let identifier_length = usize::from(packet[2]) + usize::from(packet[3]) * 256;
            let identifier_end = 4 + identifier_length;
            if packet.len() >= identifier_end {
                let identifier = String::from_utf8_lossy(&packet[4..identifier_end]);
                info!(surface_id = surface_id.0, nfc_identifier = %identifier, "Stream Deck NFC scanned");
            }
        }
        _ => {}
    }
}

async fn send_command(
    stream: &mut TcpStream,
    transport: TransportMode,
    command: SurfaceCommand,
    flip_image: bool,
) -> Result<(), String> {
    let SurfaceCommand::RenderKey(rendering) = command;
    let image = render_key_image(&rendering, flip_image)?;
    let chunk_size = LEGACY_RESPONSE_SIZE - IMAGE_REPORT_HEADER_SIZE;
    let chunk_count = (image.len() + chunk_size - 1) / chunk_size;

    for (page, chunk) in image.chunks(chunk_size).enumerate() {
        let page = u16::try_from(page).map_err(|_| "key image has too many pages".to_string())?;
        let mut report = vec![0_u8; LEGACY_RESPONSE_SIZE];
        report[0] = 0x02;
        report[1] = 0x07;
        report[2] = rendering.key_index;
        report[3] = u8::from(usize::from(page) + 1 == chunk_count);
        let chunk_size = u16::try_from(chunk.len())
            .map_err(|_| "key image page exceeds Stream Deck payload size".to_string())?;
        report[4..6].copy_from_slice(&chunk_size.to_le_bytes());
        report[6..8].copy_from_slice(&page.to_le_bytes());
        report[IMAGE_REPORT_HEADER_SIZE..IMAGE_REPORT_HEADER_SIZE + chunk.len()]
            .copy_from_slice(chunk);

        match transport {
            TransportMode::Legacy => stream
                .write_all(&report)
                .await
                .map_err(|error| error.to_string())?,
            TransportMode::Cora => {
                write_cora_message(stream, CORA_VERBATIM, CORA_WRITE, 0, &report).await?
            }
        }
    }

    Ok(())
}

pub fn render_key(rendering: &KeyRendering) -> Result<Vec<u8>, String> {
    render_key_image(rendering, false)
}

fn render_key_image(rendering: &KeyRendering, flip_image: bool) -> Result<Vec<u8>, String> {
    let background = rendering.background_color.as_ref().map_or((0, 0, 0), rgb);
    let foreground = rendering
        .foreground_color
        .as_ref()
        .map_or((255, 255, 255), rgb);
    let mut pixels = vec![0_u8; STUDIO_KEY_IMAGE_SIZE * STUDIO_KEY_IMAGE_SIZE * 3];

    for pixel in pixels.chunks_exact_mut(3) {
        pixel.copy_from_slice(&[background.0, background.1, background.2]);
    }

    if let Some(icon) = &rendering.icon {
        draw_icon(&mut pixels, icon, foreground);
    }
    if let Some(text) = &rendering.text {
        draw_text(&mut pixels, text, foreground, rendering.icon.is_some());
    }

    if flip_image {
        flip_pixels_180(&mut pixels, STUDIO_KEY_IMAGE_SIZE, STUDIO_KEY_IMAGE_SIZE);
    }

    let mut encoded = Vec::new();
    Encoder::new(&mut encoded, 85)
        .encode(
            &pixels,
            STUDIO_KEY_IMAGE_SIZE as u16,
            STUDIO_KEY_IMAGE_SIZE as u16,
            ColorType::Rgb,
        )
        .map_err(|error| error.to_string())?;

    Ok(encoded)
}

fn flip_pixels_180(pixels: &mut [u8], width: usize, height: usize) {
    let pixel_count = width * height;
    for pixel_index in 0..pixel_count / 2 {
        let opposite_index = pixel_count - pixel_index - 1;
        for channel in 0..3 {
            pixels.swap(pixel_index * 3 + channel, opposite_index * 3 + channel);
        }
    }
}

fn rgb(color: &RgbaColor) -> (u8, u8, u8) {
    let alpha = u16::from(color.alpha);
    (
        (u16::from(color.red) * alpha / u16::from(u8::MAX)) as u8,
        (u16::from(color.green) * alpha / u16::from(u8::MAX)) as u8,
        (u16::from(color.blue) * alpha / u16::from(u8::MAX)) as u8,
    )
}

fn draw_icon(pixels: &mut [u8], icon: &KeyIcon, color: (u8, u8, u8)) {
    for y in 20..62 {
        for x in 20..76 {
            let center_x = 48_i32;
            let center_y = 40_i32;
            let x = x as i32;
            let y = y as i32;
            let is_set = match icon {
                KeyIcon::Circle => (x - center_x).pow(2) + (y - center_y).pow(2) <= 18_i32.pow(2),
                KeyIcon::Diamond => (x - center_x).abs() + (y - center_y).abs() <= 22,
                KeyIcon::Pause => (28..40).contains(&x) || (56..68).contains(&x),
                KeyIcon::Play => x >= 30 && x <= 68 && (y - center_y).abs() <= (x - 30) / 2,
                KeyIcon::Square => (28..68).contains(&x) && (20..60).contains(&y),
                KeyIcon::Triangle => y >= 20 && y <= 62 && (x - center_x).abs() <= (y - 20) / 2,
            };
            if is_set {
                set_pixel(pixels, x as usize, y as usize, color);
            }
        }
    }
}

fn draw_text(pixels: &mut [u8], text: &str, color: (u8, u8, u8), has_icon: bool) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }

    let font = &*KEY_FONT;
    let max_width = STUDIO_KEY_IMAGE_SIZE as f32 - KEY_TEXT_PADDING * 2.0;
    let (band_top, band_height) = if has_icon {
        (62.0, STUDIO_KEY_IMAGE_SIZE as f32 - 62.0 - KEY_TEXT_PADDING)
    } else {
        (
            KEY_TEXT_PADDING,
            STUDIO_KEY_IMAGE_SIZE as f32 - KEY_TEXT_PADDING * 2.0,
        )
    };

    let px = fit_text_scale(font, text, max_width, band_height);
    let scaled = font.as_scaled(PxScale::from(px));
    let text_width: f32 = text
        .chars()
        .map(|character| scaled.h_advance(font.glyph_id(character)))
        .sum();

    let start_x = (STUDIO_KEY_IMAGE_SIZE as f32 - text_width) / 2.0;
    let ascent = scaled.ascent();
    let descent = -scaled.descent();
    let baseline_y = band_top + (band_height - (ascent + descent)) / 2.0 + ascent;

    let mut caret_x = start_x;
    for character in text.chars() {
        let glyph_id = font.glyph_id(character);
        let glyph = Glyph {
            id: glyph_id,
            scale: PxScale::from(px),
            position: point(caret_x, baseline_y),
        };
        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|glyph_x, glyph_y, coverage| {
                blend_pixel(
                    pixels,
                    bounds.min.x as i32 + glyph_x as i32,
                    bounds.min.y as i32 + glyph_y as i32,
                    color,
                    coverage,
                );
            });
        }
        caret_x += scaled.h_advance(glyph_id);
    }
}

fn fit_text_scale(font: &FontRef<'static>, text: &str, max_width: f32, max_height: f32) -> f32 {
    let width_at = |px: f32| -> f32 {
        let scaled = font.as_scaled(PxScale::from(px));
        text.chars()
            .map(|character| scaled.h_advance(font.glyph_id(character)))
            .sum()
    };

    let mut px = max_height.min(KEY_TEXT_MAX_PX);
    let width = width_at(px);
    if width > max_width && width > 0.0 {
        px *= max_width / width;
    }
    px.clamp(KEY_TEXT_MIN_PX, KEY_TEXT_MAX_PX)
}

fn set_pixel(pixels: &mut [u8], x: usize, y: usize, color: (u8, u8, u8)) {
    if x >= STUDIO_KEY_IMAGE_SIZE || y >= STUDIO_KEY_IMAGE_SIZE {
        return;
    }
    let offset = (y * STUDIO_KEY_IMAGE_SIZE + x) * 3;
    pixels[offset..offset + 3].copy_from_slice(&[color.0, color.1, color.2]);
}

fn blend_pixel(pixels: &mut [u8], x: i32, y: i32, color: (u8, u8, u8), coverage: f32) {
    if x < 0 || y < 0 || x >= STUDIO_KEY_IMAGE_SIZE as i32 || y >= STUDIO_KEY_IMAGE_SIZE as i32 {
        return;
    }
    let coverage = coverage.clamp(0.0, 1.0);
    let offset = (y as usize * STUDIO_KEY_IMAGE_SIZE + x as usize) * 3;
    let channels = [color.0, color.1, color.2];
    for (index, channel) in channels.into_iter().enumerate() {
        let background = f32::from(pixels[offset + index]);
        let blended = background + (f32::from(channel) - background) * coverage;
        pixels[offset + index] = blended.round().clamp(0.0, 255.0) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        flip_pixels_180, parse_child_device_info, parse_primary_device_info, render_key_image,
        stream_deck_layout, stream_deck_model_name, KeyIcon, KeyRendering, RgbaColor,
        SurfaceLayout, ELGATO_VENDOR_ID, NETWORK_DOCK_PRODUCT_ID,
    };

    #[test]
    fn parses_vendor_and_product_from_a_primary_device_info_reply() {
        let mut payload = [0_u8; 64];
        payload[0] = 0x03;
        payload[1] = 0x80;
        payload[12..14].copy_from_slice(&ELGATO_VENDOR_ID.to_le_bytes());
        payload[14..16].copy_from_slice(&NETWORK_DOCK_PRODUCT_ID.to_le_bytes());
        assert_eq!(
            parse_primary_device_info(&payload),
            Some((ELGATO_VENDOR_ID, NETWORK_DOCK_PRODUCT_ID))
        );
        assert_eq!(parse_primary_device_info(&[0x03, 0x80]), None);
        assert_eq!(parse_primary_device_info(&[0_u8; 64]), None);
    }

    #[test]
    fn maps_product_ids_to_the_correct_model_and_layout() {
        assert_eq!(stream_deck_model_name(0x0084), "Stream Deck Plus");
        assert_eq!(
            stream_deck_layout(0x0084),
            SurfaceLayout::Grid {
                columns: 4,
                rows: 2
            }
        );
        assert_eq!(stream_deck_model_name(0x006c), "Stream Deck XL");
        assert_eq!(
            stream_deck_layout(0x006c),
            SurfaceLayout::Grid {
                columns: 8,
                rows: 4
            }
        );
        assert_eq!(stream_deck_model_name(0x00aa), "Stream Deck Studio");
        assert_eq!(
            stream_deck_layout(0x00aa),
            SurfaceLayout::Grid {
                columns: 16,
                rows: 2
            }
        );
        assert_eq!(
            stream_deck_model_name(NETWORK_DOCK_PRODUCT_ID),
            "Stream Deck Network Dock"
        );
    }

    #[test]
    fn parses_a_network_dock_child_report() {
        let mut payload = [0_u8; 512];
        payload[0..5].copy_from_slice(&[0x01, 0x0b, 0x7c, 0x00, 0x02]);
        payload[26..28].copy_from_slice(&ELGATO_VENDOR_ID.to_le_bytes());
        payload[28..30].copy_from_slice(&0x008f_u16.to_le_bytes());
        payload[94..108].copy_from_slice(b"A00NA33330CRSJ");
        payload[126..128].copy_from_slice(&20004_u16.to_le_bytes());

        assert_eq!(
            parse_child_device_info(&payload),
            Some((0x008f, 20004, Some("A00NA33330CRSJ".to_string())))
        );
    }

    #[test]
    fn rejects_non_child_and_truncated_reports() {
        assert_eq!(parse_child_device_info(&[0_u8; 127]), None);

        let mut payload = [0_u8; 128];
        payload[0..5].copy_from_slice(&[0x01, 0x0b, 0x7c, 0x00, 0x01]);
        assert_eq!(parse_child_device_info(&payload), None);
    }

    #[test]
    fn renders_a_jpeg_for_text_icon_and_color() {
        let image = render_key_image(
            &KeyRendering {
                key_index: 0,
                text: Some("Hello".to_string()),
                icon: Some(KeyIcon::Circle),
                foreground_color: Some(RgbaColor {
                    red: u8::MAX,
                    green: u8::MAX,
                    blue: u8::MAX,
                    alpha: u8::MAX,
                }),
                background_color: Some(RgbaColor {
                    red: 10,
                    green: 20,
                    blue: 30,
                    alpha: u8::MAX,
                }),
            },
            false,
        )
        .expect("key rendering should encode");

        assert!(image.starts_with(&[0xff, 0xd8]));
        assert!(image.ends_with(&[0xff, 0xd9]));
    }

    #[test]
    fn flips_xl_pixels_by_180_degrees() {
        let mut pixels = vec![1, 0, 0, 2, 0, 0, 3, 0, 0, 4, 0, 0];

        flip_pixels_180(&mut pixels, 2, 2);

        assert_eq!(pixels, vec![4, 0, 0, 3, 0, 0, 2, 0, 0, 1, 0, 0]);
    }
}

async fn write_cora_message(
    stream: &mut TcpStream,
    flags: u16,
    hid_operation: u8,
    message_id: u32,
    payload: &[u8],
) -> Result<(), String> {
    let mut packet = Vec::with_capacity(CORA_HEADER_SIZE + payload.len());
    packet.extend_from_slice(&CORA_MAGIC);
    packet.extend_from_slice(&flags.to_le_bytes());
    packet.push(hid_operation);
    packet.push(0);
    packet.extend_from_slice(&message_id.to_le_bytes());
    packet.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    packet.extend_from_slice(payload);

    stream
        .write_all(&packet)
        .await
        .map_err(|error| error.to_string())
}

fn discovered_surface(service: &ResolvedService) -> Option<DiscoveredNetworkSurface> {
    if service
        .get_property_val_str("dt")
        .and_then(parse_device_id)
        .is_some_and(|device_type| device_type == NETWORK_DOCK_DEVICE_TYPE)
    {
        return discovered_network_dock(service);
    }

    let vendor_id = parse_device_id(service.get_property_val_str("vid")?)?;
    let product_id = parse_device_id(service.get_property_val_str("pid")?)?;
    let host = service.get_addresses().iter().next()?.to_string();

    if vendor_id != ELGATO_VENDOR_ID || product_id != STREAM_DECK_STUDIO_PRODUCT_ID {
        return None;
    }

    Some(DiscoveredNetworkSurface {
        discovery_id: service.get_fullname().to_string(),
        name: service.get_fullname().to_string(),
        host,
        port: service.get_port(),
        serial_number: service.get_property_val_str("sn").map(str::to_string),
        model: "Stream Deck Studio".to_string(),
    })
}

fn discovered_network_dock(service: &ResolvedService) -> Option<DiscoveredNetworkSurface> {
    let host = service.get_addresses().iter().next()?.to_string();

    Some(DiscoveredNetworkSurface {
        discovery_id: service.get_fullname().to_string(),
        name: service.get_fullname().to_string(),
        host,
        port: service.get_port(),
        serial_number: service.get_property_val_str("sn").map(str::to_string),
        model: "Stream Deck Network Dock".to_string(),
    })
}

fn parse_device_id(value: &str) -> Option<u16> {
    let value = value.trim();

    match value.strip_prefix("0x") {
        Some(hexadecimal_value) => u16::from_str_radix(hexadecimal_value, 16).ok(),
        None => value.parse().ok(),
    }
}

pub fn default_port() -> u16 {
    DEFAULT_STREAM_DECK_PORT
}
