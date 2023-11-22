#![deny(unsafe_code)]

use crate::errors::Errors;
use crate::services::slave_controller_link::domain::Signals;
use crate::services::slave_controller_link::parsers::{ SignalParser, SignalParseResult };


pub trait SignalsHandler {
    fn on_signal(&mut self, signal_data: SignalParseResult);
    fn on_signal_parse_error(&mut self, error: Errors, sent_to_slave_success: bool, data: &[u8]);
    fn on_signal_process_error(&mut self, error: Errors, sent_to_slave_success: bool, data: SignalParseResult);
}

pub trait SignalController<'a, SP: SignalParser<'a>> {
    fn process_signal(&mut self, parser: SP);

}

pub struct SignalControllerImpl<SH>
    where
        SH: SignalsHandler,
{
    signal_handler: SH,
}

impl <SH: SignalsHandler> SignalControllerImpl<SH> {
    pub fn new(signal_handler: SH) -> Self {
        Self { signal_handler }
    }
}

impl <'a, SH, SP> SignalController<'a, SP> for SignalControllerImpl<SH>
    where
        SH: SignalsHandler,
        SP: SignalParser<'a>,
{
    fn process_signal<>(&mut self, payload: SP) {
        match payload.parse() {
            Ok(signal_data) => {
                self.signal_handler.on_signal(signal_data);
            }
            Err(error) => {
                self.signal_handler.on_signal_parse_error(error, false, payload.data());
            }
        }
    }
}



#[cfg(test)]
mod tests {
    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::cell::{ RefCell};
    use super::*;
    use rand::prelude::*;
    use crate::hal_ext::rtc_wrapper::RelativeSeconds;
    use crate::services::slave_controller_link::parsers::RelaySignalData;


    #[test]
    fn test_signal_controller_should_try_to_parse_signal_code_and_report_error() {
        let mut rng = rand::thread_rng();
        let mut signal_controller =
            SignalControllerImpl::new( MockSignalsHandler::new());
        let mut rng = rand::thread_rng();
        let error = Errors::CommandDataCorrupted;
        let data = [0_u8, rng.gen(), rng.gen(), rng.gen()];
        let mock_signal_parser = MockSignalParser::new(
            Err(error), data.to_vec());

        signal_controller.process_signal(mock_signal_parser);

        assert_eq!(Some((error, false, &data)),signal_controller.signal_handler.on_signal_error_params);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_handler.on_signal_signal_data);
    }

    #[test]
    fn test_signal_controller_should_proxy_parsed_signal_on_success() {
        let mut rng = rand::thread_rng();
        let mut signal_controller =
            SignalControllerImpl::new( MockSignalsHandler::new());
        let mut rng = rand::thread_rng();
        let result = SignalParseResult::new(Signals::MonitoringStateChanged, None);
        let data = [0_u8, rng.gen(), rng.gen(), rng.gen()];
        let mock_signal_parser = MockSignalParser::new(
            Ok(result), data.to_vec());

        signal_controller.process_signal(mock_signal_parser);

        assert_eq!(Some(result), signal_controller.signal_handler.on_signal_signal_data);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_handler.on_signal_error_params);
    }


    struct MockSignalsHandler {
        on_signal_signal_data: Option<SignalParseResult>,
        on_signal_error_params: Option<(Signals, Errors, bool)>,
    }

    impl MockSignalsHandler {
        fn new() -> Self {
            Self {
                on_signal_signal_data: None,
                on_signal_error_params: None,
            }
        }
    }

    impl SignalsHandler for MockSignalsHandler {
        fn on_signal(&mut self, signal_data: SignalParseResult) {
            self.on_signal_signal_data = Some(signal_data);
        }
        fn on_signal_parse_error(&mut self, instruction: Signals, error: Errors, sent: bool) {
            self.on_signal_error_params = Some((instruction, error, sent));
        }
    }

    impl SignalsHandler for Rc<RefCell<MockSignalsHandler>> {
        fn on_signal(&mut self, signal_data: SignalParseResult) {
            self.borrow_mut().on_signal(signal_data);
        }
        fn on_signal_parse_error(&mut self, instruction: Signals, error: Errors, sent: bool) {
            self.borrow_mut().on_signal_parse_error(instruction, error, sent);
        }
    }

    struct MockSignalParser {
        parse_result: Result<SignalParseResult, Errors>,
        test_data: Vec<u8>,
    }

    impl MockSignalParser {
        pub fn new(parse_result: Result<SignalParseResult, Errors>, test_data: Vec<u8>) -> Self {
            Self {
                parse_result,
                test_data,
            }
        }
    }

    impl <'a> SignalParser<'a> for MockSignalParser {

            fn parse(&self) -> Result<SignalParseResult, Errors> {
                self.parse_result
            }

            fn data(&self) -> &[u8] {
                self.test_data.into()
            }
    }
}