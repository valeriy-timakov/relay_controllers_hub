#![deny(unsafe_code)]


use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{RelativeMillis };
use crate::services::slave_controller_link::domain::{DataInstructionCodes, DataInstructions, ErrorCode, Operation, OperationCodes, Version};
use crate::services::slave_controller_link::parsers::{ResponsesParser};

const MAX_REQUESTS_COUNT: usize = 4;

#[derive(PartialEq, Debug, Copy, Clone)]
pub struct SentRequest {
    id: Option<u32>,
    operation: Operation,
    instruction: DataInstructionCodes,
    rel_timestamp: RelativeMillis,
}

impl SentRequest {
    pub fn new(id: Option<u32>, operation: Operation, instruction: DataInstructionCodes, rel_timestamp: RelativeMillis) -> Self {
        Self {
            id,
            operation,
            instruction,
            rel_timestamp
        }
    }
}

pub trait ResponseHandler {
    fn on_request_success(&mut self, request: SentRequest);
    fn on_request_error(&mut self, request: SentRequest, error_code: ErrorCode);
    fn on_request_process_error(&mut self, request: SentRequest, error: Errors, data: &[u8]);
    fn on_request_search_error(&mut self, operation: Operation, instruction: u8, id: Option<u32>, error: Errors, data: &[u8]);
    fn on_request_response(&mut self, request: SentRequest, response: DataInstructions);
}

pub trait RequestsControllerTx {
    fn add_sent_request(&mut self, request: SentRequest);
    fn check_request(&mut self, instruction: DataInstructionCodes) -> Result<Option<u32>, Errors>;
}

pub trait RequestsControllerRx {
    fn process_response(&mut self, operation_code: u8, data: &[u8]);
    fn is_response(&self, operation_code: u8) -> bool;
}

pub struct RequestsController<RH: ResponseHandler, RP: ResponsesParser> {
    sent_requests: [Option<SentRequest>; MAX_REQUESTS_COUNT],
    requests_count: usize,
    request_needs_cache_send: bool,
    response_handler: RH,
    requests_parser: RP,
    last_request_id: u32,
    api_version: Version,
}

impl <RH: ResponseHandler, RP: ResponsesParser> RequestsController<RH, RP> {
    pub fn new(response_handler: RH, requests_parser: RP, api_version: Version) -> Self {
        Self {
            sent_requests: [None, None, None, None],
            requests_count: 0,
            request_needs_cache_send: false,
            response_handler,
            requests_parser,
            last_request_id: 0,
            api_version,
        }
    }
}

impl <RH: ResponseHandler, RP: ResponsesParser> RequestsControllerTx for RequestsController<RH, RP> {

    fn check_request(&mut self, instruction_code: DataInstructionCodes) -> Result<Option<u32>, Errors> {
        if self.requests_count == MAX_REQUESTS_COUNT {
            return Err(Errors::RequestsLimitReached);
        }
        if self.requests_parser.request_needs_cache(instruction_code) && self.request_needs_cache_send {
            return Err(Errors::RequestsNeedsCacheAlreadySent);
        }

        self.last_request_id += 1;
        if self.api_version == Version::V1 {
            Ok(None)
        } else {
            Ok(Some(self.last_request_id))
        }
    }

    fn add_sent_request(&mut self, mut request: SentRequest) {
        if self.requests_parser.request_needs_cache(request.instruction) {
            self.request_needs_cache_send = true;
        }
        self.sent_requests[self.requests_count] = Some(request);
        self.requests_count += 1;
    }

}

impl <RH: ResponseHandler, RP: ResponsesParser> RequestsControllerRx for RequestsController<RH, RP> {

