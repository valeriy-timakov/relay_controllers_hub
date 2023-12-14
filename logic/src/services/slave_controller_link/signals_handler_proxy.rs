#![deny(unsafe_code)]


use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::RelativeMillis;
use crate::services::slave_controller_link::domain::{Conversation, DataInstructions, ErrorCode, OperationCodes, Signals};
use crate::services::slave_controller_link::parsers::SignalParseResult;
use crate::services::slave_controller_link::signals_controller::SignalsHandler;
use crate::services::slave_controller_link::transmitter_to_slave::ErrorsSender;

trait ControlledRequestSender {
    fn send(&mut self, operation: OperationCodes, instruction: DataInstructions, timestamp: RelativeMillis) -> Result<(), Errors>;
}

struct SignalsHandlerProxy<'a, SH, TS, S>
    where
        SH: SignalsHandler,
        TS: Fn() -> RelativeMillis,
        S: ControlledRequestSender + ErrorsSender,
{
    handler: SH,
    time_source: TS,
    tx:  &'a mut S,
}

impl <'a, SH, TS, S> SignalsHandlerProxy<'a, SH, TS, S>
    where
        SH: SignalsHandler,
        TS: Fn() -> RelativeMillis,
        S: ControlledRequestSender + ErrorsSender,
{
    fn new(handler: SH, time_source: TS, tx:  &'a mut S) -> Self {
        Self {
            handler, time_source, tx
        }
    }
}

impl  <'a, SH, TS, S> SignalsHandler for SignalsHandlerProxy<'a, SH, TS, S>
    where
        SH: SignalsHandler,
        TS: Fn() -> RelativeMillis,
        S: ControlledRequestSender + ErrorsSender,
{

    fn on_signal(&mut self, signal_data: SignalParseResult) {
        if signal_data.signal() == Signals::GetTimeStamp {
            let timestamp = (self.time_source)();
            let res = self.tx.send(
                OperationCodes::Set,
                DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())),
                timestamp);
            if res.is_err() {
                self.handler.on_signal_process_error(res.err().unwrap(), false, signal_data);
            }
        } else {
            self.handler.on_signal(signal_data);
        }
    }

    fn on_signal_parse_error(&mut self, error: Errors, sent_to_slave_success: bool, data: &[u8]) {
        let error_code = ErrorCode::for_error(error);
        let instruction = if !data.is_empty() { data[0] }  else { Signals::None as u8 };
        let sent_to_slave_success = self.tx.send_error(instruction, error_code).is_ok();
        self.handler.on_signal_parse_error(error, sent_to_slave_success, data);
    }

    fn on_signal_process_error(&mut self, error: Errors, sent_to_slave_success: bool, data: SignalParseResult) {
        self.handler.on_signal_process_error(error, sent_to_slave_success, data);
    }

}



#[cfg(test)]
mod tests {
    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::cell::{RefCell};
    use super::*;
    use rand::prelude::*;
    use crate::errors::DMAError;
    use crate::hal_ext::rtc_wrapper::RelativeSeconds;
    use crate::services::slave_controller_link::domain::Operation;
    use crate::services::slave_controller_link::parsers::{RelaySignalData, SignalParseResult};

