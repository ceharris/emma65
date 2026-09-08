use super::device::parse_suffixed_u32;
use super::write_policy::WritePolicySpec;
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

// Type name used in registering ROM as a device
const ROM_DEVICE_TYPE: &str = "rom";

/// RAM device module.
#[derive(Clone)]
pub struct RamModule;

/// ROM device module.
#[derive(Clone)]
pub struct RomModule;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryAttributes {
    #[serde(deserialize_with = "deserialize_size")]
    size: u32,
    offset: Option<isize>,
    fill: Option<u8>,
    image: Option<ExpandedPathBuf>,
    labels: Option<ExpandedPathBuf>,
}

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

/// Accepts `size` either as a plain integer (as it would already be after parsing a `--device`
/// CLI argument) or as a string such as `"32K"` (the shorthand form TOML configuration carries
/// through as a string, since figment doesn't parse suffixed strings into numbers on its own).
fn deserialize_size<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct SizeVisitor;

    impl serde::de::Visitor<'_> for SizeVisitor {
        type Value = u32;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an integer byte count, or a string such as \"32K\"")
        }

        fn visit_u64<E>(self, v: u64) -> Result<u32, E>
        where
            E: serde::de::Error,
        {
            u32::try_from(v).map_err(|_| E::custom(format!("size {v} is out of range")))
        }

        fn visit_i64<E>(self, v: i64) -> Result<u32, E>
        where
            E: serde::de::Error,
        {
            u32::try_from(v).map_err(|_| E::custom(format!("size {v} is out of range")))
        }

        fn visit_str<E>(self, v: &str) -> Result<u32, E>
        where
            E: serde::de::Error,
        {
            parse_suffixed_u32(v).map_err(|_| E::custom(format!("invalid size: \"{v}\"")))
        }
    }

    deserializer.deserialize_any(SizeVisitor)
}

impl MemoryAttributes {
    fn from_attributes(attributes: &HashMap<String, Value>) -> Result<Self, DeviceModuleError> {
        let attrs = Dict::from_iter(attributes.clone());
        figment::Figment::new()
            .merge(Serialized::defaults(attrs))
            .extract()
            .map_err(|e| DeviceModuleError::Config(format!("configuration error: {e}")))
    }
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

pub fn make_buffer(size: usize, fill_value: Option<u8>) -> Vec<u8> {
    match fill_value {
        Some(v) => vec![v; size],
        None => (0..size).map(|_| rand::random::<u8>()).collect(),
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
        let config = MemoryAttributes::from_attributes(attributes)?;
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
    fn size_accepts_plain_integer() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from(32768));

        let config = MemoryAttributes::from_attributes(&attributes).unwrap();

        assert_eq!(config.size, 32768);
    }

    #[test]
    fn size_accepts_kilobyte_shorthand_string() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from("32K"));

        let config = MemoryAttributes::from_attributes(&attributes).unwrap();

        assert_eq!(config.size, 32 * 1024);
    }

    #[test]
    fn size_accepts_lowercase_kilobyte_shorthand_string() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from("4k"));

        let config = MemoryAttributes::from_attributes(&attributes).unwrap();

        assert_eq!(config.size, 4 * 1024);
    }

    #[test]
    fn size_accepts_plain_decimal_string() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from("16384"));

        let config = MemoryAttributes::from_attributes(&attributes).unwrap();

        assert_eq!(config.size, 16384);
    }

    #[test]
    fn size_rejects_invalid_string() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from("not-a-size"));

        assert!(MemoryAttributes::from_attributes(&attributes).is_err());
    }

    #[test]
    fn rom_write_policy_defaults_to_none() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from(32768));

        let config = RomAttributes::from_attributes(&attributes).unwrap();

        assert!(config.write_policy.is_none());
    }

    #[test]
    fn rom_write_policy_accepts_ignore() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from(32768));
        attributes.insert("write-policy".to_string(), Value::from("ignore"));

        let config = RomAttributes::from_attributes(&attributes).unwrap();

        assert!(matches!(config.write_policy, Some(WritePolicySpec::Ignore)));
    }

    #[test]
    fn rom_write_policy_accepts_error() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from(32768));
        attributes.insert("write-policy".to_string(), Value::from("error"));

        let config = RomAttributes::from_attributes(&attributes).unwrap();

        assert!(matches!(config.write_policy, Some(WritePolicySpec::Error)));
    }

    #[test]
    fn rom_write_policy_rejects_invalid_string() {
        let mut attributes = HashMap::new();
        attributes.insert("size".to_string(), Value::from(32768));
        attributes.insert("write-policy".to_string(), Value::from("bogus"));

        assert!(RomAttributes::from_attributes(&attributes).is_err());
    }
}