    fn process_response(&mut self, operation_code: u8, data: &[u8]) {
        let instruction_code = data[0];
        let data = &data[1..];
        let (operation, id, data) =
            self.requests_parser.parse_operation(operation_code, data);

        if operation != Operation::Error && operation != Operation::Set && operation != Operation::Read {
            self.response_handler.on_request_search_error(
                operation, instruction_code, id, Errors::UndefinedOperation, data);
        }

        if self.requests_count > 0 {
            for i in (0..self.requests_count).rev() {
                if let Some(request) = self.sent_requests[i] {
                    if
                        id == request.id && request.instruction as u8 == instruction_code &&
                            (operation == Operation::Error || request.operation == operation)
                    {
                        if operation == Operation::Set {
                            self.response_handler.on_request_success(request);
                        } else if operation == Operation::Error {
                            self.response_handler.on_request_error(request, ErrorCode::for_code(instruction_code));
                        } else if operation == Operation::Read {
                            match self.requests_parser.parse_response(instruction_code, data) {
                                Ok(response) => {
                                    self.response_handler.on_request_response(request, response);
                                }
                                Err(error) => {
                                    self.response_handler.on_request_process_error(request, error, data);
                                }
                            }
                        }
                        if operation == Operation::Read && self.requests_parser.request_needs_cache(request.instruction) {
                            self.request_needs_cache_send = false;
                        }
                        let mut next_pos = i + 1;
                        while next_pos < self.requests_count {
                            self.sent_requests.swap(next_pos - 1, next_pos);
                            next_pos += 1;
                        }
                        self.sent_requests[next_pos - 1] = None;
                        self.requests_count -= 1;

                        return;
                    }
                }
            }
            self.response_handler.on_request_search_error(
                operation, instruction_code, id, Errors::NoRequestsFound, data);
        } else {
            self.response_handler.on_request_search_error(
                operation, instruction_code, id, Errors::SentRequestsQueueIsEmpty, data);
        }
    }

    #[inline(always)]
    fn is_response(&self, operation_code: u8) -> bool {
        self.requests_parser.is_response(operation_code)
    }

}



