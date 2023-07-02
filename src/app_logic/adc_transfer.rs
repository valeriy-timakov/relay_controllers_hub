#![deny(unsafe_code)]
#![deny(warnings)]

use stm32f4xx_hal::{
    pac::{  DMA2, ADC1 },
    dma::{
        config::DmaConfig, traits::StreamISR, PeripheralToMemory, Stream0,
        Transfer, StreamX
    },
    adc::{
        config::{AdcConfig, Dma, SampleTime, Scan, Sequence},
        Adc, Temperature,
    },
    signature::{VtempCal110, VtempCal30},
};


const BUFFER_SIZE: usize = 6;
pub type AdcBuffer = [u16; BUFFER_SIZE];

type ADCDMATransfer =
Transfer<Stream0<DMA2>, 0, Adc<ADC1>, PeripheralToMemory, &'static mut AdcBuffer>;

pub struct ADCTransfer {
    adc_transfer: ADCDMATransfer,
    back_buffer: Option<&'static mut AdcBuffer>,
    //fifo_error: bool,
}

impl ADCTransfer {
    pub fn new<CHANNEL>(
        dma_stream: StreamX<DMA2, 0>,
        adc1: ADC1,
        voltage_pin: CHANNEL
    ) -> Self
        where CHANNEL: embedded_hal::adc::Channel<ADC1, ID=u8>
    {

        let adc_config = AdcConfig::default()
            .dma(Dma::Continuous)
            .scan(Scan::Enabled);
        let mut adc = Adc::adc1(adc1, true, adc_config);

        adc.configure_channel(&Temperature, Sequence::One, SampleTime::Cycles_56);
        adc.configure_channel(&voltage_pin, Sequence::Two, SampleTime::Cycles_56);
        adc.enable_temperature_and_vref();
        let first_buffer = cortex_m::singleton!(: AdcBuffer = [0; BUFFER_SIZE]).unwrap();
        let second_buffer = cortex_m::singleton!(: AdcBuffer = [0; BUFFER_SIZE]).unwrap();
        let config = DmaConfig::default()
            .transfer_complete_interrupt(true)
            .memory_increment(true)
            .double_buffer(false);
        let adc_transfer: ADCDMATransfer<> = Transfer::init_peripheral_to_memory(dma_stream,
            adc, first_buffer, None, config);

        Self {
            adc_transfer,
            back_buffer: Some(second_buffer),
            //fifo_error: false,
        }
    }

    pub fn start_measurement(&mut self) {
        self.adc_transfer.start(|adc| {
            adc.start_conversion();
        });
    }

    pub fn get_results(&mut self) -> Option<(impl Fn(u16)->u16, &'static mut AdcBuffer)> {
        if Stream0::<DMA2>::get_transfer_complete_flag() {
            self.adc_transfer.clear_transfer_complete_interrupt();
            // When the DMA completes it will return the buffer we gave it last time - we now store that as `buffer`
            // We still have our other buffer waiting in `local.buffer`, so `take` that and give it to the `transfer`
            let back_buffer = self.back_buffer.take().unwrap();
            let (buffer, _) = self.adc_transfer.next_transfer(back_buffer).unwrap();
            Some((self.adc_transfer.peripheral().make_sample_to_millivolts(), buffer))
        } else {
            None
        }
    }

    pub fn return_buffer(&mut self, buffer: &'static mut AdcBuffer) {
        self.back_buffer = Some(buffer);
    }

    pub fn get_last_data(sample_to_millivolts: impl Fn(u16)->u16, buffer: &AdcBuffer) -> (f32, u16) {


        // Pull the ADC data out of the buffer that the DMA transfer gave us
        let raw_temp = buffer[0];
        let raw_volt = buffer[1];


        let cal30 = VtempCal30::get().read() as f32;
        let cal110 = VtempCal110::get().read() as f32;

        let temperature = (110.0 - 30.0) * ((raw_temp as f32) - cal30) / (cal110 - cal30) + 30.0;
        let voltage = sample_to_millivolts(raw_volt);
        (temperature, voltage)
    }

}
