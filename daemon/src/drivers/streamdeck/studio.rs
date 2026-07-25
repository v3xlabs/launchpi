use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, Instant},
};

use ab_glyph::{point, Font, FontRef, Glyph, PxScale, ScaleFont};
use jpeg_encoder::{ColorType, Encoder};
use lazy_static::lazy_static;
use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc,
    time::{interval, sleep, timeout},
};
use tracing::{debug, info, info_span, trace, warn, Instrument};

use crate::{
    identifiers::SurfaceId,
    panels::rendered_state::RgbaColor,
    state::AppState,
    surfaces::{
        command::{KeyIcon, KeyRendering, SurfaceCommand},
        dials::{DIAL_COUNT, DIAL_RING_SEGMENTS},
        layout::SurfaceLayout,
        logs::SurfaceLogLevel,
        managed::{DiscoveredNetworkSurface, ManagedNetworkSurface, NetworkSurfaceStatus},
    },
};

const ELGATO_VENDOR_ID: u16 = 0x0fd9;
const STREAM_DECK_STUDIO_PRODUCT_ID: u16 = 0x00aa;
const NETWORK_DOCK_DEVICE_TYPE: u16 = 215;
const NETWORK_DOCK_PRODUCT_ID: u16 = 0xffff;
const DEFAULT_STREAM_DECK_PORT: u16 = 5343;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RECONNECT_DELAY: Duration = Duration::from_secs(5);
const CHILD_START_DELAY: Duration = Duration::from_secs(2);
const CHILD_INITIALIZATION_RETRY_DELAY: Duration = Duration::from_secs(1);
const READ_TIMEOUT: Duration = Duration::from_secs(8);
/// A stuck socket write used to hang the connection task forever, silently. Fail instead, so the
/// monitor logs it and reconnects.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// Past this, the device is not draining its socket fast enough to keep up with the render queue.
const SLOW_WRITE_WARNING: Duration = Duration::from_millis(250);
/// The device sends an unsolicited keepalive roughly every 4 seconds.
const INBOUND_GAP_WARNING: Duration = Duration::from_secs(6);
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
const DIAL_RING_COMMAND: u8 = 0x0f;
const DIAL_KNOB_COMMAND: u8 = 0x10;
/// Header plus one byte per dial: the last dial sits at `5 + DIAL_COUNT - 1`.
const DIAL_REPORT_SIZE: usize = 5 + DIAL_COUNT as usize;
const CHILD_QUERY_INTERVAL: Duration = Duration::from_secs(5);
/// Replies the read loop owes the device - acknowledgements and probes, never renders.
const REPLY_QUEUE_SIZE: usize = 8;

const KEY_FONT_BYTES: &[u8] = include_bytes!("../../../assets/DejaVuSans.ttf");

lazy_static! {
    static ref KEY_FONT: FontRef<'static> =
        FontRef::try_from_slice(KEY_FONT_BYTES).expect("embedded key font is valid");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransportMode {
    Cora,
    Legacy,
}

/// Counters for one TCP session, so a disconnect can be explained from a single log line instead of
/// needing trace logging to already have been on when it happened.
struct ConnectionStats {
    connected_at: Instant,
    last_inbound_at: Instant,
    inbound_packets: u64,
    unhandled_reports: u64,
    suspicious_reads: u64,
}

impl ConnectionStats {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            connected_at: now,
            last_inbound_at: now,
            inbound_packets: 0,
            unhandled_reports: 0,
            suspicious_reads: 0,
        }
    }

    fn note_inbound(&mut self) {
        let gap = self.last_inbound_at.elapsed();
        self.last_inbound_at = Instant::now();
        self.inbound_packets += 1;
        if gap > INBOUND_GAP_WARNING {
            warn!(
                gap_ms = gap.as_millis(),
                inbound_packets = self.inbound_packets,
                "gap between Stream Deck packets exceeded the keepalive interval"
            );
        }
    }

    fn uptime_ms(&self) -> u128 {
        self.connected_at.elapsed().as_millis()
    }

    fn since_inbound_ms(&self) -> u128 {
        self.last_inbound_at.elapsed().as_millis()
    }
}

/// A report the read loop owes the device. The two loops own opposite halves of the socket, so the
/// reader hands its acknowledgements to the writer instead of writing them itself.
struct OutboundReport {
    what: &'static str,
    bytes: Vec<u8>,
}

/// What the read and write loops of one connection share.
struct ConnectionIo<'a> {
    state: &'a AppState,
    surface: &'a ManagedNetworkSurface,
    transport: TransportMode,
    reports_written: &'a AtomicU64,
    is_active: &'a AtomicBool,
}

