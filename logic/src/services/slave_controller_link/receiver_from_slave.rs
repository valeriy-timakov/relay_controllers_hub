#![deny(unsafe_code)]

use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::RelativeTimestampSource;
use crate::hal_ext::serial_transfer::Receiver;
use crate::services::slave_controller_link::parsers::{PayloadParser, ResponseParser, PayloadParserResult, SignalParser};
use crate::services::slave_controller_link::requests_controller::RequestsControllerRx;
use crate::services::slave_controller_link::signals_controller::{ControlledRequestSender, SignalController};
use crate::services::slave_controller_link::transmitter_to_slave::ErrorsSender;

pub trait ReceiverFromSlaveControllerAbstract<Rc, SC, RCR, EH, PP, SP, RP>
    where
        Rc: Receiver,
        SC: SignalController<SP>,
        RCR: RequestsControllerRx<RP>,
        EH: ErrorHandler,
        PP: PayloadParser<SP, RP>,
        SP: SignalParser,
        RP: ResponseParser,
{

    fn slice(&mut self) -> (&mut Rc, &PP);
    fn error_handler(&mut self) -> &mut EH;

    //(&mut self, payload: SP, data: &[u8])
    fn on_get_command<TS: RelativeTimestampSource, S: ControlledRequestSender + ErrorsSender + RequestsControllerSource<RCR, RP>>(
            &mut self, signal_controller: &mut SC, sender:  &mut S, time_source: &mut TS) {

        let (rx, parser_factory) = self.slice();
        let res = rx.on_rx_transfer_interrupt(|data| {
            let (parser, data) = parser_factory.parse(data)?;
            match parser {
                PayloadParserResult::ResponsePayload(response_parser) => {
                    sender.requests_controller().process_response(response_parser, data);
                    Ok(())
                }
                PayloadParserResult::SignalPayload(signal_parser) => {
                    signal_controller.process_signal(signal_parser, data, time_source, sender);
                    Ok(())
                }
            }
        });
        if res.is_err() {
            self.error_handler().on_error(res.err().unwrap());
        }
    }
}

pub trait ErrorHandler {
    fn on_error(&mut self, error: Errors);
}

pub trait RequestsControllerSource<RCR: RequestsControllerRx<RP>, RP: ResponseParser> {
    fn requests_controller(&mut self) -> &mut RCR;
}

pub struct ReceiverFromSlaveController<Rc, EH, PP, SP, RP>
    where
        Rc: Receiver,
        EH: ErrorHandler,
        PP: PayloadParser<SP, RP>,
        SP: SignalParser,
        RP: ResponseParser,
{
    rx: Rc,
    error_handler: EH,
    payload_parser: PP,
    _signal_parser: core::marker::PhantomData<SP>,
    _response_parser: core::marker::PhantomData<RP>,
}

impl < Rc, EH, PP, SP, RP> ReceiverFromSlaveController<Rc, EH, PP, SP, RP>
    where
        Rc: Receiver,
        EH: ErrorHandler,
        PP: PayloadParser<SP, RP>,
        SP: SignalParser,
        RP: ResponseParser,
{
    pub fn new(rx: Rc, error_handler: EH, payload_parser: PP) -> Self {
        Self {
            rx,
            error_handler,
            payload_parser,
            _signal_parser: core::marker::PhantomData,
            _response_parser: core::marker::PhantomData,
        }
    }

    #[inline(always)]
    pub fn inner_rx(&mut self) -> &mut Rc {
        &mut self.rx
    }
}

impl <Rc, RCR, SC, EH, PP, SP, RP> ReceiverFromSlaveControllerAbstract<Rc, SC, RCR, EH, PP, SP, RP>
    for ReceiverFromSlaveController<Rc, EH, PP, SP, RP>
    where
        Rc: Receiver,
        SC: SignalController<SP>,
        RCR: RequestsControllerRx<RP>,
        EH: ErrorHandler,
        PP: PayloadParser<SP, RP>,
        SP: SignalParser,
        RP: ResponseParser,
{
    #[inline(always)]
    fn slice(&mut self) ->( &mut Rc, &PP) {
        (&mut self.rx, &self.payload_parser)
    }

    #[inline(always)]
    fn error_handler(&mut self) -> &mut EH {
        &mut self.error_handler
    }

}



