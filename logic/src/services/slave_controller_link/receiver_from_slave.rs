#![deny(unsafe_code)]


use crate::errors::Errors;
use crate::hal_ext::serial_transfer::Receiver;
use crate::services::slave_controller_link::domain::OperationCodes;
use crate::services::slave_controller_link::requests_controller::RequestsControllerRx;
use crate::services::slave_controller_link::signals_controller::SignalController;

pub trait ReceiverFromSlaveControllerAbstract<Rc, SC, RCR, EH>
    where
        Rc: Receiver,
        SC: SignalController,
        RCR: RequestsControllerRx,
        EH: Fn(Errors),
{

    fn slice(&mut self) -> (&mut Rc, &mut SC, &mut RCR);
    fn error_handler(&mut self) -> &mut EH;

    fn on_get_command(&mut self) {
        let (rx, signal_controller, request_controller) = self.slice();
        let res = rx.on_rx_transfer_interrupt(|data| {
            if data.len() >= 2 {
                if data[0] == OperationCodes::None as u8 {
                    let operation_code = data[1];
                    let data = &data[2..];
                    if operation_code == OperationCodes::Signal as u8 {
                        signal_controller.process_signal(data);
                        Ok(())
                    } else if request_controller.is_response(operation_code) {
                        request_controller.process_response(operation_code, data);
                        Ok(())
                    } else {
                        Err(Errors::OperationNotRecognized(operation_code))
                    }
                } else {
                    Err(Errors::CommandDataCorrupted)
                }
            } else {
                Err(Errors::NotEnoughDataGot)
            }
        });
        if res.is_err() {
            (self.error_handler())(res.err().unwrap());
        }
    }
}

pub struct ReceiverFromSlaveController<Rc, SC, RCR, EH>
    where
        Rc: Receiver,
        SC: SignalController,
        RCR: RequestsControllerRx,
        EH: Fn(Errors),
{
    rx: Rc,
    signal_controller: SC,
    requests_controller_rx: RCR,
    error_handler: EH,
}

impl <Rc, SC, RCR, EH> ReceiverFromSlaveController<Rc, SC, RCR, EH>
    where
        Rc: Receiver,
        SC: SignalController,
        RCR: RequestsControllerRx,
        EH: Fn(Errors),
{
    pub fn new(rx: Rc, signal_controller: SC, requests_controller_rx: RCR, error_handler: EH) -> Self {
        Self {
            rx,
            signal_controller,
            requests_controller_rx,
            error_handler,
        }
    }

    #[inline(always)]
    pub fn inner_rx(&mut self) -> &mut Rc {
        &mut self.rx
    }
}

impl <Rc, RCR, SC, EH> ReceiverFromSlaveControllerAbstract<Rc, SC, RCR, EH> for ReceiverFromSlaveController<Rc, SC, RCR, EH>
    where
        Rc: Receiver,
        SC: SignalController,
        RCR: RequestsControllerRx,
        EH: Fn(Errors),
{
    #[inline(always)]
    fn slice(&mut self) ->( &mut Rc, &mut SC, &mut RCR) {
        (&mut self.rx, &mut self.signal_controller, &mut self.requests_controller_rx)
    }

    fn error_handler(&mut self) -> &mut EH {
        &mut self.error_handler
    }
}