/// The device is the slow end of the pipeline: every report is a padded kilobyte and one dial spin
/// produces dozens a second. A render that has already been superseded is worthless, so only the
/// newest state per key and per dial survives to reach the wire.
#[derive(Default)]
struct PendingRenders {
    keys: BTreeMap<u8, KeyRendering>,
    dials: BTreeMap<u8, (RgbaColor, u8)>,
    /// Knob colours already on the device. The ring report carries the level, so a spin at a fixed
    /// colour needs no knob report at all - half the bytes per detent.
    knob_colors: BTreeMap<u8, RgbaColor>,
}

impl PendingRenders {
    fn push(&mut self, command: SurfaceCommand) {
        match command {
            SurfaceCommand::RenderKey(rendering) => {
                self.keys.insert(rendering.key_index, rendering);
            }
            SurfaceCommand::RenderDialColor {
                dial_index,
                color,
                lit_segments,
            } => {
                self.dials.insert(dial_index, (color, lit_segments));
            }
        }
    }

    /// Dials go first: they are live feedback under the user's fingers, while a key image is both
    /// larger and already late by the time it is queued behind one.
    async fn flush<W: AsyncWrite + Unpin>(
        &mut self,
        writer: &mut W,
        transport: TransportMode,
        flip_image: bool,
        reports_written: &AtomicU64,
    ) -> Result<(), String> {
        for (dial_index, (color, lit_segments)) in std::mem::take(&mut self.dials) {
            if self.knob_colors.get(&dial_index) != Some(&color) {
                send_knob_color(writer, transport, dial_index, color.clone()).await?;
                reports_written.fetch_add(1, Ordering::Relaxed);
                self.knob_colors.insert(dial_index, color.clone());
            }
            send_dial_ring(writer, transport, dial_index, color, lit_segments).await?;
            reports_written.fetch_add(1, Ordering::Relaxed);
        }
        for (_, rendering) in std::mem::take(&mut self.keys) {
            send_key_image(writer, transport, rendering, flip_image).await?;
            reports_written.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }
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
    let is_child = surface.parent_surface_id.is_some();
    // Every log emitted while the connection task runs is tagged with this, so the low-level read
    // and write helpers do not have to carry the surface id around.
    let span = info_span!(
        "surface",
        id = surface.surface_id.0,
        host = surface.host,
        port = surface.port
    );

    tokio::spawn(
        async move {
            if is_child {
                sleep(CHILD_START_DELAY).await;
            }

            let mut attempt: u64 = 0;
            let mut consecutive_failures: u64 = 0;

            while is_active.load(Ordering::Acquire) {
                attempt += 1;
                state.update_status(&surface.surface_id, NetworkSurfaceStatus::Connecting, None);
                debug!(attempt, "connecting to Stream Deck");

                let failure = match timeout(
                    CONNECT_TIMEOUT,
                    TcpStream::connect((surface.host.as_str(), surface.port)),
                )
                .await
                {
                    Ok(Ok(stream)) => match stream.set_nodelay(true) {
                        Err(error) => Some(format!("unable to disable Nagle: {error}")),
                        Ok(()) => {
                            handle_connection(&state, &surface, stream, &is_active, &mut commands)
                                .await
                                .err()
                        }
                    },
                    Ok(Err(error)) => Some(error.to_string()),
                    Err(_) => Some(format!(
                        "connection timed out after {}s",
                        CONNECT_TIMEOUT.as_secs()
                    )),
                };

                match failure {
                    Some(error) => {
                        consecutive_failures += 1;
                        warn!(
                            attempt,
                            consecutive_failures,
                            %error,
                            "Stream Deck connection ended with an error"
                        );
                        state.update_status(
                            &surface.surface_id,
                            NetworkSurfaceStatus::Unavailable,
                            Some(error),
                        );
                    }
                    None => {
                        consecutive_failures = 0;
                        info!("Stream Deck connection closed cleanly");
                    }
                }

                if is_active.load(Ordering::Acquire) {
                    debug!(
                        delay_ms = RECONNECT_DELAY.as_millis(),
                        "waiting before reconnecting to Stream Deck"
                    );
                    sleep(RECONNECT_DELAY).await;
                }
            }

            debug!("stopped monitoring Stream Deck");
        }
        .instrument(span),
    );
}

async fn handle_connection(
    state: &AppState,
    surface: &ManagedNetworkSurface,
    mut stream: TcpStream,
    is_active: &AtomicBool,
    commands: &mut tokio::sync::mpsc::Receiver<SurfaceCommand>,
) -> Result<(), String> {
    let mut stats = ConnectionStats::new();
    let (reply_sender, mut replies) = mpsc::channel::<OutboundReport>(REPLY_QUEUE_SIZE);
    let reports_written = AtomicU64::new(0);
    let transport = timeout(
        READ_TIMEOUT,
        wait_for_handshake(
            state,
            &surface.surface_id,
            &mut stream,
            &mut stats,
            &reply_sender,
            &mut replies,
        ),
    )
    .await
    .map_err(|_| {
        format!(
            "no Stream Deck handshake within {}s ({} packets read)",
            READ_TIMEOUT.as_secs(),
            stats.inbound_packets
        )
    })??;
    let is_network_dock = surface.model == "Stream Deck Network Dock";
    info!(
        ?transport,
        handshake_ms = stats.uptime_ms(),
        "Stream Deck connected"
    );
    state.surfaces.log(
        &surface.surface_id,
        SurfaceLogLevel::Info,
        format!(
            "handshake complete over {} in {}ms",
            match transport {
                TransportMode::Cora => "cora",
                TransportMode::Legacy => "legacy hid",
            },
            stats.uptime_ms()
        ),
    );

    if is_network_dock {
        request_device_info(&mut stream, transport).await?;
    }

    if !is_network_dock {
        let initialization_attempts = if surface.parent_surface_id.is_some() {
            2
        } else {
            1
        };
        for attempt in 0..initialization_attempts {
            if attempt > 0 {
                sleep(CHILD_INITIALIZATION_RETRY_DELAY).await;
            }
            let started = Instant::now();
            reset_device(&mut stream, transport).await?;
            reset_key_stream(&mut stream, transport).await?;
            let renderings = state.surfaces.active_key_renderings(&surface.surface_id);
            let key_count = renderings.len();
            for rendering in renderings {
                send_key_image(
                    &mut stream,
                    transport,
                    rendering,
                    surface.model == "Stream Deck XL",
                )
                .await?;
            }
            let dials = state.surfaces.active_dial_rings(&surface.surface_id);
            let dial_count = dials.len();
            for (dial_index, color, lit_segments) in dials {
                send_knob_color(&mut stream, transport, dial_index, color.clone()).await?;
                send_dial_ring(&mut stream, transport, dial_index, color, lit_segments).await?;
            }
            // Nothing is read while this runs, so a slow initial paint delays the first keepalive
            // acknowledgement. Worth seeing the number.
            debug!(
                attempt,
                key_count,
                dial_count,
                elapsed_ms = started.elapsed().as_millis(),
                "painted active panel onto Stream Deck"
            );
        }
    }

    // Reads and writes get their own half of the socket from here on. Sharing one task made a
    // half-read packet collateral damage of whichever write won the `select!`, which desynced the
    // stream, and made every keepalive acknowledgement wait behind the render queue.
    let (read_half, write_half) = stream.into_split();
    let io = ConnectionIo {
        state,
        surface,
        transport,
        reports_written: &reports_written,
        is_active,
    };
    let outcome = tokio::select! {
        result = read_loop(read_half, &mut stats, &reply_sender, &io) => result,
        result = write_loop(write_half, is_network_dock, commands, &mut replies, &io) => result,
    };

    debug!(
        uptime_ms = stats.uptime_ms(),
        inbound_packets = stats.inbound_packets,
        reports_written = reports_written.load(Ordering::Relaxed),
        is_error = outcome.is_err(),
        "Stream Deck connection ended"
    );
    outcome
}

async fn read_loop<R: AsyncRead + Unpin>(
    mut reader: R,
    stats: &mut ConnectionStats,
    replies: &mpsc::Sender<OutboundReport>,
    io: &ConnectionIo<'_>,
) -> Result<(), String> {
    let reports_written = io.reports_written;
    while io.is_active.load(Ordering::Acquire) {
        match timeout(
            READ_TIMEOUT,
            read_transport_packet(
                io.state,
                &io.surface.surface_id,
                &mut reader,
                Some(io.transport),
                stats,
                replies,
            ),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                warn!(
                    %error,
                    uptime_ms = stats.uptime_ms(),
                    inbound_packets = stats.inbound_packets,
                    reports_written = reports_written.load(Ordering::Relaxed),
                    unhandled_reports = stats.unhandled_reports,
                    suspicious_reads = stats.suspicious_reads,
                    "read from the Stream Deck failed"
                );
                return Err(error);
            }
            Err(_) => {
                return Err(format!(
                    "no data for {}s (last packet {}ms ago, {} packets, {} reports written, {} unhandled reports, {} suspicious reads)",
                    READ_TIMEOUT.as_secs(),
                    stats.since_inbound_ms(),
                    stats.inbound_packets,
                    reports_written.load(Ordering::Relaxed),
                    stats.unhandled_reports,
                    stats.suspicious_reads,
                ));
            }
        }
    }
    Ok(())
}

