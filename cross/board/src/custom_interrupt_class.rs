use usb_device::class_prelude::*;
use usb_device::Result;
use usb_device::class_prelude::InterfaceNumber;

pub struct CustomInterruptClass<'a, B: UsbBus> {
    interface: InterfaceNumber,
    write_endpoint: EndpointIn<'a, B>,
}

impl<'a, B: UsbBus> CustomInterruptClass<'a, B> {
    pub fn new(alloc: &'a UsbBusAllocator<B>) -> CustomInterruptClass<'a, B> {
        CustomInterruptClass {
            interface: alloc.interface(),
            write_endpoint: alloc.interrupt(64, 1),
        }
    }

    pub fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.write_endpoint.write(data)
    }
}

impl<B: UsbBus> UsbClass<B> for CustomInterruptClass<'_, B> {
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        writer.interface(self.interface, 0x01, 0x00, 0x00)?;
        writer.endpoint(&self.write_endpoint)?;
        Ok(())
    }
}
