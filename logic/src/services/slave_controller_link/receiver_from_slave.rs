#![deny(unsafe_code)]


use crate::errors::Errors;
use crate::hal_ext::serial_transfer::Receiver;
use crate::services::slave_controller_link::domain::OperationCodes;
use crate::services::slave_controller_link::parsers::{PayloadParser, ResponseParser, PayloadParserResult, SignalParser, SignalPayload, ResponseBodyParser, ResponsePostParser, ResponseDataParser};
use crate::services::slave_controller_link::requests_controller::RequestsControllerRx;
use crate::services::slave_controller_link::signals_controller::SignalController;

pub trait ReceiverFromSlaveControllerAbstract<'a, Rc, SC, RCR, EH, PP, SP, RP, RBP, RPP>
    where
        Rc: Receiver,
        SC: SignalController<SP>,
        RCR: RequestsControllerRx<RP, RBP, RPP>,
        EH: Fn(Errors),
        PP: PayloadParser<'a, SP, RP, RBP, RPP>,
        SP: SignalParser,
        RP: ResponseParser<RBP, RPP>,
        RBP: ResponseBodyParser,
        RPP: ResponsePostParser + ResponseDataParser<RBP>,
{

    fn slice(&mut self) -> (&mut Rc, &mut SC, &mut RCR, &PP);
    fn error_handler(&mut self) -> &mut EH;

    fn on_get_command(&mut self) {
        let (rx, signal_controller, request_controller, parser) = self.slice();
        let res = rx.on_rx_transfer_interrupt(|data| {
            let payload = parser.parse(data)?;
            match payload {
                PayloadParserResult::ResponsePayload(response_payload) => {
                    request_controller.process_response(response_payload);
                    Ok(())
                }
                PayloadParserResult::SignalPayload(signal_payload) => {
                    signal_controller.process_signal(signal_payload);
                    Ok(())
                }
                _ => Err(Errors::UndefinedOperation),
            }
        });
        if res.is_err() {
            (self.error_handler())(res.err().unwrap());
        }
    }
}

pub struct ReceiverFromSlaveController<'a, Rc, SC, RCR, EH, PP, SP, RP, RBP, RPP>
    where
        Rc: Receiver,
        SC: SignalController<SP>,
        RCR: RequestsControllerRx<RP, RBP, RPP>,
        EH: Fn(Errors),
        PP: PayloadParser<'a, SP, RP, RBP, RPP>,
        SP: SignalParser,
        RP: ResponseParser<RBP, RPP>,
        RBP: ResponseBodyParser,
        RPP: ResponsePostParser + ResponseDataParser<RBP>,
{
    rx: Rc,
    signal_controller: SC,
    requests_controller_rx: RCR,
    error_handler: EH,
    payload_parser: PP,
    _signal_parser: core::marker::PhantomData<SP>,
    _response_parser: core::marker::PhantomData<&'a RP>,
    _response_body_parser: core::marker::PhantomData<RBP>,
    _response_post_parser: core::marker::PhantomData<RPP>,
}

impl <'a,  Rc, SC, RCR, EH, PP, SP, RP, RBP, RPP> ReceiverFromSlaveController<'a, Rc, SC, RCR, EH, PP, SP, RP, RBP, RPP>
    where
        Rc: Receiver,
        SC: SignalController<SP>,
        RCR: RequestsControllerRx<RP, RBP, RPP>,
        EH: Fn(Errors),
        PP: PayloadParser<'a, SP, RP, RBP, RPP>,
        SP: SignalParser,
        RP: ResponseParser<RBP, RPP>,
        RBP: ResponseBodyParser,
        RPP: ResponsePostParser + ResponseDataParser<RBP>,
{
    pub fn new(rx: Rc, signal_controller: SC, requests_controller_rx: RCR, error_handler: EH, payload_parser: PP) -> Self {
        Self {
            rx,
            signal_controller,
            requests_controller_rx,
            error_handler,
            payload_parser,
            _signal_parser: core::marker::PhantomData,
            _response_parser: core::marker::PhantomData,
            _response_body_parser: core::marker::PhantomData,
            _response_post_parser: core::marker::PhantomData,
        }
    }

    #[inline(always)]
    pub fn inner_rx(&mut self) -> &mut Rc {
        &mut self.rx
    }
}

