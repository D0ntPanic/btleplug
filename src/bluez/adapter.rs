use super::peripheral::Peripheral;
use crate::api::{BDAddr, Central, CentralEvent};
use crate::{Error, Result};
use async_trait::async_trait;
use bluez_async::{AdapterId, BluetoothError, BluetoothEvent, BluetoothSession, DeviceEvent};
use futures::stream::{Stream, StreamExt};
use log::warn;
use std::pin::Pin;

#[derive(Clone, Debug)]
pub struct Adapter {
    session: BluetoothSession,
    adapter: AdapterId,
}

impl Adapter {
    pub(crate) fn new(session: BluetoothSession, adapter: AdapterId) -> Self {
        Self { session, adapter }
    }
}

#[async_trait]
impl Central<Peripheral> for Adapter {
    async fn events(&self) -> Result<Pin<Box<dyn Stream<Item = CentralEvent>>>> {
        let events = self.session.event_stream().await?;
        let session = self.session.clone();
        Ok(Box::pin(events.filter_map(move |event| {
            central_event(event, session.clone())
        })))
    }

    async fn start_scan(&self) -> Result<()> {
        self.session.start_discovery().await?;
        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        self.session.stop_discovery().await?;
        Ok(())
    }

    async fn peripherals(&self) -> Result<Vec<Peripheral>> {
        let devices = self.session.get_devices().await?;
        Ok(devices
            .into_iter()
            .map(|device| Peripheral::new(self.session.clone(), device))
            .collect())
    }

    async fn peripheral(&self, address: BDAddr) -> Result<Peripheral> {
        let devices = self.session.get_devices().await?;
        devices
            .into_iter()
            .find_map(|device| {
                if BDAddr::from(&device.mac_address) == address {
                    Some(Peripheral::new(self.session.clone(), device))
                } else {
                    None
                }
            })
            .ok_or(Error::DeviceNotFound)
    }
}

impl From<BluetoothError> for Error {
    fn from(error: BluetoothError) -> Self {
        Error::Other(error.to_string())
    }
}

async fn central_event(event: BluetoothEvent, session: BluetoothSession) -> Option<CentralEvent> {
    match event {
        BluetoothEvent::Device {
            id,
            event: DeviceEvent::Discovered,
        } => {
            let device = session.get_device_info(&id).await.ok()?;
            Some(CentralEvent::DeviceDiscovered((&device.mac_address).into()))
        }
        BluetoothEvent::Device {
            id,
            event: DeviceEvent::Connected { connected },
        } => {
            let device = session.get_device_info(&id).await.ok()?;
            if connected {
                Some(CentralEvent::DeviceConnected((&device.mac_address).into()))
            } else {
                Some(CentralEvent::DeviceDisconnected(
                    (&device.mac_address).into(),
                ))
            }
        }
        BluetoothEvent::Device {
            id,
            event: DeviceEvent::RSSI { rssi: _ },
        } => {
            let device = session.get_device_info(&id).await.ok()?;
            Some(CentralEvent::DeviceUpdated((&device.mac_address).into()))
        }
        BluetoothEvent::Device {
            id,
            event: DeviceEvent::ManufacturerData { manufacturer_data },
        } => {
            let device = session.get_device_info(&id).await.ok()?;
            if manufacturer_data.len() > 1 {
                warn!("Got more than one manufacturer data entry.")
            }
            let (manufacturer_id, data) = manufacturer_data.into_iter().next()?;
            Some(CentralEvent::ManufacturerDataAdvertisement {
                address: (&device.mac_address).into(),
                manufacturer_id,
                data,
            })
        }
        _ => None,
    }
}
