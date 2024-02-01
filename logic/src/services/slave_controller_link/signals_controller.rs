#![deny(unsafe_code)]

use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{RelativeMillis, RelativeTimestampSource};
use crate::services::slave_controller_link::domain::{Conversation, DataInstructions, ErrorCode, Operation, SignalData, Signals};
use crate::services::slave_controller_link::parsers::SignalParser;
use crate::services::slave_controller_link::transmitter_to_slave::ErrorsSender;


pub trait SignalsHandler {
    fn on_signal(&mut self, signal_data: SignalData, processed_successfully: bool);
    fn on_signal_parse_error(&mut self, error: Errors, sent_to_slave_success: bool, data: &[u8]);
    fn on_signal_process_error(&mut self, error: Errors, sent_to_slave_success: bool, data: SignalData);
}
pub trait ControlledRequestSender {
    fn send(&mut self, operation: Operation, instruction: DataInstructions, timestamp: RelativeMillis) -> Result<Option<u32>, Errors>;
}

pub trait SignalController<SP: SignalParser> {
    fn process_signal<TS: RelativeTimestampSource, S: ControlledRequestSender + ErrorsSender>(&mut self, payload: SP, data: &[u8], time_source: &mut TS, tx:  &mut S);

}

trait SignalsPreHandler {
    fn on_signal<TS: RelativeTimestampSource, S: ControlledRequestSender + ErrorsSender>
        (&mut self, signal_data: SignalData, time_source: &mut TS, tx:  &mut S);
    fn on_signal_parse_error<S: ControlledRequestSender + ErrorsSender>(&mut self, error: Errors, data: &[u8], tx:  &mut S);
}

impl <SP: SignalParser, SPH: SignalsPreHandler> SignalController<SP> for SPH {
    fn process_signal<TS: RelativeTimestampSource, S: ControlledRequestSender + ErrorsSender>
        (&mut self, payload: SP, data: &[u8], time_source: &mut TS, tx:  &mut S)
    {
        match payload.parse(data) {
            Ok(signal_data) => {
                self.on_signal(signal_data, time_source, tx);
            }
            Err(error) => {
                self.on_signal_parse_error(error, data, tx);
            }
        }
    }
}

pub struct SignalControllerImpl<SH: SignalsHandler> {
    signal_handler: SH,
}

impl <SH: SignalsHandler> SignalControllerImpl<SH> {
    pub fn new(signal_handler: SH) -> Self {
        Self { signal_handler }
    }
}

impl <SH: SignalsHandler> SignalsPreHandler for SignalControllerImpl<SH> {

    fn on_signal<TS: RelativeTimestampSource, S: ControlledRequestSender + ErrorsSender>(&mut self, signal_data: SignalData, time_source: &mut TS, tx:  &mut S) {
        if signal_data.code() == Signals::GetTimeStamp {
            let timestamp = time_source.get();
            let res = tx.send(
                Operation::Set,
                DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())),
                timestamp);
            if res.is_err() {
                self.signal_handler.on_signal_process_error(res.err().unwrap(), false, signal_data);
            } else {
                self.signal_handler.on_signal(signal_data, true);
            }
        } else {
            self.signal_handler.on_signal(signal_data, false);
        }
    }

    fn on_signal_parse_error<S: ControlledRequestSender + ErrorsSender>(&mut self, error: Errors, data: &[u8], tx:  &mut S) {
        let error_code = ErrorCode::for_error(error);
        let instruction = if !data.is_empty() { data[0] }  else { Signals::None as u8 };
        let sent_to_slave_success = tx.send_error(instruction, error_code).is_ok();
        self.signal_handler.on_signal_parse_error(error, sent_to_slave_success, data);
    }
}


#[cfg(test)]
mod tests {
    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::cell::{ RefCell};
    use super::*;
    use rand::prelude::*;
    use crate::services::slave_controller_link::domain::{EmptyRequest, SignalData};
    use crate::errors::DMAError;
    use crate::hal_ext::rtc_wrapper::RelativeSeconds;
    use crate::services::slave_controller_link::domain::{RelaySignalData, RelaySignalDataExt};