async fn write_loop<W: AsyncWrite + Unpin>(
    mut writer: W,
    is_network_dock: bool,
    commands: &mut mpsc::Receiver<SurfaceCommand>,
    replies: &mut mpsc::Receiver<OutboundReport>,
    io: &ConnectionIo<'_>,
) -> Result<(), String> {
    let (transport, reports_written) = (io.transport, io.reports_written);
    let flip_image = io.surface.model == "Stream Deck XL";
    let mut child_query_interval = interval(CHILD_QUERY_INTERVAL);
    child_query_interval.tick().await;
    let mut pending = PendingRenders::default();

    while io.is_active.load(Ordering::Acquire) {
        tokio::select! {
            // Acknowledgements keep the session alive, so they never queue behind renders.
            biased;
            Some(reply) = replies.recv() => {
                write_all_timed(&mut writer, &reply.bytes, reply.what).await?;
                reports_written.fetch_add(1, Ordering::Relaxed);
            }
            command = commands.recv() => {
                let Some(command) = command else {
                    debug!(
                        reports_written = reports_written.load(Ordering::Relaxed),
                        "surface command channel closed, ending connection"
                    );
                    return Ok(());
                };
                pending.push(command);
                // Everything already queued is collapsed into this one pass, so a burst of dial
                // detents costs one ring report instead of one per detent.
                let mut coalesced = 0_u64;
                while let Ok(next) = commands.try_recv() {
                    pending.push(next);
                    coalesced += 1;
                }
                let started = Instant::now();
                pending.flush(&mut writer, transport, flip_image, reports_written).await?;
                let elapsed = started.elapsed();
                if elapsed > SLOW_WRITE_WARNING {
                    warn!(
                        elapsed_ms = elapsed.as_millis(),
                        coalesced,
                        "slow write to the Stream Deck"
                    );
                    io.state.surfaces.log(
                        &io.surface.surface_id,
                        SurfaceLogLevel::Warning,
                        format!(
                            "slow write: {}ms, {coalesced} superseded renders dropped",
                            elapsed.as_millis()
                        ),
                    );
                }
            }
            _ = child_query_interval.tick(), if is_network_dock => {
                let request = child_device_request(transport, CORA_CHILD_INFO_MESSAGE_ID);
                write_all_timed(&mut writer, &request.bytes, request.what).await?;
                reports_written.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    Ok(())
}

/// The socket is still whole here, so this drains the replies the read path queues itself.
async fn wait_for_handshake(
    state: &AppState,
    surface_id: &SurfaceId,
    stream: &mut TcpStream,
    stats: &mut ConnectionStats,
    reply_sender: &mpsc::Sender<OutboundReport>,
    replies: &mut mpsc::Receiver<OutboundReport>,
) -> Result<TransportMode, String> {
    loop {
        let negotiated =
            read_transport_packet(state, surface_id, stream, None, stats, reply_sender).await?;
        while let Ok(reply) = replies.try_recv() {
            write_all_timed(stream, &reply.bytes, reply.what).await?;
        }
        if let Some(transport) = negotiated {
            return Ok(transport);
        }
    }
}

async fn read_transport_packet<R: AsyncRead + Unpin>(
    state: &AppState,
    surface_id: &SurfaceId,
    stream: &mut R,
    negotiated: Option<TransportMode>,
    stats: &mut ConnectionStats,
    replies: &mpsc::Sender<OutboundReport>,
) -> Result<Option<TransportMode>, String> {
    let mut first_bytes = [0_u8; 4];
    stream
        .read_exact(&mut first_bytes)
        .await
        .map_err(|error| error.to_string())?;
    stats.note_inbound();

    if first_bytes == CORA_MAGIC {
        handle_cora_packet(state, surface_id, stream, stats, replies).await
    } else {
        // Once a session speaks Cora every packet carries the magic. A packet that does not means
        // we are reading from the middle of one - the stream is out of frame.
        if negotiated == Some(TransportMode::Cora) {
            stats.suspicious_reads += 1;
            warn!(
                header = ?first_bytes,
                suspicious_reads = stats.suspicious_reads,
                "expected a Cora frame but found no magic; the read stream may be out of frame"
            );
        }
        handle_legacy_packet(state, surface_id, stream, first_bytes, stats, replies).await
    }
}

async fn handle_legacy_packet<R: AsyncRead + Unpin>(
    state: &AppState,
    surface_id: &SurfaceId,
    stream: &mut R,
    first_bytes: [u8; 4],
    stats: &mut ConnectionStats,
    replies: &mpsc::Sender<OutboundReport>,
) -> Result<Option<TransportMode>, String> {
    let mut packet = [0_u8; LEGACY_PACKET_SIZE];
    packet[..first_bytes.len()].copy_from_slice(&first_bytes);
    stream
        .read_exact(&mut packet[first_bytes.len()..])
        .await
        .map_err(|error| error.to_string())?;
    trace!(
        header = ?&packet[..8],
        "read legacy Stream Deck packet"
    );

    // Every report the device sends starts with a non-zero report id; zeroes are packet padding,
    // so reading one as a header means the stream is out of frame.
    if packet[0] == 0 {
        stats.suspicious_reads += 1;
        warn!(
            header = ?&packet[..8],
            suspicious_reads = stats.suspicious_reads,
            "read a zero report id; the read stream may be out of frame"
        );
        return Ok(None);
    }

    if packet[0] != 1 {
        debug!(header = ?&packet[..packet.len().min(8)], "received Stream Deck TCP report");
        if let Some((vendor_id, product_id)) = parse_primary_device_info(&packet) {
            let is_dock = vendor_id == ELGATO_VENDOR_ID && product_id == NETWORK_DOCK_PRODUCT_ID;
            apply_probed_identity(state, surface_id, is_dock, product_id);
            if is_dock {
                queue_reply(
                    replies,
                    child_device_request(TransportMode::Legacy, CORA_CHILD_INFO_MESSAGE_ID),
                );
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
        record_key_events(state, surface_id, &packet, stats);
        return Ok(None);
    }

    let mut acknowledgement = vec![0_u8; LEGACY_RESPONSE_SIZE];
    acknowledgement[0] = 3;
    acknowledgement[1] = 26;
    acknowledgement[2] = packet[5];
    trace!(
        connection = packet[5],
        "acknowledging Stream Deck keepalive"
    );
    queue_reply(
        replies,
        OutboundReport {
            what: "keepalive acknowledgement",
            bytes: acknowledgement,
        },
    );
    state
        .surfaces
        .update_status(surface_id, NetworkSurfaceStatus::Connected, None);

    Ok(Some(TransportMode::Legacy))
}

async fn handle_cora_packet<R: AsyncRead + Unpin>(
    state: &AppState,
    surface_id: &SurfaceId,
    stream: &mut R,
    stats: &mut ConnectionStats,
    replies: &mpsc::Sender<OutboundReport>,
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
            "Cora payload of {payload_size} bytes exceeded {MAX_CORA_PAYLOAD_SIZE} (flags {flags:#06x}, operation {hid_operation:#04x}, message {message_id}) - the read stream is out of frame"
        ));
    }
    // Real payloads are one HID report; anything far bigger means we parsed a header out of frame.
    if payload_size > 4 * LEGACY_RESPONSE_SIZE {
        stats.suspicious_reads += 1;
        warn!(
            payload_size,
            flags = format_args!("{flags:#06x}"),
            hid_operation,
            message_id,
            suspicious_reads = stats.suspicious_reads,
            "implausibly large Cora payload; the read stream may be out of frame"
        );
    }
    trace!(
        payload_size,
        is_verbatim,
        hid_operation,
        message_id,
        "read Cora Stream Deck frame"
    );

    let mut payload = vec![0_u8; payload_size];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|error| error.to_string())?;

    if payload.is_empty() {
        debug!(hid_operation, message_id, "empty Cora payload");
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
                    queue_reply(
                        replies,
                        child_device_request(TransportMode::Cora, CORA_CHILD_INFO_MESSAGE_ID),
                    );
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
        record_key_events(state, surface_id, &payload, stats);
        return Ok(None);
    }

    let mut acknowledgement = [0_u8; 32];
    acknowledgement[0] = 3;
    acknowledgement[1] = 26;
    acknowledgement[2] = payload[5];
    queue_reply(
        replies,
        OutboundReport {
            what: "keepalive acknowledgement",
            bytes: frame_cora_message(CORA_ACK_NAK, hid_operation, message_id, &acknowledgement),
        },
    );
    state
        .surfaces
        .update_status(surface_id, NetworkSurfaceStatus::Connected, None);

    Ok(Some(TransportMode::Cora))
}

/// A reply is only useful while the session it answers is alive, and the reader must never block on
/// a wedged writer, so a full queue drops the reply and lets the read timeout end the connection.
fn queue_reply(replies: &mpsc::Sender<OutboundReport>, report: OutboundReport) {
    if replies.try_send(report).is_err() {
        warn!("dropped a reply to the Stream Deck: the write side is not keeping up");
    }
}

async fn request_device_info<W: AsyncWrite + Unpin>(
    stream: &mut W,
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
            write_all_timed(stream, &primary_packet, "primary device info probe").await?;
            write_all_timed(stream, &secondary_packet, "secondary device info probe").await?;
            write_all_timed(stream, &mini_packet, "mini device info probe").await
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

fn child_device_request(transport: TransportMode, message_id: u32) -> OutboundReport {
    let bytes = match transport {
        TransportMode::Legacy => {
            let mut packet = vec![0_u8; LEGACY_RESPONSE_SIZE];
            packet[0] = 0x03;
            packet[1] = 0x1c;
            packet
        }
        TransportMode::Cora => {
            frame_cora_message(0, CORA_GET_REPORT, message_id, &[0x03, 0x1c])
        }
    };
    OutboundReport {
        what: "child device info request",
        bytes,
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
        debug!(
            header = ?&packet[..packet.len().min(8)],
            "dock report did not parse as child device info"
        );
        return;
    };
    debug!(
        product_id = format_args!("{product_id:#06x}"),
        model = stream_deck_model_name(product_id),
        child_port,
        serial_number = serial_number.as_deref().unwrap_or("unknown"),
        "dock reports an attached Stream Deck"
    );

    if child_port == 0 {
        info!("dock reports no attached Stream Deck, disconnecting its child");
        state.surfaces.deactivate_children_of(parent_surface_id);
        return;
    }

    if state
        .surfaces
        .has_managed_endpoint(&parent.host, child_port)
    {
        trace!(
            child_port,
            "dock child is already managed, nothing to register"
        );
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
        capabilities: crate::surfaces::defaults::studio_capabilities(),
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

fn record_key_events(
    state: &AppState,
    surface_id: &SurfaceId,
    packet: &[u8],
    stats: &mut ConnectionStats,
) {
    if packet.len() < 2 {
        stats.unhandled_reports += 1;
        warn!(length = packet.len(), "input report too short to identify");
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
                    info!(key_index, "Stream Deck key pressed");
                }
            }
        }
        // Like the key report, this carries the state of every dial, so releases arrive here too.
        3 if packet.len() >= DIAL_REPORT_SIZE && packet[4] == 0 => {
            for dial_index in 0..usize::from(DIAL_COUNT) {
                let is_pressed = packet[5 + dial_index] != 0;
                let Ok(dial_index) = u8::try_from(dial_index) else {
                    continue;
                };
                let did_change = state
                    .surfaces
                    .record_dial_press(surface_id, dial_index, is_pressed);
                if did_change {
                    if is_pressed {
                        info!(dial_index, "Stream Deck dial pressed");
                    } else {
                        debug!(dial_index, "Stream Deck dial released");
                    }
                }
            }
        }
        3 if packet.len() >= DIAL_REPORT_SIZE && packet[4] == 1 => {
            for dial_index in 0..usize::from(DIAL_COUNT) {
                let detents = packet[5 + dial_index] as i8;
                if detents == 0 {
                    continue;
                }
                let Ok(dial_index) = u8::try_from(dial_index) else {
                    continue;
                };
                info!(dial_index, detents, "Stream Deck dial turned");
                state
                    .surfaces
                    .record_dial_turn(surface_id, dial_index, detents);
            }
        }
        4 if packet.len() >= 4 => {
            let identifier_length = usize::from(packet[2]) + usize::from(packet[3]) * 256;
            let identifier_end = 4 + identifier_length;
            if packet.len() >= identifier_end {
                let identifier = String::from_utf8_lossy(&packet[4..identifier_end]);
                info!(nfc_identifier = %identifier, "Stream Deck NFC scanned");
            } else {
                stats.unhandled_reports += 1;
                warn!(
                    identifier_length,
                    length = packet.len(),
                    "NFC report claims more payload than it carries"
                );
            }
        }
        // Truncated variants of reports we do handle: worth a warning, since it means either a
        // short read or a report layout that differs from what we expect.
        0 | 3 => {
            stats.unhandled_reports += 1;
            warn!(
                report = packet[1],
                length = packet.len(),
                header = ?&packet[..packet.len().min(8)],
                "input report is too short for its type"
            );
        }
        // An input report the daemon has no handler for. The device does not need a reply, so this
        // is safe to skip - but it is exactly what to look at when the hardware misbehaves.
        report => {
            stats.unhandled_reports += 1;
            debug!(
                report,
                length = packet.len(),
                header = ?&packet[..packet.len().min(12)],
                unhandled_reports = stats.unhandled_reports,
                "unhandled Stream Deck input report"
            );
        }
    }
}

async fn write_all_timed<W: AsyncWrite + Unpin>(
    stream: &mut W,
    bytes: &[u8],
    what: &str,
) -> Result<(), String> {
    let started = Instant::now();
    let result = timeout(WRITE_TIMEOUT, stream.write_all(bytes)).await;
    let elapsed = started.elapsed();

    match result {
        Ok(Ok(())) => {
            if elapsed > SLOW_WRITE_WARNING {
                warn!(
                    what,
                    bytes = bytes.len(),
                    elapsed_ms = elapsed.as_millis(),
                    "slow write to the Stream Deck"
                );
            }
            Ok(())
        }
        Ok(Err(error)) => Err(format!("{what} write failed: {error}")),
        Err(_) => Err(format!(
            "{what} write blocked for more than {}s ({} bytes)",
            WRITE_TIMEOUT.as_secs(),
            bytes.len()
        )),
    }
}

async fn send_key_image<W: AsyncWrite + Unpin>(
    stream: &mut W,
    transport: TransportMode,
    rendering: KeyRendering,
    flip_image: bool,
) -> Result<(), String> {
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

async fn reset_key_stream<W: AsyncWrite + Unpin>(
    stream: &mut W,
    transport: TransportMode,
) -> Result<(), String> {
    let mut report = vec![0_u8; LEGACY_RESPONSE_SIZE];
    report[0] = 0x02;
    match transport {
        TransportMode::Legacy => stream
            .write_all(&report)
            .await
            .map_err(|error| error.to_string()),
        TransportMode::Cora => {
            write_cora_message(stream, CORA_VERBATIM, CORA_WRITE, 0, &report).await
        }
    }
}

async fn reset_device<W: AsyncWrite + Unpin>(
    stream: &mut W,
    transport: TransportMode,
) -> Result<(), String> {
    let mut report = vec![0_u8; 32];
    report[..2].copy_from_slice(&[0x03, 0x02]);
    send_report(stream, transport, &report).await
}

/// Lights the knob's own LED. Independent of the ring, so it only needs sending when the colour
/// changes.
async fn send_knob_color<W: AsyncWrite + Unpin>(
    stream: &mut W,
    transport: TransportMode,
    dial_index: u8,
    color: RgbaColor,
) -> Result<(), String> {
    if dial_index >= DIAL_COUNT {
        return Err(format!(
            "invalid Stream Deck Studio dial index {dial_index}"
        ));
    }
    let (red, green, blue) = rgb(&color);
    let mut knob_report = vec![0_u8; LEGACY_RESPONSE_SIZE];
    knob_report[..6].copy_from_slice(&[0x02, DIAL_KNOB_COMMAND, dial_index, red, green, blue]);
    send_report(stream, transport, &knob_report).await
}

async fn send_dial_ring<W: AsyncWrite + Unpin>(
    stream: &mut W,
    transport: TransportMode,
    dial_index: u8,
    color: RgbaColor,
    lit_segments: u8,
) -> Result<(), String> {
    if dial_index >= DIAL_COUNT {
        return Err(format!(
            "invalid Stream Deck Studio dial index {dial_index}"
        ));
    }
    let ring_segments = usize::from(DIAL_RING_SEGMENTS);
    let (red, green, blue) = rgb(&color);
    let mut segments = vec![[0_u8; 3]; ring_segments];
    for segment in segments
        .iter_mut()
        .take(usize::from(lit_segments.min(DIAL_RING_SEGMENTS)))
    {
        *segment = [red, green, blue];
    }
    if dial_index == 0 {
        segments.rotate_left(ring_segments / 2);
    }
    let mut ring_report = vec![0_u8; LEGACY_RESPONSE_SIZE];
    ring_report[..3].copy_from_slice(&[0x02, DIAL_RING_COMMAND, dial_index]);
    for (index, segment) in segments.into_iter().enumerate() {
        ring_report[3 + index * 3..6 + index * 3].copy_from_slice(&segment);
    }
    send_report(stream, transport, &ring_report).await
}

async fn send_report<W: AsyncWrite + Unpin>(
    stream: &mut W,
    transport: TransportMode,
    report: &[u8],
) -> Result<(), String> {
    match transport {
        TransportMode::Legacy => write_all_timed(stream, report, "report").await,
        TransportMode::Cora => {
            write_cora_message(stream, CORA_VERBATIM, CORA_WRITE, 0, report).await
        }
    }
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
    use std::sync::atomic::AtomicU64;

    use super::{
        flip_pixels_180, parse_child_device_info, parse_primary_device_info, render_key_image,
        stream_deck_layout, stream_deck_model_name, KeyIcon, KeyRendering, PendingRenders,
        RgbaColor, SurfaceCommand, SurfaceLayout, TransportMode, DIAL_RING_COMMAND,
        ELGATO_VENDOR_ID, LEGACY_RESPONSE_SIZE, NETWORK_DOCK_PRODUCT_ID,
    };

    fn amber() -> RgbaColor {
        RgbaColor {
            red: 200,
            green: 120,
            blue: 0,
            alpha: u8::MAX,
        }
    }

    async fn flush(pending: &mut PendingRenders) -> Vec<u8> {
        let mut wire = Vec::new();
        pending
            .flush(
                &mut wire,
                TransportMode::Legacy,
                false,
                &AtomicU64::new(0),
            )
            .await
            .expect("writing to a buffer should not fail");
        wire
    }

    #[tokio::test]
    async fn a_burst_of_dial_turns_collapses_to_the_newest_ring_position() {
        let mut pending = PendingRenders::default();
        for lit_segments in 1..=12 {
            pending.push(SurfaceCommand::RenderDialColor {
                dial_index: 1,
                color: amber(),
                lit_segments,
            });
        }

        let wire = flush(&mut pending).await;

        // One knob colour report, one ring report - not one pair per detent.
        assert_eq!(wire.len(), 2 * LEGACY_RESPONSE_SIZE);
        let ring = &wire[LEGACY_RESPONSE_SIZE..];
        assert_eq!(ring[1], DIAL_RING_COMMAND);
        // Twelve segments lit and the thirteenth dark: the last turn won, not the first.
        assert_eq!(ring[3 + 11 * 3], 200);
        assert_eq!(ring[3 + 12 * 3], 0);
    }

    #[tokio::test]
    async fn spinning_a_dial_at_one_colour_stops_resending_the_knob_colour() {
        let mut pending = PendingRenders::default();
        pending.push(SurfaceCommand::RenderDialColor {
            dial_index: 0,
            color: amber(),
            lit_segments: 4,
        });
        assert_eq!(flush(&mut pending).await.len(), 2 * LEGACY_RESPONSE_SIZE);

        pending.push(SurfaceCommand::RenderDialColor {
            dial_index: 0,
            color: amber(),
            lit_segments: 5,
        });
        let wire = flush(&mut pending).await;

        assert_eq!(wire.len(), LEGACY_RESPONSE_SIZE);
        assert_eq!(wire[1], DIAL_RING_COMMAND);
    }

    #[tokio::test]
    async fn only_the_newest_render_of_a_key_reaches_the_wire() {
        let mut pending = PendingRenders::default();
        for text in ["one", "two", "three"] {
            pending.push(SurfaceCommand::RenderKey(KeyRendering {
                key_index: 3,
                text: Some(text.to_string()),
                ..KeyRendering::default()
            }));
        }

        let wire = flush(&mut pending).await;
        let single = {
            let mut pending = PendingRenders::default();
            pending.push(SurfaceCommand::RenderKey(KeyRendering {
                key_index: 3,
                text: Some("three".to_string()),
                ..KeyRendering::default()
            }));
            flush(&mut pending).await
        };

        assert_eq!(wire, single);
    }

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

fn frame_cora_message(
    flags: u16,
    hid_operation: u8,
    message_id: u32,
    payload: &[u8],
) -> Vec<u8> {
    let mut packet = Vec::with_capacity(CORA_HEADER_SIZE + payload.len());
    packet.extend_from_slice(&CORA_MAGIC);
    packet.extend_from_slice(&flags.to_le_bytes());
    packet.push(hid_operation);
    packet.push(0);
    packet.extend_from_slice(&message_id.to_le_bytes());
    packet.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    packet.extend_from_slice(payload);
    packet
}

async fn write_cora_message<W: AsyncWrite + Unpin>(
    stream: &mut W,
    flags: u16,
    hid_operation: u8,
    message_id: u32,
    payload: &[u8],
) -> Result<(), String> {
    let packet = frame_cora_message(flags, hid_operation, message_id, payload);
    write_all_timed(stream, &packet, "Cora frame").await
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