#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::cell::{Ref, RefCell};
    use core::ops::Deref;
    use super::*;
    use quickcheck_macros::quickcheck;
    use rand::distributions::uniform::SampleBorrow;
    use rand::prelude::*;
    use crate::errors::DMAError;
    use crate::services::slave_controller_link::domain::{Operation, Response};
    use crate::services::slave_controller_link::parsers::RequestsParserImpl;



    #[test]
    fn test_requests_controller_check_request_should_return_error_on_cache_overflow() {
        let mock_response_handler = MockResponsesHandler::new();
        let mock_responses_parser = default();

        let mut tested =
            RequestsController::new(mock_response_handler, mock_responses_parser, Version::V1);
        tested.requests_count = MAX_REQUESTS_COUNT;

        for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
            let result = tested.check_request(data_instruction_code);

            assert_eq!(Err(Errors::RequestsLimitReached), result);
        }
    }

    #[test]
    fn test_requests_controller_check_request_should_return_error_on_needed_cache_request_send_duplication() {
        let mock_response_handler = MockResponsesHandler::new();
        let mock_responses_parser = new_check_needs_cache( |_| true );

        let mut tested =
            RequestsController::new(mock_response_handler, mock_responses_parser, Version::V1);
        tested.request_needs_cache_send = true;

        for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
            let result = tested.check_request(data_instruction_code);

            assert_eq!(Err(Errors::RequestsNeedsCacheAlreadySent), result);
        }
    }

    #[test]
    fn test_requests_controller_check_request_success() {
        let mock_response_handler = MockResponsesHandler::new();
        let mock_responses_parser = new_check_needs_cache( |_| false );

        let mut tested =
            RequestsController::new(mock_response_handler, mock_responses_parser, Version::V1);

        for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
            tested.request_needs_cache_send = false;
            let result = tested.check_request(data_instruction_code);
            assert_eq!(Ok(None), result);

            tested.request_needs_cache_send = true;
            let result = tested.check_request(data_instruction_code);
            assert_eq!(Ok(None), result);
        }
    }

    #[test]
    fn test_requests_controller_check_request_success_v2() {
        let mock_response_handler = MockResponsesHandler::new();
        let mock_responses_parser = new_check_needs_cache( |_| false );

        let mut tested =
            RequestsController::new(mock_response_handler, mock_responses_parser, Version::V2);

        let mut count = 0_u32;
        for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
            tested.request_needs_cache_send = false;
            let result = tested.check_request(data_instruction_code);
            count += 1;
            assert_eq!(Ok(Some(count)), result);

            tested.request_needs_cache_send = true;
            let result = tested.check_request(data_instruction_code);
            count += 1;
            assert_eq!(Ok(Some(count)), result);
        }
    }

    #[test]
    fn test_requests_controller_add_sent_request() {
        let mock_response_handler = MockResponsesHandler::new();
        let needs_cache_result = Rc::new(RefCell::new(false));
        let needs_cache_result_clone = needs_cache_result.clone();
        let mock_responses_parser = new_check_needs_cache(move |_| *needs_cache_result_clone.borrow() );

        let mut tested =
            RequestsController::new(mock_response_handler, mock_responses_parser, Version::V1);

        let mut rng = rand::thread_rng();
        let mut requests = [
            SentRequest::new (None, Operation::None, DataInstructionCodes::None,
                              RelativeMillis::new(rng.gen_range(1..u32::MAX))
            ),
            SentRequest::new (None,  Operation::Set, DataInstructionCodes::None,
                              RelativeMillis::new(rng.gen_range(1..u32::MAX))
            ),
            SentRequest::new (None,  Operation::Read, DataInstructionCodes::None,
                              RelativeMillis::new(rng.gen_range(1..u32::MAX))
            ),
            SentRequest::new (None,  Operation::Read, DataInstructionCodes::None,
                              RelativeMillis::new(rng.gen_range(1..u32::MAX))
            ),
        ];

        let mut count = 0;
        let mut id = 0;
        tested.request_needs_cache_send = false;

        *needs_cache_result.borrow_mut() = false;
        tested.add_sent_request(requests[0]);

        count += 1;
        id += 1;
        assert_eq!(count, tested.requests_count);
        assert_eq!(false, tested.request_needs_cache_send);
        assert_eq!(Some(requests[0]), tested.sent_requests[tested.requests_count - 1]);


        tested.request_needs_cache_send = false;

        *needs_cache_result.borrow_mut() = true;
        tested.add_sent_request(requests[1]);

        count += 1;
        id += 1;
        assert_eq!(count, tested.requests_count);
        assert_eq!(true, tested.request_needs_cache_send);
        assert_eq!(Some(requests[1]), tested.sent_requests[tested.requests_count - 1]);

        *needs_cache_result.borrow_mut() = false;
        tested.add_sent_request(requests[2]);

        count += 1;
        id += 1;
        assert_eq!(count, tested.requests_count);
        assert_eq!(true, tested.request_needs_cache_send);
        assert_eq!(Some(requests[2]), tested.sent_requests[tested.requests_count - 1]);


        tested.add_sent_request(requests[3]);
    }

    #[test]
    fn test_requests_controller_check_request_should_return_error_on_cache_overflow_outer() {
        let mock_response_handler = MockResponsesHandler::new();
        let mock_responses_parser = new_check_needs_cache(|_| { false });

        let mut tested =
            RequestsController::new(mock_response_handler, mock_responses_parser, Version::V1);
        let mut rng = rand::thread_rng();
        let request = SentRequest::new (
            None, Operation::None, DataInstructionCodes::None,
            RelativeMillis::new(rng.gen_range(1..u32::MAX)));
        for _ in 0..MAX_REQUESTS_COUNT {
            tested.add_sent_request(request);
        }

        for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
            let result = tested.check_request(data_instruction_code);

            assert_eq!(Err(Errors::RequestsLimitReached), result);
        }
    }

    #[test]
    fn test_requests_controller_check_request_should_return_error_on_needed_cache_request_send_duplication_outer() {
        let mock_response_handler = MockResponsesHandler::new();
        let needs_cache_result = Rc::new(RefCell::new(false));
        let needs_cache_result_clone = needs_cache_result.clone();
        let operation = Operation::Success;
        let mock_responses_parser = MockResponsesParser::new(
            |_, _| { unimplemented!("parse_response") },
            move |_| *needs_cache_result_clone.borrow(),
            |_, _| { (operation, None, &[0, 0, 0, 0]) },
            false
        );

        let mut tested =
            RequestsController::new(mock_response_handler, mock_responses_parser, Version::V1);

        let mut rng = rand::thread_rng();
        let request = SentRequest::new (
            None, operation, DataInstructionCodes::None,
            RelativeMillis::new(rng.gen_range(1..u32::MAX)));


        *needs_cache_result.borrow_mut() = false;
        tested.add_sent_request(request);
        *needs_cache_result.borrow_mut() = false;

        for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
            let result = tested.check_request(data_instruction_code);

            assert_eq!(Ok(None), result);
        }

        tested.process_response(request.operation as u8, &[request.instruction as u8, 0, 0, 0, 0]);
        *needs_cache_result.borrow_mut() = false;
        tested.add_sent_request(request);
        *needs_cache_result.borrow_mut() = true;

        for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
            let result = tested.check_request(data_instruction_code);

            assert_eq!(Ok(None), result);
        }

        tested.process_response(request.operation as u8, &[request.instruction as u8, 0, 0, 0, 0]);
        *needs_cache_result.borrow_mut() = true;
        tested.add_sent_request(request);
        *needs_cache_result.borrow_mut() = false;

        for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
            let result = tested.check_request(data_instruction_code);

            assert_eq!(Ok(None), result);
        }

        tested.process_response(request.operation as u8, &[request.instruction as u8, 0, 0, 0, 0]);
        *needs_cache_result.borrow_mut() = true;
        tested.add_sent_request(request);
        *needs_cache_result.borrow_mut() = true;

        for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
            let result = tested.check_request(data_instruction_code);

            assert_eq!(Err(Errors::RequestsNeedsCacheAlreadySent), result);
        }
    }

    #[test]
    fn test_requests_controller_is_request_should_proxy_to_parser() {

        let mock_parser = MockResponsesParser::new(
            |_, _| { unimplemented!("parse_response") },
            |_| { unimplemented!("request_needs_cache") },
            |_, _| { unimplemented!("parse_operation") },
            false
        );

        let mut controller = RequestsController::new(
            MockResponsesHandler::new(), default(), Version::V1);

        let responses = [OperationCodes::Response as u8, OperationCodes::Success as u8, OperationCodes::Error as u8,
            OperationCodes::SuccessV2 as u8, OperationCodes::ResponseV2 as u8, OperationCodes::ErrorV2 as u8,
            OperationCodes::None as u8, OperationCodes::Set as u8, OperationCodes::Read as u8,
            OperationCodes::Command as u8, OperationCodes::Signal as u8, 11, 12, 13, 14, 56, 128, 255];

        let mut rng = rand::thread_rng();
        for response in responses {
            controller.requests_parser.is_response_result = rng.gen();
            assert_eq!(controller.requests_parser.is_response_result, controller.is_response(response));
            assert_eq!(Some(response), *controller.requests_parser.is_response_param.borrow());
        }

    }


    const ADD_DATA_INSTRUCTION_CODES: [DataInstructionCodes; 22] = [
        DataInstructionCodes::None,
        DataInstructionCodes::Settings,
        DataInstructionCodes::State,
        DataInstructionCodes::Id,
        DataInstructionCodes::InterruptPin,
        DataInstructionCodes::RemoteTimestamp,
        DataInstructionCodes::StateFixSettings,
        DataInstructionCodes::RelayState,
        DataInstructionCodes::Version,
        DataInstructionCodes::CurrentTime,
        DataInstructionCodes::ContactWaitData,
        DataInstructionCodes::FixData,
        DataInstructionCodes::SwitchData,
        DataInstructionCodes::CyclesStatistics,
        DataInstructionCodes::SwitchCountingSettings,
        DataInstructionCodes::RelayDisabledTemp,
        DataInstructionCodes::RelaySwitchedOn,
        DataInstructionCodes::RelayMonitorOn,
        DataInstructionCodes::RelayControlOn,
        DataInstructionCodes::All,
        DataInstructionCodes::Last,
        DataInstructionCodes::Unknown, ];

    struct MockResponsesHandler {

    }

    impl MockResponsesHandler {
        pub fn new() -> Self {
            Self {

            }
        }
    }

    impl ResponseHandler for MockResponsesHandler {
        fn on_request_success(&mut self, request: SentRequest) {

        }

        fn on_request_error(&mut self, request: SentRequest, error_code: ErrorCode) {

        }

        fn on_request_process_error(&mut self, request: SentRequest, error: Errors, data: &[u8]) {

        }

        fn on_request_response(&mut self, request: SentRequest, response: DataInstructions) {

        }

        fn on_request_search_error(&mut self, operation: Operation, instruction: u8, id: Option<u32>, error: Errors, data: &[u8]) {

        }
    }

    fn new_parse_response<F>(parse_response_cb: F) -> MockResponsesParser<F, fn(DataInstructionCodes) -> bool, fn(u8, &[u8]) -> (Operation, Option<u32>, &[u8])>
        where
            F: Fn(u8, &[u8]) -> Result<DataInstructions, Errors>,
    {
        MockResponsesParser::new(parse_response_cb, |_| unimplemented!(), |_, _| unimplemented!(), false)
    }

    fn new_check_needs_cache<F>(request_needs_cache_cb: F) -> MockResponsesParser<fn(u8, &[u8]) -> Result<DataInstructions, Errors>, F, fn(u8, &[u8]) -> (Operation, Option<u32>, &[u8])>
        where
            F: Fn(DataInstructionCodes) -> bool,
    {
        MockResponsesParser::new(|_, _| unimplemented!(), request_needs_cache_cb, |_, _| unimplemented!(), false)
    }

    fn default() -> MockResponsesParser<fn(u8, &[u8]) -> Result<DataInstructions, Errors>, fn(DataInstructionCodes) -> bool, fn(u8, &[u8]) -> (Operation, Option<u32>, &[u8])> {
        MockResponsesParser::new(|_, _| unimplemented!(), |_| unimplemented!(), |_, _| unimplemented!(), false)
    }

    struct MockResponsesParser<F1, F2, F3>
        where
            F1: Fn(u8, &[u8]) -> Result<DataInstructions, Errors>,
            F2: Fn(DataInstructionCodes) -> bool,
            F3: Fn(u8, &[u8]) -> (Operation, Option<u32>, &[u8]),
    {
        parse_response_cb: F1,
        request_needs_cache_cb: F2,
        parse_operation_cb: F3,
        is_response_param: RefCell<Option<u8>>,
        is_response_result: bool,
    }

    impl <F1, F2, F3>  MockResponsesParser<F1, F2, F3>
        where
            F1: Fn(u8, &[u8]) -> Result<DataInstructions, Errors> ,
            F2: Fn(DataInstructionCodes) -> bool,
            F3: Fn(u8, &[u8]) -> (Operation, Option<u32>, &[u8]),
    {
        pub fn new<>(parse_response_cb: F1, request_needs_cache_cb: F2, parse_operation_cb: F3, is_response_result: bool) -> Self {
            Self {
                parse_response_cb,
                request_needs_cache_cb,
                parse_operation_cb,
                is_response_param: RefCell::new(None),
                is_response_result,
            }
        }
    }

    impl <F1, F2, F3> ResponsesParser for MockResponsesParser<F1, F2, F3>
        where
            F1: Fn(u8, &[u8]) -> Result<DataInstructions, Errors> ,
            F2: Fn(DataInstructionCodes) -> bool,
            F3: Fn(u8, &[u8]) -> (Operation, Option<u32>, &[u8]),
    {
        fn parse_response(&self, instruction_code: u8, data: &[u8]) -> Result<DataInstructions, Errors> {
            (self.parse_response_cb)(instruction_code, data)
        }
        fn request_needs_cache(&self, instruction: DataInstructionCodes) -> bool {
            (self.request_needs_cache_cb)(instruction)
        }
        fn parse_operation<'a>(&self, operation_code: u8, data: &'a [u8]) -> (Operation, Option<u32>, &'a [u8]) {
            (self.parse_operation_cb)(operation_code, data)
        }
        fn is_response(&self, operation_code: u8) -> bool {
            *self.is_response_param.borrow_mut() = Some(operation_code);
            self.is_response_result
        }
    }
}