#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    use core::cell::{RefCell};
    use super::*;


    #[test]
    fn test_on_get_command_should_report_error_not_enough_data_error_on_low_bytes_message() {

        let datas = Vec::from([[].to_vec(), [1].to_vec()]);

        for data in datas {
            let mock_receiver = MockReceiver::new(data);
            let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
            let mock_error_handler = |error: Errors| {
                *handled_error.borrow_mut() = Some(error);
            };
            let mut controller = ReceiverFromSlaveController::new(
                mock_receiver, MockSignalController::new(),
                MockRequestsControllerRx::new(false), &mock_error_handler);


            controller.on_get_command();

            assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
            assert_eq!(Err(Errors::NotEnoughDataGot), controller.rx.receiver_result.unwrap());
            assert_eq!(Some(Errors::NotEnoughDataGot), *handled_error.borrow());
            //nothing other should be called
            assert_eq!(None, controller.signal_controller.process_signal_params);
            assert_eq!(None, controller.requests_controller_rx.process_response_params);
        }
    }

    #[test]
    fn test_on_get_command_should_return_corrupted_data_error_on_starting_not_0() {
        let mock_receiver = MockReceiver::new( [1, 2, 3].to_vec() );
        let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
        let mock_error_handler = |error: Errors| {
            *handled_error.borrow_mut() = Some(error);
        };
        let mut controller = ReceiverFromSlaveController::new(
            mock_receiver, MockSignalController::new(),
            MockRequestsControllerRx::new(false), &mock_error_handler);

        controller.on_get_command();

        assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
        assert_eq!(Err(Errors::CommandDataCorrupted), controller.rx.receiver_result.unwrap());
        assert_eq!(Some(Errors::CommandDataCorrupted), *handled_error.borrow());
        //nothing other should be called
        assert_eq!(None, controller.signal_controller.process_signal_params);
        assert_eq!(None, controller.requests_controller_rx.process_response_params);
    }

    #[test]
    fn test_on_get_command_should_renurn_not_recognized_on_unknown() {
        let not_request_not_signal_operations = [OperationCodes::Unknown as u8, OperationCodes::None as u8,
            OperationCodes::Set as u8, OperationCodes::Read as u8, OperationCodes::Command as u8, 12, 56];

        for operation_code in not_request_not_signal_operations {
            let mock_receiver = MockReceiver::new( [OperationCodes::None as u8, operation_code, 0].to_vec() );
            let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
            let mock_error_handler = |error: Errors| {
                *handled_error.borrow_mut() = Some(error);
            };
            let mut controller = ReceiverFromSlaveController::new(
                mock_receiver, MockSignalController::new(),
                MockRequestsControllerRx::new(false), &mock_error_handler);

            controller.on_get_command();

            assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
            assert_eq!(Some(operation_code), *controller.requests_controller_rx.is_response_param.borrow());
            assert_eq!(Err(Errors::OperationNotRecognized(operation_code)), controller.rx.receiver_result.unwrap());
            assert_eq!(Some(Errors::OperationNotRecognized(operation_code)), *handled_error.borrow());
            //nothing other should be called
            assert_eq!(None, controller.signal_controller.process_signal_params);
            assert_eq!(None, controller.requests_controller_rx.process_response_params);
        }
    }

    #[test]
    fn test_on_get_command_should_call_request_controller_on_response_operations() {
        let response_operations = [OperationCodes::Response as u8, OperationCodes::Success as u8, OperationCodes::Error as u8];

        for operation_code in response_operations {
            for instruction_code in 0..100 {

                let mock_receiver = MockReceiver::new(
                    [OperationCodes::None as u8, operation_code, instruction_code, 1, 2, 3].to_vec() );
                let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
                let mock_error_handler = |error: Errors| {
                    *handled_error.borrow_mut() = Some(error);
                };
                let mut controller = ReceiverFromSlaveController::new(
                    mock_receiver, MockSignalController::new(),
                    MockRequestsControllerRx::new(true), &mock_error_handler);

                controller.on_get_command();

                assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
                assert_eq!(Some(operation_code), *controller.requests_controller_rx.is_response_param.borrow());
                assert_eq!(Some((operation_code, [instruction_code, 1, 2, 3].to_vec())),
                           controller.requests_controller_rx.process_response_params);
                assert_eq!(Ok(()), controller.rx.receiver_result.unwrap());
                //nothing other should be called
                assert_eq!(None, controller.signal_controller.process_signal_params);
                assert_eq!(None, *handled_error.borrow());
            }
        }
    }

    #[test]
    fn test_on_get_command_should_proxy_signals() {
        let operation_code = OperationCodes::Signal as u8;

        for instruction_code in 0..50 {
            let data = [OperationCodes::None as u8, operation_code, instruction_code, 1, 2, 3, 4];
            let mock_receiver = MockReceiver::new(data.to_vec() );
            let handled_error: RefCell<Option<Errors>> = RefCell::new(None);
            let mock_error_handler = |error: Errors| {
                *handled_error.borrow_mut() = Some(error);
            };
            let mut controller = ReceiverFromSlaveController::new(
                mock_receiver, MockSignalController::new(),
                MockRequestsControllerRx::new(true), &mock_error_handler);

            controller.on_get_command();

            assert_eq!(true, controller.rx.on_rx_transfer_interrupt_called);
            assert_eq!(Ok(()), controller.rx.receiver_result.unwrap());
            assert_eq!(Some((&data[2..]).to_vec().to_vec()), controller.signal_controller.process_signal_params);
            //nothing other should be called
            assert_eq!(None, *handled_error.borrow());
            assert_eq!(None, controller.requests_controller_rx.process_response_params);
        }
    }
    struct MockReceiver {
        data: Vec<u8>,
        receiver_result: Option<Result<(), Errors>>,
        on_rx_transfer_interrupt_called: bool,
    }

    impl MockReceiver {
        pub fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                receiver_result: None,
                on_rx_transfer_interrupt_called: false,
            }
        }
    }

    impl Receiver for MockReceiver {
        fn on_rx_transfer_interrupt<F: FnOnce(&[u8]) -> Result<(), Errors>> (&mut self, receiver: F) -> Result<(), Errors> {
            let res = receiver(&self.data.as_slice());
            self.receiver_result = Some(res);
            self.on_rx_transfer_interrupt_called = true;
            res
        }
    }
    struct MockSignalController {
        process_signal_params: Option<Vec<u8>>,
    }

    impl MockSignalController {
        pub fn new() -> Self {
            Self {
                process_signal_params: None,
            }
        }
    }

    impl SignalController for MockSignalController {
        fn process_signal(&mut self, data: &[u8]) {
            self.process_signal_params = Some(data.to_vec());
        }
    }
    struct MockRequestsControllerRx {
        process_response_params: Option<(u8, Vec<u8>)>,
        is_response_result: bool,
        is_response_param: RefCell<Option<u8>>,
    }

    impl MockRequestsControllerRx {
        pub fn new (is_response_result: bool) -> Self {
            Self {
                process_response_params: None,
                is_response_result,
                is_response_param: RefCell::new(None),
            }
        }
    }

    impl RequestsControllerRx for MockRequestsControllerRx {
        fn process_response(&mut self, operation_code: u8, data: &[u8]) {
            self.process_response_params = Some((operation_code, data.to_vec()));
        }
        fn is_response(&self, operation_code: u8) -> bool {
            *self.is_response_param.borrow_mut() = Some(operation_code);
            self.is_response_result
        }
    }

}