use super::device::{deserialize_size, make_buffer};
use super::write_policy::WritePolicySpec;
use super::{DeviceModule, DeviceModuleError, ExpandedPathBuf, InstantiationContext, loader};
use crate::emulator::bus::{DeviceIdAllocator, symbol};
use crate::emulator::{AddressRange, BusConfig};
use figment::providers::Serialized;
use figment::value::{Dict, Value};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// Type name used in registering ROM as a device
const ROM_DEVICE_TYPE: &str = "rom";

/// ROM device module.
#[derive(Clone)]
pub struct RomModule;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RomAttributes {
    #[serde(deserialize_with = "deserialize_size")]
    size: u32,
    offset: Option<isize>,
    fill: Option<u8>,
    image: Option<ExpandedPathBuf>,
    labels: Option<ExpandedPathBuf>,
    #[serde(rename = "write-policy", skip_serializing_if = "Option::is_none")]
    write_policy: Option<WritePolicySpec>,
}

impl RomAttributes {
    fn from_attributes(attributes: &HashMap<String, Value>) -> Result<Self, DeviceModuleError> {
        let attrs = Dict::from_iter(attributes.clone());
        figment::Figment::new()
            .merge(Serialized::defaults(attrs))
            .extract()
            .map_err(|e| DeviceModuleError::Config(format!("configuration error: {e}")))
    }
}

impl DeviceModule for RomModule {
    fn name(&self) -> &'static str {
        ROM_DEVICE_TYPE
    }

    async fn instantiate(
        &self,
        bus_config: BusConfig,
        address: u16,
        attributes: &HashMap<String, Value>,
        _context: &InstantiationContext,
        _id_allocator: Arc<Mutex<DeviceIdAllocator>>,
    ) -> Result<BusConfig, DeviceModuleError> {
        let config = RomAttributes::from_attributes(attributes)?;
        let range = AddressRange::new(address, address + (config.size - 1) as u16);
        let offset = config.offset.unwrap_or(0);

        let bus_config = if let Some(filename) = config.labels {
            let table = symbol::load_vice_labels(filename)
                .await
                .map_err(DeviceModuleError::SymbolTable)?;
            bus_config.symbol_table(&table)
        } else {
            bus_config
        };

        let mut data = make_buffer(config.size as usize, config.fill);
        if let Some(filename) = config.image {
            loader::load_image(&filename, &mut data, offset)
                .await
                .map_err(DeviceModuleError::Load)?;
        }
        match config.write_policy {
            Some(write_policy) => bus_config
                .rom_with_write_policy(range, data, write_policy.to_rom_write_policy())
                .map_err(DeviceModuleError::BusConfig),
            None => bus_config
                .rom(range, data)
                .map_err(DeviceModuleError::BusConfig),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_accepts_kilobyte_shorthand_string() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from("32K"));

        let config = RomAttributes::from_attributes(&attributes).unwrap();

        assert_eq!(config.size, 32 * 1024);
    }

    #[test]
    fn write_policy_defaults_to_none() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from(32768));

        let config = RomAttributes::from_attributes(&attributes).unwrap();

        assert!(config.write_policy.is_none());
    }

    #[test]
    fn write_policy_accepts_ignore() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from(32768));
        attributes.insert("write-policy".to_string(), Value::from("ignore"));

        let config = RomAttributes::from_attributes(&attributes).unwrap();

        assert!(matches!(config.write_policy, Some(WritePolicySpec::Ignore)));
    }

    #[test]
    fn write_policy_accepts_error() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from(32768));
        attributes.insert("write-policy".to_string(), Value::from("error"));

        let config = RomAttributes::from_attributes(&attributes).unwrap();

        assert!(matches!(config.write_policy, Some(WritePolicySpec::Error)));
    }

    #[test]
    fn write_policy_rejects_invalid_string() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from(32768));
        attributes.insert("write-policy".to_string(), Value::from("bogus"));

        assert!(RomAttributes::from_attributes(&attributes).is_err());
    }
}
