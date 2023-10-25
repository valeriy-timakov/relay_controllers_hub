#![deny(unsafe_code)]

use crate::errors::Errors;
use crate::services::slave_controller_link::domain::Signals;
use crate::services::slave_controller_link::parsers::{SignalData, SignalsParser};


pub trait SignalsHandler {
    fn on_signal(&mut self, signal_data: SignalData);
    fn on_signal_error(&mut self, instruction: Signals, error: Errors, sent_to_slave_success: bool);
}

pub trait SignalController {
    fn process_signal(&mut self, data: &[u8]);

}

pub struct SignalControllerImpl<SH, SP>
    where
        SH: SignalsHandler,
        SP: SignalsParser,
{
    signal_handler: SH,
    signal_parser: SP,
}

impl <SH, SP> SignalControllerImpl<SH, SP>
    where
        SH: SignalsHandler,
        SP: SignalsParser,
{
    pub fn new(signal_handler: SH, signal_parser: SP) -> Self {
        Self { signal_handler, signal_parser }
    }
}

impl <SH, SP> SignalController for SignalControllerImpl<SH, SP>
    where
        SH: SignalsHandler,
        SP: SignalsParser,
{
    fn process_signal(&mut self, data: &[u8]) {

        if data.len() < 1 {
            self.signal_handler.on_signal_error(Signals::Unknown, Errors::InvalidDataSize, false);
            return;
        }

        let instruction_code = data[0];
        let data = &data[1..];

        match self.signal_parser.parse_instruction(instruction_code) {
            Some(instruction) => {
                match self.signal_parser.parse(instruction, data) {
                    Ok(signal_data) => {
                        self.signal_handler.on_signal(signal_data);
                    }
                    Err(error) => {
                        self.signal_handler.on_signal_error(instruction, error, false);
                    }
                }
            }
            None => {
                self.signal_handler.on_signal_error(Signals::Unknown, Errors::InstructionNotRecognized(instruction_code), false);
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
    fn test_signal_controller_should_report_error_on_empty_data() {
        let mut signal_controller =
            SignalControllerImpl::new(MockSignalsHandler::new(),
                                      MockSignalsParser::new(Err(Errors::CommandDataCorrupted),
                                                             None));
        let data = [];

        signal_controller.process_signal(&data);

        assert_eq!(Some((Signals::Unknown, Errors::InvalidDataSize, false)),
                   signal_controller.signal_handler.on_signal_error_params);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_handler.on_signal_signal_data);
        assert_eq!(None, signal_controller.signal_parser.test_data.borrow().parse_instruction_params);
        assert_eq!(None, signal_controller.signal_parser.test_data.borrow().parse_params);
    }

    #[test]
    fn test_signal_controller_should_try_to_parse_signal_code_and_report_error() {
        let mock_signal_parser = MockSignalsParser::new(
            Err(Errors::CommandDataCorrupted), None);
        let mut signal_controller =
            SignalControllerImpl::new( MockSignalsHandler::new(), mock_signal_parser);
        let signal_code = Signals::MonitoringStateChanged as u8;
        let data = [signal_code, 1, 2, 3, 4, 5, 6, 7, 8, 9];

        signal_controller.process_signal(&data);

        assert_eq!(Some((Signals::Unknown, Errors::InstructionNotRecognized(signal_code), false)),
                   signal_controller.signal_handler.on_signal_error_params);
        //should call parse signal code
        assert_eq!(Some(signal_code as u8), signal_controller.signal_parser.test_data.borrow().parse_instruction_params);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_parser.test_data.borrow().parse_params);
        assert_eq!(None, signal_controller.signal_handler.on_signal_signal_data);
    }

    #[test]
    fn test_signal_controller_should_try_parse_signal_body_and_report_error_if_any() {
        let signal = Signals::MonitoringStateChanged;
        let signal_parse_error = Errors::CommandDataCorrupted;
        let mock_signal_parser = MockSignalsParser::new(
            Err(signal_parse_error), Some(signal));
        let mut signal_controller =
            SignalControllerImpl::new( MockSignalsHandler::new(), mock_signal_parser);
        let data = [signal as u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];

        signal_controller.process_signal(&data);

        assert_eq!(Some((signal, signal_parse_error, false)),
                   signal_controller.signal_handler.on_signal_error_params);
        //should call parse signal code
        assert_eq!(Some(signal as u8), signal_controller.signal_parser.test_data.borrow().parse_instruction_params);
        //should call parse signal body
        assert_eq!(Some((signal, (&data[1..]).to_vec())), signal_controller.signal_parser.test_data.borrow().parse_params);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_handler.on_signal_signal_data);

    }

    #[test]
    fn test_signal_controller_should_proxy_parsed_signal_on_success() {
        let signal = Signals::MonitoringStateChanged;
        let mut rng = rand::thread_rng();
        let parsed_signal_data = SignalData {
            instruction: signal,
            relay_signal_data: Some(RelaySignalData::new (
                RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                rng.gen_range(0..15),
                rng.gen_range(0..1) == 1,
                Some(rng.gen_range(0..1) == 1),
            )),
        };
        let mock_signal_parser = MockSignalsParser::new(Ok(parsed_signal_data), Some(signal));
        let mut signal_controller =
            SignalControllerImpl::new( MockSignalsHandler::new(), mock_signal_parser);
        let data = [signal as u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];

        signal_controller.process_signal(&data);

        assert_eq!(Some(parsed_signal_data), signal_controller.signal_handler.on_signal_signal_data);
        //should call parse signal code
        assert_eq!(Some(signal as u8), signal_controller.signal_parser.test_data.borrow().parse_instruction_params);
        //should call parse signal body
        assert_eq!(Some((signal, (&data[1..]).to_vec())), signal_controller.signal_parser.test_data.borrow().parse_params);
        //should not call other methods
        assert_eq!(None, signal_controller.signal_handler.on_signal_error_params);

    }


    struct MockSignalsParserTestData {
        parse_params: Option<(Signals, Vec<u8>)>,
        parse_instruction_params: Option<u8>,
    }

    impl MockSignalsParserTestData {
        fn new() -> Self {
            Self {
                parse_params : None,
                parse_instruction_params: None,
            }
        }
    }

    struct MockSignalsParser {
        parse_result: Result<SignalData, Errors>,
        parse_instruction_result: Option<Signals>,
        test_data: RefCell<MockSignalsParserTestData>,
    }

    impl MockSignalsParser {
        pub fn new(parse_result: Result<SignalData, Errors>, parse_instruction_result: Option<Signals>) -> Self {
            let test_data = RefCell::new(MockSignalsParserTestData::new());
            Self {
                parse_result,
                parse_instruction_result,
                test_data,
            }
        }
    }

    impl SignalsParser for MockSignalsParser {

        fn parse(&self, instruction: Signals, data: &[u8]) -> Result<SignalData, Errors> {
            self.test_data.borrow_mut().parse_params = Some((instruction, data.to_vec()));
            self.parse_result
        }


        fn parse_instruction(&self, instruction_code: u8) -> Option<Signals> {
            self.test_data.borrow_mut().parse_instruction_params = Some(instruction_code);
            self.parse_instruction_result
        }
    }

    struct MockSignalsHandler {
        on_signal_signal_data: Option<SignalData>,
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
        fn on_signal(&mut self, signal_data: SignalData) {
            self.on_signal_signal_data = Some(signal_data);
        }
        fn on_signal_error(&mut self, instruction: Signals, error: Errors, sent: bool) {
            self.on_signal_error_params = Some((instruction, error, sent));
        }
    }

    impl SignalsHandler for Rc<RefCell<MockSignalsHandler>> {
        fn on_signal(&mut self, signal_data: SignalData) {
            self.borrow_mut().on_signal(signal_data);
        }
        fn on_signal_error(&mut self, instruction: Signals, error: Errors, sent: bool) {
            self.borrow_mut().on_signal_error(instruction, error, sent);
        }
    }
}