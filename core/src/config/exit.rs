use figment::providers::Serialized;
use figment::value::{Dict, Value};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::{DeviceModule, DeviceModuleError, InstantiationContext};
use crate::bus::DeviceIdAllocator;
use crate::device::ExitDevice;
use crate::{AddressRange, BusConfig};

/// Device module that registers the `exit` device: a one-byte register that
/// ends the run with an exit status (see [`ExitDevice`]). It takes no
/// attributes.
#[derive(Clone)]
pub struct ExitModule;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExitAttributes {}

impl DeviceModule for ExitModule {
    fn name(&self) -> &'static str {
        "exit"
    }

    async fn instantiate(
        &self,
        bus_config: BusConfig,
        address: u16,
        attributes: &HashMap<String, Value>,
        _context: &InstantiationContext,
        id_allocator: Arc<Mutex<DeviceIdAllocator>>,
    ) -> Result<BusConfig, DeviceModuleError> {
        let attrs = Dict::from_iter(attributes.clone());
        let _: ExitAttributes = figment::Figment::new()
            .merge(Serialized::defaults(attrs))
            .extract()
            .map_err(|e| DeviceModuleError::Config(format!("configuration error: {e}")))?;

        let device_id = id_allocator.lock().unwrap().next_available();
        bus_config
            .device(
                AddressRange::new(address, address),
                device_id,
                Box::new(ExitDevice::new(address)),
            )
            .map_err(DeviceModuleError::BusConfig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn instantiate(attributes: &[(&str, Value)]) -> Result<BusConfig, DeviceModuleError> {
        let attributes = attributes
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        ExitModule
            .instantiate(
                BusConfig::new(),
                0xFFD2,
                &attributes,
                &InstantiationContext::default(),
                Arc::new(Mutex::new(DeviceIdAllocator::new())),
            )
            .await
    }

    #[tokio::test]
    async fn exits_on_the_sequence() {
        let mut bus = instantiate(&[]).await.unwrap().build();
        for b in *b"EXIT\x08" {
            bus.write(0xFFD2, b).unwrap();
        }
        assert_eq!(
            bus.take_written_request(),
            Some(crate::bus::CpuRequest::Exit(8))
        );
    }

    #[tokio::test]
    async fn takes_no_attributes() {
        let err = instantiate(&[("size", Value::from(2u8))])
            .await
            .err()
            .unwrap();
        assert!(err.to_string().contains("size"), "{err}");
    }
}