    #[test]
    fn test_signal_controller_should_try_to_parse_signal_code_and_report_error() {
        let mut rng = rand::thread_rng();
        let mut signal_controller = MockSignalController::new( );
        let error = Errors::CommandDataCorrupted;
        let data = [0_u8, rng.gen(), rng.gen(), rng.gen()];
        let mock_signal_parser = MockSignalParser::new(Err(error));
        let mut time_source = MockTimeSource::new(RelativeMillis::new(rng.gen_range(1..u32::MAX)));
        let tx_id = rng.gen_range(1..u32::MAX);
        let mut mock_tx = MockSender::new(Ok(Some(tx_id)), Ok(()));

        signal_controller.process_signal(mock_signal_parser, data.as_slice(), &mut time_source, &mut mock_tx);

        assert_eq!(Some((error, data.to_vec())), signal_controller.on_signal_parse_error_parameters);
        assert_eq!(Some(tx_id), signal_controller.tx_id);
        //should not call other methods
        assert_eq!(None, signal_controller.on_signal_parameters);
    }

    #[test]
    fn test_signal_controller_should_proxy_parsed_signal_on_success() {
        let mut rng = rand::thread_rng();
        let mut signal_controller = MockSignalController::new( );
        let result = SignalData::GetTimeStamp;
        let data = [0_u8, rng.gen(), rng.gen(), rng.gen()];
        let mock_signal_parser = MockSignalParser::new(Ok(result));
        let time_src_id = rng.gen_range(1..u32::MAX);
        let mut time_source = MockTimeSource::new(RelativeMillis::new(time_src_id));
        let tx_id = rng.gen_range(1..u32::MAX);
        let mut mock_tx = MockSender::new(Ok(Some(tx_id)), Ok(()));

        signal_controller.process_signal(mock_signal_parser, data.as_slice(), &mut time_source, &mut mock_tx);

        assert_eq!(Some(result), signal_controller.on_signal_parameters);
        assert_eq!(Some(tx_id), signal_controller.tx_id);
        assert_eq!(Some(time_src_id), signal_controller.time_source_id);
        //should not call other methods
        assert_eq!(None, signal_controller.on_signal_parse_error_parameters);
    }