    #[test]
    fn test_signals_proxy_sends_timestamp() {

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));
        let time_source_called = RefCell::new(false);
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
        let mock_time_source = || {
            *time_source_called.borrow_mut() = true;
            timestamp
        };
        let mut mock_tx = MockControlledSender::new(Ok(()), Ok(()));

        let mut proxy = SignalsHandlerProxy::new(
            mock_signals_handler.clone(),
            mock_time_source,
            &mut mock_tx
        );

        let data = SignalParseResult::new(Signals::GetTimeStamp,None);

        proxy.on_signal(data);

        assert_eq!(true, *time_source_called.borrow());
        assert_eq!(
            Some((
                OperationCodes::Set,
                DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())),
                timestamp)),
            mock_tx.send_params);
        assert_eq!(None, mock_signals_handler.borrow().on_signal_signal_data);
        assert_eq!(None, mock_signals_handler.borrow().on_signal_parse_error_params);

    }

    #[test]
    fn test_signals_proxy_proxies_send_timestamp_errors_to_handler() {

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));
        let time_source_called = RefCell::new(false);
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
        let mock_time_source = || {
            *time_source_called.borrow_mut() = true;
            timestamp
        };

        let data = SignalParseResult::new(Signals::GetTimeStamp, None);

        let errors = [Errors::OutOfRange, Errors::NoBufferAvailable, Errors::DmaError(DMAError::Overrun(())),
            Errors::DmaError(DMAError::NotReady(())), Errors::DmaError(DMAError::SmallBuffer(())),
            Errors::NotEnoughDataGot, Errors::InvalidDataSize, Errors::DmaBufferOverflow];

        for error in errors {
            let mut mock_tx = MockControlledSender::new(Err(error), Ok(()));

            let mut proxy = SignalsHandlerProxy::new(
                mock_signals_handler.clone(),
                mock_time_source,
                &mut mock_tx
            );

            proxy.on_signal(data);

            assert_eq!(true, *time_source_called.borrow());
            assert_eq!(
                Some((
                    OperationCodes::Set,
                    DataInstructions::RemoteTimestamp(Conversation::Data(timestamp.seconds())),
                    timestamp)),
                mock_tx.send_params);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_signal_data);
            assert_eq!(Some((error, false, data)), mock_signals_handler.borrow().on_signal_process_errorr_params);
        }

    }

    #[test]
    fn test_signals_proxy_proxies_other_signals_to_handler() {

        let mock_signals_handler = Rc::new(RefCell::new(MockSignalsHandler::new()));
        let time_source_called = RefCell::new(false);
        let mut rng = rand::thread_rng();
        let timestamp = RelativeMillis::new(rng.gen_range(1..u32::MAX));
        let mock_time_source = || {
            *time_source_called.borrow_mut() = true;
            timestamp
        };
        let mut mock_tx = MockControlledSender::new(Ok(()), Ok(()));

        let mut rng = rand::thread_rng();

        let datas = [
            SignalParseResult::new(Signals::ControlStateChanged, None) ,
            SignalParseResult::new(Signals::MonitoringStateChanged, Some(RelaySignalData::new (
                RelativeSeconds::new(rng.gen_range(1..u32::MAX)),
                rng.gen_range(0..15),
                rng.gen_range(0..1) == 1,
                Some(rng.gen_range(0..1) == 1),
            ))),
        ];

        for data in datas {

            let mut proxy = SignalsHandlerProxy::new(
                mock_signals_handler.clone(),
                mock_time_source,
                &mut mock_tx
            );
            proxy.on_signal(data);

            assert_eq!(false, *time_source_called.borrow());
            assert_eq!(None, mock_tx.send_params);
            assert_eq!(Some(data), mock_signals_handler.borrow().on_signal_signal_data);
            assert_eq!(None, mock_signals_handler.borrow().on_signal_parse_error_params);
        }
    }
    struct MockControlledSender {
        send_params: Option<(OperationCodes, DataInstructions, RelativeMillis)>,
        send_result: Result<(), Errors>,
        send_error_called: bool,
        send_error_params: Option<(u8, ErrorCode)>,
        send_error_result: Result<(), Errors>,
    }

    impl MockControlledSender {
        pub fn new(send_result: Result<(), Errors>, send_error_result: Result<(), Errors>) -> Self {
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
        fn send(&mut self, operation: OperationCodes, instruction: DataInstructions, timestamp: RelativeMillis) -> Result<(), Errors> {
            self.send_params = Some((operation, instruction, timestamp));
            self.send_result
        }
    }

    struct MockSignalsHandler {
        on_signal_signal_data: Option<SignalParseResult>,
        on_signal_parse_error_params: Option<(Errors, bool, Vec<u8>)>,
        on_signal_process_errorr_params: Option<(Errors, bool, SignalParseResult)>,
    }

    impl MockSignalsHandler {
        fn new() -> Self {
            Self {
                on_signal_signal_data: None,
                on_signal_parse_error_params: None,
                on_signal_process_errorr_params: None,
            }
        }
    }

    impl SignalsHandler for MockSignalsHandler {
        fn on_signal(&mut self, signal_data: SignalParseResult) {
            self.on_signal_signal_data = Some(signal_data);
        }
        fn on_signal_parse_error(&mut self, error: Errors, sent_to_slave_success: bool, data: &[u8]) {
            self.on_signal_parse_error_params = Some((error, sent_to_slave_success, data.to_vec()));
        }
        fn on_signal_process_error(&mut self, error: Errors, sent_to_slave_success: bool, data: SignalParseResult) {
            self.on_signal_process_errorr_params = Some((error, sent_to_slave_success, data));
        }
    }

    impl SignalsHandler for Rc<RefCell<MockSignalsHandler>> {
        fn on_signal(&mut self, signal_data: SignalParseResult) {
            self.borrow_mut().on_signal(signal_data);
        }
        fn on_signal_parse_error(&mut self, error: Errors, sent_to_slave_success: bool, data: &[u8]) {
            self.borrow_mut().on_signal_parse_error(error, sent_to_slave_success, data);
        }
        fn on_signal_process_error(&mut self, error: Errors, sent_to_slave_success: bool, data: SignalParseResult) {
            self.borrow_mut().on_signal_process_error(error, sent_to_slave_success, data);
        }
    }

}