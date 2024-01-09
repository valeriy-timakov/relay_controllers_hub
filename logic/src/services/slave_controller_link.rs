#![deny(unsafe_code)]

pub mod domain;
mod parsers;
mod requests_controller;
mod signals_controller;
mod transmitter_to_slave;
mod receiver_from_slave;
mod signals_handler_proxy;

use embedded_dma::{ReadBuffer, WriteBuffer};
use crate::services::slave_controller_link::domain::{*};
use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{RelativeMillis };
use crate::hal_ext::serial_transfer::{ ReadableBuffer, RxTransfer, RxTransferProxy, SerialTransfer, TxTransfer, TxTransferProxy};
use crate::services::slave_controller_link::parsers::{PayloadParserImpl, ResponseBodyParserImpl, ResponseParserImpl, SignalParserImpl};
use crate::services::slave_controller_link::receiver_from_slave::ReceiverFromSlaveController;
use crate::utils::dma_read_buffer::BufferWriter;
use crate::services::slave_controller_link::requests_controller::{RequestsController, ResponseHandler};
use crate::services::slave_controller_link::signals_controller::{SignalControllerImpl, SignalsHandler};
use crate::services::slave_controller_link::transmitter_to_slave::TransmitterToSlaveController;
use crate::services::slave_controller_link::receiver_from_slave::ReceiverFromSlaveControllerAbstract;



pub struct SlaveControllerLink<T, R, TxBuff, RxBuff, SH, RH, EH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        SH: SignalsHandler,
        RH: ResponseHandler,
        EH: Fn(Errors),
{
    tx: TransmitterToSlaveController<TxBuff, TxTransfer<T, TxBuff>>,
    rx: ReceiverFromSlaveController<
        RxTransfer<R, RxBuff>, SignalControllerImpl<SH>, RequestsController<RH, ResponseBodyParserImpl>,
        EH, PayloadParserImpl, SignalParserImpl, ResponseParserImpl
    >,
}


impl <'a, T, R, TxBuff, RxBuff, SH, RH, EH> SlaveControllerLink<T, R,TxBuff, RxBuff, SH, RH, EH>
    where
        TxBuff: ReadBuffer + BufferWriter,
        RxBuff: WriteBuffer + ReadableBuffer,
        T: TxTransferProxy<TxBuff>,
        R: RxTransferProxy<RxBuff>,
        SH: SignalsHandler,
        RH: ResponseHandler,
        EH: Fn(Errors),
{
    pub fn create(serial_transfer: SerialTransfer<T, R, TxBuff, RxBuff>, signals_handler: SH,
                  responses_handler: RH, receive_error_handler: EH, api_version: Version) -> Result<Self, Errors>
    {
        let (tx, rx) = serial_transfer.into();
        let response_body_parser = ResponseBodyParserImpl::create()?;
        let requests_controller = RequestsController::new(responses_handler,
                                                          response_body_parser, api_version);
        let signals_controller = SignalControllerImpl::new(signals_handler);
        let payload_parser = PayloadParserImpl::new();

        let rx = ReceiverFromSlaveController::new(rx, signals_controller,
                                                          requests_controller, receive_error_handler, payload_parser);
        Ok(Self {
            tx: TransmitterToSlaveController::new(tx),
            rx,
        })
    }

    #[inline(always)]
    pub fn on_get_command<E, TS:  FnOnce() -> RelativeMillis>( &mut self) {
        self.rx.on_get_command();
    }

    #[inline(always)]
    pub fn on_rx_dma_interrupts(&mut self) {
        self.rx.inner_rx().on_dma_interrupts();
    }

    #[inline(always)]
    pub fn on_tx_dma_interrupts(&mut self) {
        self.tx.inner_tx().on_dma_interrupts();
    }
}


