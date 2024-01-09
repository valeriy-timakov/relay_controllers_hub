#![deny(unsafe_code)]

use crate::errors::Errors;
use crate::hal_ext::serial_transfer::Receiver;
use crate::services::slave_controller_link::parsers::{PayloadParser, ResponseParser, PayloadParserResult, SignalParser};
use crate::services::slave_controller_link::requests_controller::RequestsControllerRx;
use crate::services::slave_controller_link::signals_controller::SignalController;

pub trait ReceiverFromSlaveControllerAbstract<Rc, SC, RCR, EH, PP, SP, RP>
    where
        Rc: Receiver,
        SC: SignalController<SP>,
        RCR: RequestsControllerRx<RP>,
        EH: Fn(Errors),
        PP: PayloadParser<SP, RP>,
        SP: SignalParser,
        RP: ResponseParser,
{

    fn slice(&mut self) -> (&mut Rc, &mut SC, &mut RCR, &PP);
    fn error_handler(&mut self) -> &mut EH;

    fn on_get_command(&mut self) {
        let (rx, signal_controller, request_controller, parser_factory) = self.slice();
        let res = rx.on_rx_transfer_interrupt(|data| {
            let (parser, data) = parser_factory.parse(data)?;
            match parser {
                PayloadParserResult::ResponsePayload(response_parser) => {
                    request_controller.process_response(response_parser, data);
                    Ok(())
                }
                PayloadParserResult::SignalPayload(signal_parser) => {
                    signal_controller.process_signal(signal_parser, data);
                    Ok(())
                }
            }
        });
        if res.is_err() {
            (self.error_handler())(res.err().unwrap());
        }
    }
}

pub struct ReceiverFromSlaveController<Rc, SC, RCR, EH, PP, SP, RP>
    where
        Rc: Receiver,
        SC: SignalController<SP>,
        RCR: RequestsControllerRx<RP>,
        EH: Fn(Errors),
        PP: PayloadParser<SP, RP>,
        SP: SignalParser,
        RP: ResponseParser,
{
    rx: Rc,
    signal_controller: SC,
    requests_controller_rx: RCR,
    error_handler: EH,
    payload_parser: PP,
    _signal_parser: core::marker::PhantomData<SP>,
    _response_parser: core::marker::PhantomData<RP>,
}

impl < Rc, SC, RCR, EH, PP, SP, RP> ReceiverFromSlaveController<Rc, SC, RCR, EH, PP, SP, RP>
    where
        Rc: Receiver,
        SC: SignalController<SP>,
        RCR: RequestsControllerRx<RP>,
        EH: Fn(Errors),
        PP: PayloadParser<SP, RP>,
        SP: SignalParser,
        RP: ResponseParser,
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
        }
    }

    #[inline(always)]
    pub fn inner_rx(&mut self) -> &mut Rc {
        &mut self.rx
    }
}

impl <Rc, RCR, SC, EH, PP, SP, RP> ReceiverFromSlaveControllerAbstract<Rc, SC, RCR, EH, PP, SP, RP>
    for ReceiverFromSlaveController<Rc, SC, RCR, EH, PP, SP, RP>
    where
        Rc: Receiver,
        SC: SignalController<SP>,
        RCR: RequestsControllerRx<RP>,
        EH: Fn(Errors),
        PP: PayloadParser<SP, RP>,
        SP: SignalParser,
        RP: ResponseParser,
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