#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use core::cell::{RefCell};
    use rand::Rng;
    use crate::hal_ext::rtc_wrapper::RelativeMillis;
    use crate::services::slave_controller_link::domain::{DataInstructionCodes, DataInstructions, ErrorCode, Operation, SignalData, Version};
    use crate::services::slave_controller_link::parsers::{ResponseBodyParser, ResponseData, SignalParser};
    use super::*;


    #[test]
    fn test_on_get_command_should_report_rx_error() {
        let mut rng = rand::thread_rng();

        let rx_error = Errors::DmaBufferOverflow;
        let mock_receiver = MockReceiver::defected(rx_error);
        let mock_error_handler = MockErrorHandler::new();

        let error = Errors::NotEnoughDataGot;
        let mock_parser = MockPayloadParser::new(Err(error));

        let mut signal_controller = MockSignalController::new();
        let request_controller_rx = MockRequestsControllerRx::new();
        let mut controller =
            ReceiverFromSlaveController::new(mock_receiver, mock_error_handler,mock_parser);
        let mut time_source = MockTimeSource::new(RelativeMillis::new(rng.gen_range(1..u32::MAX)));
        let mut mock_tx = MockSender::new(Ok(Some(rng.gen_range(1..u32::MAX))), Ok(()), request_controller_rx);

        controller.on_get_command(&mut signal_controller, &mut mock_tx, &mut time_source);

        assert_eq!(Some(rx_error),  controller.error_handler.on_error_params);
        //nothing other should be called
        assert_eq!(None, *controller.payload_parser.parse_params.borrow());
        assert_eq!(None, mock_tx.requests_controller.process_response_params);
        assert_eq!(None, signal_controller.process_signal_params);
    }

    #[test]
    fn test_on_get_command_should_report_parse_error() {
        let mut rng = rand::thread_rng();
        let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
            rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)].to_vec();

        let mock_receiver = MockReceiver::new(data.to_vec());
        let mock_error_handler = MockErrorHandler::new();

        let error = Errors::NotEnoughDataGot;
        let mock_parser = MockPayloadParser::new(Err(error));

        let mut signal_controller = MockSignalController::new();
        let request_controller_rx = MockRequestsControllerRx::new();
        let mut controller =
            ReceiverFromSlaveController::new(mock_receiver, mock_error_handler,mock_parser);
        let mut time_source = MockTimeSource::new(RelativeMillis::new(rng.gen_range(1..u32::MAX)));
        let mut mock_tx = MockSender::new(Ok(Some(rng.gen_range(1..u32::MAX))), Ok(()), request_controller_rx);

        controller.on_get_command(&mut signal_controller, &mut mock_tx, &mut time_source);

        assert_eq!(Some(data), *controller.payload_parser.parse_params.borrow());
        assert_eq!(Some(error),  controller.error_handler.on_error_params);
        //nothing other should be called
        assert_eq!(None, mock_tx.requests_controller.process_response_params);
        assert_eq!(None, signal_controller.process_signal_params);
    }

    #[test]
    fn test_on_get_command_should_call_request_controller_on_response_operations() {
        let mut rng = rand::thread_rng();
        let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
            rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)].to_vec();

        let mock_receiver = MockReceiver::new(data.to_vec());
        let mock_error_handler = MockErrorHandler::new();

        let mock_response_parser = MockResponseParser{};
        let mock_parser = MockPayloadParser::new(
            Ok(PayloadParserResult::ResponsePayload(mock_response_parser)));

        let mut signal_controller = MockSignalController::new();
        let request_controller_rx = MockRequestsControllerRx::new();
        let mut controller =
            ReceiverFromSlaveController::new(mock_receiver, mock_error_handler,mock_parser);
        let mut time_source = MockTimeSource::new(RelativeMillis::new(rng.gen_range(1..u32::MAX)));
        let mut mock_tx = MockSender::new(Ok(Some(rng.gen_range(1..u32::MAX))), Ok(()), request_controller_rx);

        controller.on_get_command(&mut signal_controller, &mut mock_tx, &mut time_source);

        assert_eq!(None,  controller.error_handler.on_error_params);
        assert_eq!(Some(data.clone()), *controller.payload_parser.parse_params.borrow());
        assert_eq!(Some((mock_response_parser, data.clone())), mock_tx.requests_controller.process_response_params);
        //nothing other should be called
        assert_eq!(None, signal_controller.process_signal_params);
    }

    #[test]
    fn test_on_get_command_should_proxy_signals() {
        let mut rng = rand::thread_rng();
        let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
            rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)].to_vec();

        let mock_receiver = MockReceiver::new(data.to_vec());
        let mock_error_handler = MockErrorHandler::new();

        let mock_signal_parser = MockSignalParser{};
        let mock_parser = MockPayloadParser::new(
            Ok(PayloadParserResult::SignalPayload(mock_signal_parser)));

        let mut signal_controller = MockSignalController::new();
        let request_controller_rx = MockRequestsControllerRx::new();
        let mut controller =
            ReceiverFromSlaveController::new(mock_receiver, mock_error_handler,mock_parser);
        let mut time_source = MockTimeSource::new(RelativeMillis::new(rng.gen_range(1..u32::MAX)));
        let mut mock_tx = MockSender::new(Ok(Some(rng.gen_range(1..u32::MAX))), Ok(()), request_controller_rx);

        controller.on_get_command(&mut signal_controller, &mut mock_tx, &mut time_source);

        assert_eq!(None, controller.error_handler.on_error_params);
        assert_eq!(Some(data.clone()), *controller.payload_parser.parse_params.borrow());
        assert!(signal_controller.process_signal_params.is_some());
        assert_eq!(Some((mock_signal_parser, data.clone())), signal_controller.process_signal_params);
        //nothing other should be called
        assert_eq!(None, mock_tx.requests_controller.process_response_params);
    }

    struct MockReceiver {
        data: Vec<u8>,
        receiver_error: Option<Errors>,
    }

    impl MockReceiver {
        pub fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                receiver_error: None,
            }
        }
        pub fn defected(error: Errors) -> Self {
            Self {
                data: Vec::new(),
                receiver_error: Some(error),
            }
        }
    }

    impl Receiver for MockReceiver {
        fn on_rx_transfer_interrupt<F: FnOnce(&[u8]) -> Result<(), Errors>> (&mut self, receiver: F) -> Result<(), Errors> {
            match self.receiver_error {
                Some(e) => Err(e),
                None => receiver(&self.data.as_slice()),
            }
        }
    }

    struct MockErrorHandler {
        on_error_params: Option<Errors>,
    }

    impl MockErrorHandler {
        pub fn new() -> Self {
            Self {
                on_error_params: None,
            }
        }
    }

    impl ErrorHandler for MockErrorHandler {
        fn on_error(&mut self, error: Errors) {
            self.on_error_params = Some(error);
        }
    }
    struct MockSignalController {
        process_signal_params: Option<(MockSignalParser, Vec<u8>)>,
    }

    impl MockSignalController {
        pub fn new() -> Self {
            Self {
                process_signal_params: None,
            }
        }
    }

    impl SignalController<MockSignalParser> for MockSignalController {
        fn process_signal<TS: RelativeTimestampSource, S: ControlledRequestSender + ErrorsSender>
            (&mut self, payload: MockSignalParser, data: &[u8], _: &mut TS, _:  &mut S)
        {
            //TODO add time_source and tx to params
            self.process_signal_params = Some((payload, data.to_vec()));
        }
    }

    struct MockRequestsControllerRx {
        process_response_params: Option<(MockResponseParser, Vec<u8>)>,
    }

    impl MockRequestsControllerRx {
        pub fn new () -> Self {
            Self {
                process_response_params: None,
            }
        }
    }

    impl RequestsControllerRx<MockResponseParser> for MockRequestsControllerRx {
        fn process_response(&mut self, payload: MockResponseParser, data: &[u8]) {
            self.process_response_params = Some((payload, data.to_vec()));
        }
    }

    #[derive(Debug, PartialEq, Copy, Clone)]
    struct MockSignalParser {}

    impl SignalParser for MockSignalParser {
        fn parse(&self, _: &[u8]) -> Result<SignalData, Errors> {
            unimplemented!()
        }
    }


    #[derive(Debug, PartialEq, Copy, Clone)]
    struct MockResponseBodyParser {}

    #[derive(Debug, PartialEq, Copy, Clone)]
    struct MockResponseParser {}

    impl ResponseParser for MockResponseParser {
        fn parse<'a>(&self, _: &'a[u8], _: Version) -> Result<(ResponseData, &'a[u8]), Errors> {
            unimplemented!()
        }
    }

    impl ResponseBodyParser for MockResponseBodyParser {
        fn request_needs_cache(&self, _: DataInstructionCodes) -> bool {
            unimplemented!()
        }
        fn parse(&self, _: DataInstructionCodes, _: &[u8]) -> Result<DataInstructions, Errors> {
            unimplemented!()
        }
    }

    struct MockPayloadParser  {
        parse_result: Result<PayloadParserResult<MockSignalParser, MockResponseParser>, Errors>,
        parse_params: RefCell<Option<Vec<u8>>>,
    }

    impl MockPayloadParser  {
        fn new(parse_result: Result<PayloadParserResult<MockSignalParser, MockResponseParser>, Errors>) -> Self {
            Self{
                parse_result,
                parse_params: RefCell::new(None),
            }
        }
    }

    impl PayloadParser<MockSignalParser, MockResponseParser> for MockPayloadParser {
        fn parse<'a>(&self, data: &'a[u8]) -> Result<(PayloadParserResult<MockSignalParser, MockResponseParser>, &'a[u8]), Errors> {
            *self.parse_params.borrow_mut() = Some(data.to_vec());
            self.parse_result.clone()
                .map(|r| {
                    match r {
                        PayloadParserResult::ResponsePayload(_) => (PayloadParserResult::ResponsePayload(MockResponseParser{}), data),
                        PayloadParserResult::SignalPayload(_) => (PayloadParserResult::SignalPayload(MockSignalParser{}), data),
                    }

                })
                .map_err(|e| e.clone())
        }
    }
    struct MockTimeSource {
        time_source_called: bool,
        time_source_result: RelativeMillis,
    }

    impl MockTimeSource {
        pub fn new(time_source_result: RelativeMillis) -> Self {
            Self {
                time_source_called: false,
                time_source_result,
            }
        }
    }

    impl RelativeTimestampSource for MockTimeSource {

        fn get(&mut self) ->RelativeMillis {
            self.time_source_called = true;
            self.time_source_result
        }
    }

    struct MockSender {
        send_params: Option<(Operation, DataInstructions, RelativeMillis)>,
        send_result: Result<Option<u32>, Errors>,
        send_error_params: Option<(u8, ErrorCode)>,
        send_error_result: Result<(), Errors>,
        requests_controller: MockRequestsControllerRx,
    }

    impl MockSender {
        pub fn new(send_result: Result<Option<u32>, Errors>, send_error_result: Result<(), Errors>,
                   requests_controller: MockRequestsControllerRx) -> Self {
            Self {
                send_params: None,
                send_result,
                send_error_params: None,
                send_error_result,
                requests_controller
            }
        }
    }

    impl ControlledRequestSender for MockSender {
        fn send(&mut self, operation: Operation, instruction: DataInstructions, timestamp: RelativeMillis) -> Result<Option<u32>, Errors> {
            self.send_params = Some((operation, instruction, timestamp));
            self.send_result
        }
    }

    impl RequestsControllerSource<MockRequestsControllerRx, MockResponseParser> for MockSender {
        fn requests_controller(&mut self) -> &mut MockRequestsControllerRx {
            &mut self.requests_controller
        }
    }

    impl ErrorsSender for MockSender {
        fn send_error(&mut self,  instruction_code: u8, error_code: ErrorCode) -> Result<(), Errors> {
            self.send_error_params = Some((instruction_code, error_code));
            self.send_error_result
        }
    }


}