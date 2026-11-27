use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use figment::value::Value;

use super::{DeviceModule, DeviceModuleError, InstantiationContext};
use crate::bus::DeviceIdAllocator;
use crate::device::PicFinch;
use crate::{AddressRange, BusConfig};

/// Number of bytes this device occupies in the bus address space: just the IER register.
/// The 16-byte vector table at `0xFFE0`-`0xFFEF` is expected to be backed by ROM and is
/// not claimed by the PIC itself — see [`PicFinch`]'s module docs.
const BUS_SIZE: u16 = 1;

/// Device module that registers a [`PicFinch`] as a bus device and installs its
/// [`VectorResolver`](crate::VectorResolver) on the `Cpu` being built. Accepts
/// no device-specific attributes.
#[derive(Clone)]
pub struct PicFinchModule;

impl DeviceModule for PicFinchModule {
    fn name(&self) -> &'static str {
        "pic/finch"
    }

    async fn instantiate(
        &self,
        bus_config: BusConfig,
        address: u16,
        _attributes: &HashMap<String, Value>,
        context: &InstantiationContext,
        id_allocator: Arc<Mutex<DeviceIdAllocator>>,
    ) -> Result<BusConfig, DeviceModuleError> {
        let device_id = id_allocator.lock().unwrap().next_available();
        let mut device = PicFinch::new(self.name()).with_address(address);
        if let Some(sender) = &context.log_sender {
            device.set_log_sender(sender.clone());
        }
        let resolver = device.vector_resolver();

        let bus_config = bus_config
            .device(
                AddressRange::new(address, address + (BUS_SIZE - 1)),
                device_id,
                Box::new(device),
            )
            .map_err(DeviceModuleError::BusConfig)?;

        bus_config
            .vector_resolver(Box::new(resolver))
            .map_err(DeviceModuleError::BusConfig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> InstantiationContext {
        InstantiationContext::default()
    }

    #[tokio::test]
    async fn instantiate_registers_ier_as_a_bus_device() {
        let module = PicFinchModule;
        let attributes: HashMap<String, Value> = HashMap::new();
        let id_allocator = Arc::new(Mutex::new(DeviceIdAllocator::new()));
        let bus_config = module
            .instantiate(
                BusConfig::new(),
                0xFFFF,
                &attributes,
                &context(),
                id_allocator,
            )
            .await
            .unwrap();
        let mut bus = bus_config.build();
        bus.write(0xFFFF, !(1 << 2) & 0x7F).unwrap(); // disable all but slot 2
        assert_eq!(bus.read(0xFFFF).unwrap(), 0x80 | (1 << 2));
    }
}