#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use core::cell::{RefCell};
    use rand::Rng;
    use crate::services::slave_controller_link::domain::{DataInstructionCodes, DataInstructions, SignalData, Version};
    use crate::services::slave_controller_link::parsers::{ResponseBodyParser, ResponseData, SignalParser};
    use super::*;


    #[test]
    fn test_on_get_command_should_report_rx_error() {
        let mut rng = rand::thread_rng();
        let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
            rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)].to_vec();

        let rx_error = Errors::DmaBufferOverflow;
        let mock_receiver = MockReceiver::defected(rx_error);
        let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
        let mock_error_handler = |error: Errors| {
            *handled_error.borrow_mut() = Some(error);
        };

        let error = Errors::NotEnoughDataGot;
        let mock_parser = MockPayloadParser::new(Err(error));

        let mut controller =
            ReceiverFromSlaveController::new(mock_receiver, MockSignalController::new(),
                                             MockRequestsControllerRx::new(), &mock_error_handler,mock_parser);

        controller.on_get_command();

        assert_eq!(Some(rx_error), *handled_error.borrow());
        //nothing other should be called
        assert_eq!(None, *controller.payload_parser.parse_params.borrow());
        assert_eq!(None, controller.requests_controller_rx.process_response_params);
        assert_eq!(None, controller.signal_controller.process_signal_params);
    }

    #[test]
    fn test_on_get_command_should_report_parse_error() {
        let mut rng = rand::thread_rng();
        let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
            rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)].to_vec();

        let mock_receiver = MockReceiver::new(data.to_vec());
        let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
        let mock_error_handler = |error: Errors| {
            *handled_error.borrow_mut() = Some(error);
        };

        let error = Errors::NotEnoughDataGot;
        let mock_parser = MockPayloadParser::new(Err(error));

        let mut controller =
            ReceiverFromSlaveController::new(mock_receiver, MockSignalController::new(),
                                             MockRequestsControllerRx::new(), &mock_error_handler,mock_parser);

        controller.on_get_command();

        assert_eq!(Some(data), *controller.payload_parser.parse_params.borrow());
        assert_eq!(Some(error), *handled_error.borrow());
        //nothing other should be called
        assert_eq!(None, controller.requests_controller_rx.process_response_params);
        assert_eq!(None, controller.signal_controller.process_signal_params);
    }

    #[test]
    fn test_on_get_command_should_call_request_controller_on_response_operations() {
        let mut rng = rand::thread_rng();
        let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
            rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)].to_vec();

        let mock_receiver = MockReceiver::new(data.to_vec());
        let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
        let mock_error_handler = |error: Errors| {
            *handled_error.borrow_mut() = Some(error);
        };

        let mock_response_parser = MockResponseParser{};
        let mock_parser = MockPayloadParser::new(
            Ok(PayloadParserResult::ResponsePayload(mock_response_parser)));

        let mut controller =
            ReceiverFromSlaveController::new(mock_receiver, MockSignalController::new(),
                                             MockRequestsControllerRx::new(), &mock_error_handler,mock_parser);

        controller.on_get_command();

        assert_eq!(None, *handled_error.borrow());
        assert_eq!(Some(data.clone()), *controller.payload_parser.parse_params.borrow());
        assert_eq!(Some((mock_response_parser, data.clone())), controller.requests_controller_rx.process_response_params);
        //nothing other should be called
        assert_eq!(None, controller.signal_controller.process_signal_params);
    }

    #[test]
    fn test_on_get_command_should_proxy_signals() {
        let mut rng = rand::thread_rng();
        let data = [rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX),
            rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)].to_vec();

        let mock_receiver = MockReceiver::new(data.to_vec());
        let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
        let mock_error_handler = |error: Errors| {
            *handled_error.borrow_mut() = Some(error);
        };

        let mock_signal_parser = MockSignalParser{};
        let mock_parser = MockPayloadParser::new(
            Ok(PayloadParserResult::SignalPayload(mock_signal_parser)));

        let mut controller =
            ReceiverFromSlaveController::new(mock_receiver, MockSignalController::new(),
                MockRequestsControllerRx::new(), &mock_error_handler,mock_parser);

        controller.on_get_command();

        assert_eq!(None, *handled_error.borrow());
        assert_eq!(Some(data.clone()), *controller.payload_parser.parse_params.borrow());
        assert!(controller.signal_controller.process_signal_params.is_some());
        assert_eq!(Some((mock_signal_parser, data.clone())), controller.signal_controller.process_signal_params);
        //nothing other should be called
        assert_eq!(None, controller.requests_controller_rx.process_response_params);
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
        fn process_signal(&mut self, parser: MockSignalParser, data: &[u8]) {
            self.process_signal_params = Some((parser, data.to_vec()));
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
        fn parse(&self, data: &[u8]) -> Result<SignalData, Errors> {
            unimplemented!()
        }
    }


    #[derive(Debug, PartialEq, Copy, Clone)]
    struct MockResponseBodyParser {}

    #[derive(Debug, PartialEq, Copy, Clone)]
    struct MockResponseParser {}

    impl ResponseParser for MockResponseParser {
        fn parse<'a>(&self, data: &'a[u8], slave_controller_version: Version) -> Result<(ResponseData, &'a[u8]), Errors> {
            unimplemented!()
        }
    }

    impl ResponseBodyParser for MockResponseBodyParser {
        fn request_needs_cache(&self, instruction: DataInstructionCodes) -> bool {
            unimplemented!()
        }
        fn parse(&self, instruction: DataInstructionCodes, data: &[u8]) -> Result<DataInstructions, Errors> {
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


}