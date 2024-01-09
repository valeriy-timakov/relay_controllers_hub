#![deny(unsafe_code)]

use crate::errors::Errors;
use crate::services::slave_controller_link::domain::SignalData;
use crate::services::slave_controller_link::parsers::SignalParser;


pub trait SignalsHandler {
    fn on_signal(&mut self, signal_data: SignalData);
    fn on_signal_parse_error(&mut self, error: Errors, sent_to_slave_success: bool, data: &[u8]);
    fn on_signal_process_error(&mut self, error: Errors, sent_to_slave_success: bool, data: SignalData);
}

pub trait SignalController<SP: SignalParser> {
    fn process_signal(&mut self, parser: SP, data: &[u8]);

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

impl <SH, SP> SignalController<SP> for SignalControllerImpl<SH>
    where
        SH: SignalsHandler,
        SP: SignalParser,
{
    fn process_signal<>(&mut self, payload: SP, data: &[u8]) {
        match payload.parse(data) {
            Ok(signal_data) => {
                self.signal_handler.on_signal(signal_data);
            }
            Err(error) => {
                self.signal_handler.on_signal_parse_error(error, false, data);
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
    use crate::services::slave_controller_link::domain::SignalData;


    #[test]
    fn test_signal_controller_should_try_to_parse_signal_code_and_report_error() {
        let mut rng = rand::thread_rng();
        let mut signal_controller =
            SignalControllerImpl::new( MockSignalsHandler::new());
        let mut rng = rand::thread_rng();
        let error = Errors::CommandDataCorrupted;
        let data = [0_u8, rng.gen(), rng.gen(), rng.gen()];
        let mock_signal_parser = MockSignalParser::new(Err(error));

        signal_controller.process_signal(mock_signal_parser, data.as_slice());

        assert_eq!(Some((error, false, data.to_vec())), signal_controller.signal_handler.on_signal_error_params);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_handler.on_signal_signal_data);
    }

    #[test]
    fn test_signal_controller_should_proxy_parsed_signal_on_success() {
        let mut rng = rand::thread_rng();
        let mut signal_controller =
            SignalControllerImpl::new( MockSignalsHandler::new());
        let mut rng = rand::thread_rng();
        let result = SignalData::GetTimeStamp;
        let data = [0_u8, rng.gen(), rng.gen(), rng.gen()];
        let mock_signal_parser = MockSignalParser::new(Ok(result));

        signal_controller.process_signal(mock_signal_parser, data.as_slice());

        assert_eq!(Some(result), signal_controller.signal_handler.on_signal_signal_data);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_handler.on_signal_error_params);
    }


    struct MockSignalsHandler {
        on_signal_signal_data: Option<SignalData>,
        on_signal_error_params: Option<(Errors, bool, Vec<u8>)>,
        on_signal_process_error: Option<(Errors, bool, SignalData)>,
    }

    impl MockSignalsHandler {
        fn new() -> Self {
            Self {
                on_signal_signal_data: None,
                on_signal_error_params: None,
                on_signal_process_error: None,
            }
        }
    }

    impl SignalsHandler for MockSignalsHandler {
        fn on_signal(&mut self, signal_data: SignalData) {
            self.on_signal_signal_data = Some(signal_data);
        }
        fn on_signal_parse_error(&mut self, error: Errors, sent_to_slave_success: bool, data: &[u8]) {
            self.on_signal_error_params = Some((error, sent_to_slave_success, data.to_vec()));
        }

        fn on_signal_process_error(&mut self, error: Errors, sent_to_slave_success: bool, data: SignalData) {
            self.on_signal_process_error = Some((error, sent_to_slave_success, data));
        }


    }

    impl SignalsHandler for Rc<RefCell<MockSignalsHandler>> {
        fn on_signal(&mut self, signal_data: SignalData) {
            self.borrow_mut().on_signal(signal_data);
        }
        fn on_signal_parse_error(&mut self, error: Errors, sent_to_slave_success: bool, data: &[u8]) {
            self.borrow_mut().on_signal_parse_error(error, sent_to_slave_success, data);
        }

        fn on_signal_process_error(&mut self, error: Errors, sent_to_slave_success: bool, data: SignalData) {
            self.borrow_mut().on_signal_process_error(error, sent_to_slave_success, data);
        }
    }

    struct MockSignalParser {
        parse_result: Result<SignalData, Errors>,
        parse_params: RefCell<Option<Vec<u8>>>,
    }

    impl MockSignalParser {
        pub fn new(parse_result: Result<SignalData, Errors>) -> Self {
            Self {
                parse_result,
                parse_params: RefCell::new(None)
            }
        }
    }

    impl SignalParser for MockSignalParser {
            fn parse(&self, data: &[u8]) -> Result<SignalData, Errors> {
                *self.parse_params.borrow_mut() = Some(data.to_vec());
                self.parse_result
            }
    }
}