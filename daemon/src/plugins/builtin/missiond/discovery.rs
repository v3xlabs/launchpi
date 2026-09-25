use std::{collections::BTreeMap, sync::Arc};

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use reqwest::Url;
use tokio::sync::watch;

use crate::plugins::plugin::{CancelToken, PluginError};

const SERVICE_TYPE: &str = "_missiond._tcp.local.";

#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    pub device_id: String,
    pub name: String,
    pub api: Url,
}

/// Every display announcing itself on the local network, keyed by its mDNS instance name, which is
/// what a removal names.
pub struct Discovery {
    displays: watch::Sender<BTreeMap<String, Display>>,
}

impl Discovery {
    /// Browses until the instance is cancelled.
    pub fn start(cancel: CancelToken) -> Result<Arc<Self>, PluginError> {
        let failed = |error: mdns_sd::Error| {
            PluginError::Upstream(format!("cannot browse the network for displays: {error}"))
        };
        let daemon = ServiceDaemon::new().map_err(failed)?;
        let events = daemon.browse(SERVICE_TYPE).map_err(failed)?;
        let discovery = Arc::new(Self {
            displays: watch::Sender::new(BTreeMap::new()),
        });

        let browsing = discovery.clone();
        tokio::spawn(async move {
            loop {
                let event = tokio::select! {
                    _ = cancel.cancelled() => break,
                    event = events.recv_async() => event,
                };
                match event {
                    Ok(ServiceEvent::ServiceResolved(service)) => {
                        if let Some(display) = display_from(&service) {
                            let fullname = service.get_fullname().to_string();
                            browsing.displays.send_if_modified(|displays| {
                                displays.insert(fullname, display.clone()).as_ref()
                                    != Some(&display)
                            });
                        }
                    }
                    Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                        browsing
                            .displays
                            .send_if_modified(|displays| displays.remove(&fullname).is_some());
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            let _ = daemon.shutdown();
        });

        Ok(discovery)
    }

    pub fn subscribe(&self) -> watch::Receiver<BTreeMap<String, Display>> {
        self.displays.subscribe()
    }

    pub fn displays(&self) -> Vec<Display> {
        self.displays.borrow().values().cloned().collect()
    }
}

/// IPv4 only: an IPv6 address from mDNS is usually link-local, and a URL cannot carry the
/// interface it is scoped to. A display bound to every interface announces its loopback address
/// too, which from here names this machine rather than the display.
fn display_from(service: &ResolvedService) -> Option<Display> {
    let device_id = service.get_property_val_str("device_id")?.to_string();
    let address = service
        .get_addresses_v4()
        .into_iter()
        .filter(|address| !address.is_loopback())
        .min_by_key(|address| (address.is_link_local(), *address))?;
    let api = Url::parse(&format!("http://{address}:{}/api", service.get_port())).ok()?;
    let name = service
        .get_property_val_str("name")
        .filter(|name| !name.is_empty())
        .unwrap_or(&device_id)
        .to_string();

    Some(Display {
        device_id,
        name,
        api,
    })
}
