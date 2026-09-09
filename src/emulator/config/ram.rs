use super::device::{deserialize_size, make_buffer};
use super::{DeviceModule, DeviceModuleError, ExpandedPathBuf, InstantiationContext, loader};
use crate::emulator::bus::{DeviceIdAllocator, symbol};
use crate::emulator::{AddressRange, BusConfig};
use figment::providers::Serialized;
use figment::value::{Dict, Value};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// Type name used in registering RAM as a device
const RAM_DEVICE_TYPE: &str = "ram";

/// RAM device module.
#[derive(Clone)]
pub struct RamModule;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RamAttributes {
    #[serde(deserialize_with = "deserialize_size")]
    size: u32,
    offset: Option<isize>,
    fill: Option<u8>,
    image: Option<ExpandedPathBuf>,
    labels: Option<ExpandedPathBuf>,
}

impl RamAttributes {
    fn from_attributes(attributes: &HashMap<String, Value>) -> Result<Self, DeviceModuleError> {
        let attrs = Dict::from_iter(attributes.clone());
        figment::Figment::new()
            .merge(Serialized::defaults(attrs))
            .extract()
            .map_err(|e| DeviceModuleError::Config(format!("configuration error: {e}")))
    }
}

impl DeviceModule for RamModule {
    fn name(&self) -> &'static str {
        RAM_DEVICE_TYPE
    }

    async fn instantiate(
        &self,
        bus_config: BusConfig,
        address: u16,
        attributes: &HashMap<String, Value>,
        _context: &InstantiationContext,
        _id_allocator: Arc<Mutex<DeviceIdAllocator>>,
    ) -> Result<BusConfig, DeviceModuleError> {
        let config = RamAttributes::from_attributes(attributes)?;
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

        if let Some(filename) = config.image {
            let mut data = make_buffer(config.size as usize, config.fill);
            loader::load_image(&filename, &mut data, offset)
                .await
                .map_err(DeviceModuleError::Load)?;
            bus_config
                .ram_with_data(range, data)
                .map_err(DeviceModuleError::BusConfig)
        } else if let Some(fill) = config.fill {
            bus_config
                .ram_with_fill(range, fill)
                .map_err(DeviceModuleError::BusConfig)
        } else {
            bus_config.ram(range).map_err(DeviceModuleError::BusConfig)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_accepts_plain_integer() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from(32768));

        let config = RamAttributes::from_attributes(&attributes).unwrap();

        assert_eq!(config.size, 32768);
    }

    #[test]
    fn size_accepts_kilobyte_shorthand_string() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from("32K"));

        let config = RamAttributes::from_attributes(&attributes).unwrap();

        assert_eq!(config.size, 32 * 1024);
    }

    #[test]
    fn size_accepts_lowercase_kilobyte_shorthand_string() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from("4k"));

        let config = RamAttributes::from_attributes(&attributes).unwrap();

        assert_eq!(config.size, 4 * 1024);
    }

    #[test]
    fn size_accepts_plain_decimal_string() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from("16384"));

        let config = RamAttributes::from_attributes(&attributes).unwrap();

        assert_eq!(config.size, 16384);
    }

    #[test]
    fn size_rejects_invalid_string() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from("not-a-size"));

        assert!(RamAttributes::from_attributes(&attributes).is_err());
    }
}