impl <'a, Rc, RCR, SC, EH, PP, SP, RP, RBP, RPP> ReceiverFromSlaveControllerAbstract<'a, Rc, SC, RCR, EH, PP, SP, RP, RBP, RPP>
    for ReceiverFromSlaveController<'a, Rc, SC, RCR, EH, PP, SP, RP, RBP, RPP>
    where
        Rc: Receiver,
        SC: SignalController<SP>,
        RCR: RequestsControllerRx<RP, RBP, RPP>,
        EH: Fn(Errors),
        PP: PayloadParser<'a, SP, RP, RBP, RPP>,
        SP: SignalParser,
        RP: ResponseParser<RBP, RPP>,
        RBP: ResponseBodyParser,
        RPP: ResponsePostParser + ResponseDataParser<RBP>,
{
    #[inline(always)]
    fn slice(&mut self) ->( &mut Rc, &mut SC, &mut RCR, &PP) {
        (&mut self.rx, &mut self.signal_controller, &mut self.requests_controller_rx, &self.payload_parser)
    }

    #[inline(always)]
    fn error_handler(&mut self) -> &mut EH {
        &mut self.error_handler
    }

}


//
// #[cfg(test)]
// mod tests {
//     use alloc::vec::Vec;
//     use core::cell::{RefCell};
//     use rand::Rng;
//     use crate::services::slave_controller_link::domain::{DataInstructionCodes, DataInstructions, ErrorCode, Operation, Signals, Version};
//     use crate::services::slave_controller_link::parsers::{ResponsePayloadParsed, SignalParser, SignalParseResult, SignalPayload};
//     use super::*;
//
//
//     #[test]
//     fn test_on_get_command_should_report_rx_error() {
//         let mut rng = rand::thread_rng();
//         let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
//             rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)].to_vec();
//
//         let rx_error = Errors::DmaBufferOverflow;
//         let mock_receiver = MockReceiver::defected(rx_error);
//         let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
//         let mock_error_handler = |error: Errors| {
//             *handled_error.borrow_mut() = Some(error);
//         };
//
//         let error = Errors::NotEnoughDataGot;
//         let mock_parser = MockPayloadParser::new(Err(error));
//
//         let mut controller =
//             ReceiverFromSlaveController::new(mock_receiver, MockSignalController::new(),
//                                              MockRequestsControllerRx::new(), &mock_error_handler,mock_parser);
//
//         controller.on_get_command();
//
//         assert_eq!(Some(rx_error), *handled_error.borrow());
//         //nothing other should be called
//         assert_eq!(Some(data), controller.payload_parser.parse_params.borrow().map(|v| v.to_vec()));
//         assert_eq!(None, controller.requests_controller_rx.process_response_params);
//         assert!(controller.signal_controller.process_signal_params.is_none());
//     }
//
//     #[test]
//     fn test_on_get_command_should_report_parse_error() {
//         let mut rng = rand::thread_rng();
//         let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
//             rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)].to_vec();
//
//         let mock_receiver = MockReceiver::new(data.to_vec());
//         let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
//         let mock_error_handler = |error: Errors| {
//             *handled_error.borrow_mut() = Some(error);
//         };
//
//         let error = Errors::NotEnoughDataGot;
//         let mock_parser = MockPayloadParser::new(Err(error));
//
//         let mut controller =
//             ReceiverFromSlaveController::new(mock_receiver, MockSignalController::new(),
//                                              MockRequestsControllerRx::new(), &mock_error_handler,mock_parser);
//
//         controller.on_get_command();
//
//         assert_eq!(Err(error), controller.rx.receiver_result.unwrap());
//         assert_eq!(Some(data), controller.payload_parser.parse_params.borrow().map(|v| v.to_vec()));
//         assert_eq!(Some(error), *handled_error.borrow());
//         //nothing other should be called
//         assert_eq!(None, controller.requests_controller_rx.process_response_params);
//         assert!(controller.signal_controller.process_signal_params.is_none());
//     }
//
//     #[test]
//     fn test_on_get_command_should_call_request_controller_on_response_operations() {
//         let mut rng = rand::thread_rng();
//         let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
//             rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)].to_vec();
//
//         let mock_receiver = MockReceiver::new(data.to_vec());
//         let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
//         let mock_error_handler = |error: Errors| {
//             *handled_error.borrow_mut() = Some(error);
//         };
//
//         let mock_response_parser = MockResponseParser{};
//         let mock_parser = MockPayloadParser::new(
//             Ok(PayloadParserResult::ResponsePayload(mock_response_parser)));
//
//         let mut controller =
//             ReceiverFromSlaveController::new(mock_receiver, MockSignalController::new(),
//                                              MockRequestsControllerRx::new(), &mock_error_handler,mock_parser);
//
//         controller.on_get_command();
//
//         assert_eq!(Ok(()), controller.rx.receiver_result.unwrap());
//         assert_eq!(Some(data), controller.payload_parser.parse_params.borrow().map(|v| v.to_vec()));
//         assert_eq!(Some(mock_response_parser), controller.requests_controller_rx.process_response_params);
//         //nothing other should be called
//         assert_eq!(None, *handled_error.borrow());
//         assert!(controller.signal_controller.process_signal_params.is_none());
//     }
//
//     #[test]
//     fn test_on_get_command_should_proxy_signals() {
//         let mut rng = rand::thread_rng();
//         let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
//             rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)].to_vec();
//
//         let mock_receiver = MockReceiver::new(data.to_vec());
//         let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
//         let mock_error_handler = |error: Errors| {
//             *handled_error.borrow_mut() = Some(error);
//         };
//
//         let mock_signal_parser = MockSignalParser{};
//         let mock_parser = MockPayloadParser::new(
//             Ok(PayloadParserResult::SignalPayload(mock_signal_parser)));
//
//         let mut controller =
//             ReceiverFromSlaveController::new(mock_receiver, MockSignalController::new(),
//                 MockRequestsControllerRx::new(), &mock_error_handler,mock_parser);
//
//         controller.on_get_command();
//
//         assert_eq!(Ok(()), controller.rx.receiver_result.unwrap());
//         assert_eq!(Some(data), controller.payload_parser.parse_params.borrow().map(|v| v.to_vec()));
//         assert!(controller.signal_controller.process_signal_params.is_some());
//         assert_eq!(mock_signal_parser.data(), controller.signal_controller.process_signal_params.unwrap().data());
//         //nothing other should be called
//         assert_eq!(None, *handled_error.borrow());
//         assert_eq!(None, controller.requests_controller_rx.process_response_params);
//     }
//
//     struct MockReceiver {
//         data: Vec<u8>,
//         receiver_result: Option<Result<(), Errors>>,
//         call_proxy: bool,
//     }
//
//     impl MockReceiver {
//         pub fn new(data: Vec<u8>) -> Self {
//             Self {
//                 data,
//                 receiver_result: None,
//                 call_proxy: true,
//             }
//         }
//         pub fn defected(error: Errors) -> Self {
//             Self {
//                 data: Vec::new(),
//                 receiver_result: Some(Err(error)),
//                 call_proxy: false,
//             }
//         }
//     }
//
//     impl Receiver for MockReceiver {
//         fn on_rx_transfer_interrupt<F: FnOnce(&[u8]) -> Result<(), Errors>> (&mut self, receiver: F) -> Result<(), Errors> {
//             let res = receiver(&self.data.as_slice());
//             self.receiver_result = Some(res);
//             res
//         }
//     }
//     struct MockSignalController {
//         process_signal_params: Option<MockSignalParser>,
//     }
//
//     impl MockSignalController {
//         pub fn new() -> Self {
//             Self {
//                 process_signal_params: None,
//             }
//         }
//     }
//
//     impl SignalController<MockSignalParser> for MockSignalController {
//         fn process_signal(&mut self, parser: MockSignalParser) {
//             self.process_signal_params = Some(parser);
//         }
//     }
//
//     struct MockRequestsControllerRx {
//         process_response_params: Option<MockResponseParser>,
//     }
//
//     impl MockRequestsControllerRx {
//         pub fn new () -> Self {
//             Self {
//                 process_response_params: None,
//             }
//         }
//     }
//
//     impl <'a> RequestsControllerRx<'a, MockResponseParser, MockResponseBodyParser, MockResponsePostParser> for MockRequestsControllerRx {
//         fn process_response(&mut self, payload: MockResponseParser) {
//             self.process_response_params = Some(payload);
//         }
//     }
//
//     struct MockSignalParser {}
//
//     impl SignalParser for MockSignalParser {
//         fn parse(&self) -> Result<SignalParseResult, Errors> {
//             unimplemented!()
//         }
//
//         fn data(&self) -> &[u8] {
//             unimplemented!()
//         }
//     }
//
//     struct MockResponseBodyParser {}
//
//     #[derive(Debug, PartialEq, Copy, Clone)]
//     struct MockResponseParser {}
//
//     impl <'a> ResponseParser<MockResponseBodyParser, MockResponsePostParser> for MockResponseParser {
//         fn parse(&self, body_parser: &MockResponseBodyParser) -> Result<MockResponsePostParser, Errors> {
//             unimplemented!()
//         }
//
//         fn data(&self) -> &[u8] {
//             unimplemented!()
//         }
//     }
//
//     impl ResponseBodyParser for MockResponseBodyParser {
//         fn request_needs_cache(&self, instruction: DataInstructionCodes) -> bool {
//             unimplemented!()
//         }
//         fn parse_id<'a>(&self, data: &'a[u8]) -> Result<(Option<u32>, &'a[u8]), Errors> {
//             unimplemented!()
//         }
//         fn parse(&self, instruction: DataInstructionCodes, data: &[u8]) -> Result<DataInstructions, Errors> {
//             unimplemented!()
//         }
//         fn slave_controller_version(&self) -> Version {
//             unimplemented!()
//         }
//     }
//
//     struct MockResponsePostParser {
//
//     }
//
//     impl ResponsePostParser for MockResponsePostParser {
//         fn operation(&self) -> Operation {
//             unimplemented!()
//         }
//         fn instruction(&self) -> DataInstructionCodes {
//             unimplemented!()
//         }
//         fn request_id(&self) -> Option<u32> {
//             unimplemented!()
//         }
//         fn needs_cache(&self) -> bool {
//             unimplemented!()
//         }
//         fn error_code(&self) -> ErrorCode {
//             unimplemented!()
//         }
//         fn data(&self) -> &[u8] {
//             unimplemented!()
//         }
//     }
//
//     impl ResponseDataParser<MockResponseBodyParser> for MockResponsePostParser {
//         fn parse(&self, body_parser: &MockResponseBodyParser) -> Result<DataInstructions, Errors> {
//             unimplemented!()
//         }
//     }
//
//     struct MockPayloadParser<'a>  {
//         parse_result: Result<PayloadParserResult<'a, MockSignalParser, MockResponseParser, MockResponseBodyParser, MockResponsePostParser>, Errors>,
//         parse_params: RefCell<Option<Vec<u8>>>,
//     }
//
//     impl <'a> MockPayloadParser<'a>  {
//         fn new(parse_result: Result<PayloadParserResult<'a, MockSignalParser, MockResponseParser, MockResponseBodyParser, MockResponsePostParser>, Errors>) -> Self {
//             Self{
//                 parse_result,
//                 parse_params: RefCell::new(None),
//             }
//         }
//     }
//
//     impl <'a>  PayloadParser<'a, MockSignalParser, MockResponseParser, MockResponseBodyParser, MockResponsePostParser> for MockPayloadParser<'a> {
//         fn parse(&self, data: &[u8]) -> Result<PayloadParserResult<'a, MockSignalParser, MockResponseParser, MockResponseBodyParser, MockResponsePostParser>, Errors> {
//             *self.parse_params.borrow_mut() = Some(data.to_vec());
//             self.parse_result.as_ref()
//                 .map(|r| {
//                     match r {
//                         PayloadParserResult::ResponsePayload(_) => PayloadParserResult::ResponsePayload(MockResponseParser{}),
//                         PayloadParserResult::SignalPayload(_) => PayloadParserResult::SignalPayload(MockSignalParser{}),
//                         _ => unimplemented!(),
//                     }
//
//                 })
//                 .map_err(|e| e.clone())
//         }
//     }
//
//
// }