    #[test]
    fn test_on_error() {
        let mut rng = rand::thread_rng();

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));
        let mut controller = SignalControllerImpl::new(mock_signals_handler.clone());

        let error = Errors::CommandDataCorrupted;
        let error_code = ErrorCode::for_error(error);
        let instruction: u8 = rng.gen();
        let data_arr = [instruction, rng.gen(), rng.gen(), rng.gen()];


        for send_error_success in [true, false] {
            let mut mock_tx = MockSender::new(Err(Errors::IndexOverflow),
                                              if send_error_success {Ok(())} else {Err(Errors::IndexOverflow)});
            controller.on_signal_parse_error(error, data_arr.as_slice(), &mut mock_tx);

            assert_eq!(Some((instruction, error_code)), mock_tx.send_error_params);
            assert_eq!(Some((error, send_error_success, data_arr.to_vec())), mock_signals_handler.borrow().on_signal_error_params);
            // should not call other methods
            assert_eq!(None, mock_tx.send_params);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_params);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_process_error_params);
        }
    }

    #[test]
    fn test_signals_proxy_sends_timestamp() {
        let mut rng = rand::thread_rng();

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));
        let mut controller = SignalControllerImpl::new(mock_signals_handler.clone());

        let data = SignalData::GetTimeStamp;
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
        let mut time_source = MockTimeSource::new(timestamp);
        let mut mock_tx = MockSender::new(Ok(Some(rng.gen_range(1..u32::MAX))), Ok(()));

        controller.on_signal(data, &mut time_source, &mut mock_tx);

        assert_eq!(true, time_source.time_source_called);
        assert_eq!(
            Some((
                Operation::Set,
                DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())),
                timestamp)),
            mock_tx.send_params);
        assert_eq!(Some((SignalData::GetTimeStamp, true)), mock_signals_handler.borrow().on_signal_params);
        // should not call other methods
        assert_eq!(None, mock_signals_handler.borrow().on_signal_process_error_params);
        assert_eq!(None, mock_tx.send_error_params);
        assert_eq!(None, mock_signals_handler.borrow().on_signal_error_params);

    }

    #[test]
    fn test_signals_proxy_proxies_send_timestamp_errors_to_handler() {

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));
        let mut rng = rand::thread_rng();

        let errors = [Errors::OutOfRange, Errors::NoBufferAvailable, Errors::DmaError(DMAError::Overrun(())),
            Errors::DmaError(DMAError::NotReady(())), Errors::DmaError(DMAError::SmallBuffer(())),
            Errors::NotEnoughDataGot, Errors::InvalidDataSize, Errors::DmaBufferOverflow];

        for error in errors {
            let mut controller = SignalControllerImpl::new(mock_signals_handler.clone());

            let data = SignalData::GetTimeStamp;
            let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
            let mut time_source = MockTimeSource::new(timestamp);
            let mut mock_tx = MockSender::new(Err(error), Ok(()));

            controller.on_signal(data, &mut time_source, &mut mock_tx);

            assert_eq!(true, time_source.time_source_called);
            assert_eq!(
                Some((
                    Operation::Set,
                    DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())),
                    timestamp)),
                mock_tx.send_params);
            assert_eq!(Some((error, false, data)), mock_signals_handler.borrow().on_signal_process_error_params);
            // should not call other methods
            assert_eq!(None, mock_signals_handler.borrow().on_signal_params);
            assert_eq!(None, mock_tx.send_error_params);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_error_params);
        }

    }

    #[test]
    fn test_signals_proxy_proxies_other_signals_to_handler() {

        let mut rng = rand::thread_rng();

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));

        let datas = [
            SignalData::ControlStateChanged(RelaySignalData::new (
                RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                rng.gen_range(0..15),
                rng.gen_range(0..1) == 1,
            )),
            SignalData::MonitoringStateChanged(RelaySignalData::new (
                RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                rng.gen_range(0..15),
                rng.gen_range(0..1) == 1,
            )),
            SignalData::StateFixTry(RelaySignalData::new (
                RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                rng.gen_range(0..15),
                rng.gen_range(0..1) == 1,
            )),
            SignalData::RelayStateChanged(RelaySignalDataExt::new (
                RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                rng.gen_range(0..15),
                rng.gen_range(0..1) == 1,
                rng.gen_range(0..1) == 1,
            )),
        ];

        for data in datas {


            let mut controller = SignalControllerImpl::new(mock_signals_handler.clone());

            let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
            let mut time_source = MockTimeSource::new(timestamp);
            let mut mock_tx = MockControlledSender::new(Ok(None), Ok(()));

            controller.on_signal(data, &mut time_source, &mut mock_tx);


            assert_eq!(Some((data, false)), mock_signals_handler.borrow().on_signal_params);
            assert_eq!(false, time_source.time_source_called);
            // should not call other methods
            assert_eq!(None, mock_tx.send_params);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_process_error_params);
            assert_eq!(None, mock_tx.send_error_params);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_error_params);
        }
    }

    //---combined tests---

    #[test]
    fn test_signals_proxy_parse_error_combined() {
        let mut rng = rand::thread_rng();

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));
        let mut controller = SignalControllerImpl::new(mock_signals_handler.clone());

        let error = Errors::CommandDataCorrupted;
        let error_code = ErrorCode::for_error(error);
        let instruction: u8 = rng.gen();
        let data_arr = [instruction, rng.gen(), rng.gen(), rng.gen()];

        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
        let mut time_source = MockTimeSource::new(timestamp);

        for send_error_success in [true, false] {
            let mut mock_tx = MockSender::new(Err(Errors::IndexOverflow),
                                              if send_error_success {Ok(())} else {Err(Errors::IndexOverflow)});
            let mock_signal_parser = MockSignalParser::new(Err(error));
            controller.process_signal(mock_signal_parser, data_arr.as_slice(), &mut time_source, &mut mock_tx);

            assert_eq!(Some((instruction, error_code)), mock_tx.send_error_params);
            assert_eq!(Some((error, send_error_success, data_arr.to_vec())), mock_signals_handler.borrow().on_signal_error_params);
            // should not call other methods
            assert_eq!(None, mock_tx.send_params);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_params);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_process_error_params);
            assert_eq!(false, time_source.time_source_called);
        }

    }

    #[test]
    fn test_signals_proxy_sends_timestamp_combined() {

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));
        let mut rng = rand::thread_rng();
        let mut controller = SignalControllerImpl::new(mock_signals_handler.clone());

        let data = SignalData::GetTimeStamp;

        let data_arr = [0_u8, rng.gen(), rng.gen(), rng.gen()];
        let mock_signal_parser = MockSignalParser::new(Ok(data));

        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
        let mut time_source = MockTimeSource::new(timestamp);
        let mut mock_tx = MockSender::new(Ok(Some(rng.gen_range(1..u32::MAX))), Ok(()));

        controller.process_signal(mock_signal_parser, data_arr.as_slice(), &mut time_source, &mut mock_tx);

        assert_eq!(true, time_source.time_source_called);
        assert_eq!(
            Some((
                Operation::Set,
                DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())),
                timestamp)),
            mock_tx.send_params);
        assert_eq!(Some((SignalData::GetTimeStamp, true)), mock_signals_handler.borrow().on_signal_params);
        // should not call other methods
        assert_eq!(None, mock_signals_handler.borrow().on_signal_process_error_params);
        assert_eq!(None, mock_tx.send_error_params);
        assert_eq!(None, mock_signals_handler.borrow().on_signal_error_params);

    }

    #[test]
    fn test_signals_proxy_proxies_send_timestamp_errors_to_handler_combined() {

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));
        let mut rng = rand::thread_rng();

        let data_arr = [0_u8, rng.gen(), rng.gen(), rng.gen()];

        let errors = [Errors::OutOfRange, Errors::NoBufferAvailable, Errors::DmaError(DMAError::Overrun(())),
            Errors::DmaError(DMAError::NotReady(())), Errors::DmaError(DMAError::SmallBuffer(())),
            Errors::NotEnoughDataGot, Errors::InvalidDataSize, Errors::DmaBufferOverflow];

        for error in errors {
            let mut controller = SignalControllerImpl::new(mock_signals_handler.clone());

            let data = SignalData::GetTimeStamp;
            let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
            let mut time_source = MockTimeSource::new(timestamp);
            let mut mock_tx = MockSender::new(Err(error), Ok(()));

            let mock_signal_parser = MockSignalParser::new(Ok(data));

            controller.process_signal(mock_signal_parser, data_arr.as_slice(), &mut time_source, &mut mock_tx);

            assert_eq!(true, time_source.time_source_called);
            assert_eq!(
                Some((
                    Operation::Set,
                    DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())),
                    timestamp)),
                mock_tx.send_params);
            assert_eq!(Some((error, false, data)), mock_signals_handler.borrow().on_signal_process_error_params);
            // should not call other methods
            assert_eq!(None, mock_signals_handler.borrow().on_signal_params);
            assert_eq!(None, mock_tx.send_error_params);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_error_params);
        }

    }

    #[test]
    fn test_signals_proxy_proxies_other_signals_to_handler_combined() {

        let mut rng = rand::thread_rng();

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));

        let datas = [
            SignalData::ControlStateChanged(RelaySignalData::new (
                RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                rng.gen_range(0..15),
                rng.gen_range(0..1) == 1,
            )),
            SignalData::MonitoringStateChanged(RelaySignalData::new (
                RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                rng.gen_range(0..15),
                rng.gen_range(0..1) == 1,
            )),
            SignalData::StateFixTry(RelaySignalData::new (
                RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                rng.gen_range(0..15),
                rng.gen_range(0..1) == 1,
            )),
            SignalData::RelayStateChanged(RelaySignalDataExt::new (
                RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                rng.gen_range(0..15),
                rng.gen_range(0..1) == 1,
                rng.gen_range(0..1) == 1,
            )),
        ];

        for data in datas {

            let data_arr = [0_u8, rng.gen(), rng.gen(), rng.gen()];
            let mock_signal_parser = MockSignalParser::new(Ok(data));

            let mut controller = SignalControllerImpl::new(mock_signals_handler.clone());

            let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
            let mut time_source = MockTimeSource::new(timestamp);
            let mut mock_tx = MockControlledSender::new(Ok(None), Ok(()));

            controller.process_signal(mock_signal_parser, data_arr.as_slice(), &mut time_source, &mut mock_tx);


            assert_eq!(Some((data, false)), mock_signals_handler.borrow().on_signal_params);
            assert_eq!(false, time_source.time_source_called);
            // should not call other methods
            assert_eq!(None, mock_tx.send_params);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_process_error_params);
            assert_eq!(None, mock_tx.send_error_params);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_error_params);
        }
    }


    struct MockSignalsHandler {
        on_signal_params: Option<(SignalData, bool)>,
        on_signal_error_params: Option<(Errors, bool, Vec<u8>)>,
        on_signal_process_error_params: Option<(Errors, bool, SignalData)>,
    }

    impl MockSignalsHandler {
        fn new() -> Self {
            Self {
                on_signal_params: None,
                on_signal_error_params: None,
                on_signal_process_error_params: None,
            }
        }
    }

    impl SignalsHandler for MockSignalsHandler {
        fn on_signal(&mut self, signal_data: SignalData, processed_successfully: bool) {
            self.on_signal_params = Some((signal_data, processed_successfully));
        }
        fn on_signal_parse_error(&mut self, error: Errors, sent_to_slave_success: bool, data: &[u8]) {
            self.on_signal_error_params = Some((error, sent_to_slave_success, data.to_vec()));
        }

        fn on_signal_process_error(&mut self, error: Errors, sent_to_slave_success: bool, data: SignalData) {
            self.on_signal_process_error_params = Some((error, sent_to_slave_success, data));
        }


    }

    impl SignalsHandler for Rc<RefCell<MockSignalsHandler>> {
        fn on_signal(&mut self, signal_data: SignalData, processed_successfully: bool) {
            self.borrow_mut().on_signal(signal_data, processed_successfully);
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

    struct MockControlledSender {
        send_params: Option<(Operation, DataInstructions, RelativeMillis)>,
        send_result: Result<Option<u32>, Errors>,
        send_error_called: bool,
        send_error_params: Option<(u8, ErrorCode)>,
        send_error_result: Result<(), Errors>,
    }
    impl MockControlledSender {
        pub fn new(send_result: Result<Option<u32>, Errors>, send_error_result: Result<(), Errors>) -> Self {
            Self {
                send_params: None,
                send_result,
                send_error_called: false,
                send_error_params: None,
                send_error_result,
            }
        }
    }
    impl ErrorsSender for MockControlledSender {
        fn send_error(&mut self, instruction_code: u8, error_code: ErrorCode) -> Result<(), Errors> {
            self.send_error_called = true;
            self.send_error_params = Some((instruction_code, error_code));
            self.send_error_result
        }
    }
    impl ControlledRequestSender for MockControlledSender {
        fn send(&mut self, operation: Operation, instruction: DataInstructions, timestamp: RelativeMillis) -> Result<Option<u32>, Errors> {
            self.send_params = Some((operation, instruction, timestamp));
            self.send_result
        }
    }

    struct MockSignalController {
        on_signal_parameters: Option<SignalData>,
        on_signal_parse_error_parameters: Option<(Errors, Vec<u8>)>,
        tx_id: Option<u32>,
        time_source_id: Option<u32>,
    }
    impl MockSignalController {
        pub fn new() -> Self {
            Self {
                on_signal_parameters: None,
                on_signal_parse_error_parameters: None,
                tx_id: None,
                time_source_id: None,
            }
        }
    }
    impl SignalsPreHandler for MockSignalController {

        fn on_signal<TS: RelativeTimestampSource, S: ControlledRequestSender + ErrorsSender>(&mut self, signal_data: SignalData, time_source: &mut TS, tx:  &mut S) {
            let send_res  = tx.send(Operation::None, DataInstructions::Id(Conversation::Request(EmptyRequest::new())),
                    RelativeMillis::new(0));
            self.tx_id = Some(send_res.ok().unwrap().unwrap());
            self.time_source_id = Some(time_source.get().value());
            self.on_signal_parameters = Some(signal_data);
        }

        fn on_signal_parse_error<S: ControlledRequestSender + ErrorsSender>(&mut self, error: Errors, data: &[u8], tx:  &mut S) {
            let send_res  = tx.send(Operation::None, DataInstructions::Id(Conversation::Request(EmptyRequest::new())),
                                    RelativeMillis::new(0));
            self.tx_id = Some(send_res.ok().unwrap().unwrap());
            self.on_signal_parse_error_parameters = Some((error, data.to_vec()));
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

    #[derive(Debug, PartialEq)]
    struct MockSender {
        send_params: Option<(Operation, DataInstructions, RelativeMillis)>,
        send_result: Result<Option<u32>, Errors>,
        send_error_params: Option<(u8, ErrorCode)>,
        send_error_result: Result<(), Errors>,
    }

    impl MockSender {
        pub fn new(send_result: Result<Option<u32>, Errors>, send_error_result: Result<(), Errors>) -> Self {
            Self {
                send_params: None,
                send_result,
                send_error_params: None,
                send_error_result,
            }
        }
    }

    impl ControlledRequestSender for MockSender {
        fn send(&mut self, operation: Operation, instruction: DataInstructions, timestamp: RelativeMillis) -> Result<Option<u32>, Errors> {
            self.send_params = Some((operation, instruction, timestamp));
            self.send_result
        }
    }

    impl ErrorsSender for MockSender {
        fn send_error(&mut self,  instruction_code: u8, error_code: ErrorCode) -> Result<(), Errors> {
            self.send_error_params = Some((instruction_code, error_code));
            self.send_error_result
        }
    }

}