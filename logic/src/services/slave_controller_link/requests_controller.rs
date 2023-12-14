#![deny(unsafe_code)]


use core::marker::PhantomData;
use crate::errors::Errors;
use crate::hal_ext::rtc_wrapper::{RelativeMillis };
use crate::services::slave_controller_link::domain::{DataInstructionCodes, DataInstructions, ErrorCode, Operation, OperationCodes, Version};
use crate::services::slave_controller_link::parsers::{ResponseParser, ResponsePayload, ResponseBodyParser, ResponsePayloadParsed, ResponseDataParser, ResponsePostParser};

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

pub trait ResponseHandler<RPP: ResponsePostParser,> {
    fn on_request_success(&mut self, request: SentRequest);
    fn on_request_response(&mut self, request: SentRequest, response: DataInstructions);
    fn on_request_error(&mut self, request: SentRequest, error_code: ErrorCode);
    fn on_request_parse_error(&mut self, request: Option<SentRequest>, error: Errors, data: &[u8]);
    fn on_request_search_error(&mut self, payload: RPP, error: Errors);
}

pub trait RequestsControllerTx {
    fn add_sent_request(&mut self, request: SentRequest);
    fn check_request(&mut self, instruction: DataInstructionCodes) -> Result<Option<u32>, Errors>;
}

pub trait RequestsControllerRx<RP, RBP, RPP>
    where
        RP: ResponseParser<RBP, RPP>,
        RBP: ResponseBodyParser,
        RPP: ResponsePostParser + ResponseDataParser<RBP>,
{
    fn process_response(&mut self, payload: RP);
}



pub struct RequestsController<RH, RBP, RPP>
    where
        RH: ResponseHandler<RPP>,
        RBP: ResponseBodyParser,
        RPP: ResponsePostParser,
{
    sent_requests: [Option<SentRequest>; MAX_REQUESTS_COUNT],
    requests_count: usize,
    request_needs_cache_send: bool,
    response_handler: RH,
    response_body_parser: RBP,
    last_request_id: u32,
    _phantom: PhantomData<RPP>,
}

impl <RH, RBP, RPP> RequestsController<RH, RBP, RPP>
    where
        RH: ResponseHandler<RPP>,
        RBP: ResponseBodyParser,
        RPP: ResponsePostParser,
{
    pub fn new(response_handler: RH, response_body_parser: RBP) -> Self {
        Self {
            sent_requests: [None, None, None, None],
            requests_count: 0,
            request_needs_cache_send: false,
            response_handler,
            response_body_parser,
            last_request_id: 0,
            _phantom: PhantomData,
        }
    }
}

impl <RH, RBP, RPP> RequestsControllerTx for RequestsController<RH, RBP, RPP>
    where
        RH: ResponseHandler<RPP>,
        RBP: ResponseBodyParser,
        RPP: ResponsePostParser,
{

    fn check_request(&mut self, instruction_code: DataInstructionCodes) -> Result<Option<u32>, Errors> {
        if self.requests_count == MAX_REQUESTS_COUNT {
            return Err(Errors::RequestsLimitReached);
        }
        if self.response_body_parser.request_needs_cache(instruction_code) && self.request_needs_cache_send {
            return Err(Errors::RequestsNeedsCacheAlreadySent);
        }

        self.last_request_id += 1;
        if self.response_body_parser.slave_controller_version() == Version::V1 {
            Ok(None)
        } else {
            Ok(Some(self.last_request_id))
        }
    }

    fn add_sent_request(&mut self, request: SentRequest) {
        if self.response_body_parser.request_needs_cache(request.instruction) {
            self.request_needs_cache_send = true;
        }
        self.sent_requests[self.requests_count] = Some(request);
        self.requests_count += 1;
    }

}

impl <RH, RP, RBP, RPP> RequestsControllerRx<RP, RBP, RPP> for RequestsController<RH, RBP, RPP>
    where
        RH: ResponseHandler<RPP>,
        RP: ResponseParser<RBP, RPP>,
        RBP: ResponseBodyParser,
        RPP: ResponsePostParser + ResponseDataParser<RBP>,
{

    fn process_response(&mut self, payload: RP) {

        let response = match payload.parse(&self.response_body_parser) {
            Ok(response) => response,
            Err(error) => {
                self.response_handler.on_request_parse_error(None, error, payload.data());
                return;
            }
        };

        let response_operation = response.operation();
        if self.requests_count > 0 {
            for i in (0..self.requests_count).rev() {
                if let Some(request) = self.sent_requests[i] {
                    if
                        response.request_id() == request.id && response.instruction() == request.instruction &&
                            (response_operation == Operation::Error || response_operation == request.operation)
                    {
                        if response_operation == Operation::Set {
                            self.response_handler.on_request_success(request);
                        } else if response_operation == Operation::Error {
                            self.response_handler.on_request_error(request, response.error_code());
                        } else if response_operation == Operation::Read {
                            let result = response.parse(&self.response_body_parser);
                            match result {
                                Ok(response_body) => {
                                    self.response_handler.on_request_response(request, response_body);
                                }
                                Err(error) => {
                                    self.response_handler.on_request_parse_error(Some(request), error, response.data());
                                }
                            }
                        }
                        if response_operation == Operation::Read && self.response_body_parser.request_needs_cache(request.instruction) {
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
            self.response_handler.on_request_search_error(response, Errors::NoRequestsFound);
        } else {
            self.response_handler.on_request_search_error(response, Errors::SentRequestsQueueIsEmpty);
        }
    }
}

//
//
// #[cfg(test)]
// mod tests {
//     use alloc::boxed::Box;
//     use alloc::rc::Rc;
//     use alloc::vec::Vec;
//     use core::cell::{Ref, RefCell};
//     use core::marker::PhantomData;
//     use core::ops::Deref;
//     use super::*;
//     use quickcheck_macros::quickcheck;
//     use rand::distributions::uniform::SampleBorrow;
//     use rand::prelude::*;
//     use crate::errors::DMAError;
//     use crate::services::slave_controller_link::domain::{Conversation, Operation, Response};
//     use crate::services::slave_controller_link::parsers::{ ResponseBodyParserImpl};
//
//
//     #[test]
//     fn test_requests_controller_check_request_should_return_error_on_cache_overflow() {
//         let mock_response_handler = MockResponsesHandler::new();
//         let mock_responses_parser = default_mock_parser(Version::V1);
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser,);
//         tested.requests_count = MAX_REQUESTS_COUNT;
//
//         for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
//             let result = tested.check_request(data_instruction_code);
//
//             assert_eq!(Err(Errors::RequestsLimitReached), result);
//         }
//     }
//
//     #[test]
//     fn test_requests_controller_check_request_should_return_error_on_needed_cache_request_send_duplication() {
//         let mock_response_handler = MockResponsesHandler::new();
//         let mock_responses_parser = new_check_needs_cache(true,
//                                                           Version::V1);
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser);
//         tested.request_needs_cache_send = true;
//
//         for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
//             let result = tested.check_request(data_instruction_code);
//
//             assert_eq!(Err(Errors::RequestsNeedsCacheAlreadySent), result);
//         }
//     }
//
//     #[test]
//     fn test_requests_controller_check_request_success_v1() {
//         let mock_response_handler = MockResponsesHandler::new();
//         let mock_responses_parser = new_check_needs_cache(false,
//                                                           Version::V1);
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser);
//
//         for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
//             tested.request_needs_cache_send = false;
//             let result = tested.check_request(data_instruction_code);
//             assert_eq!(Ok(None), result);
//
//             tested.request_needs_cache_send = true;
//             let result = tested.check_request(data_instruction_code);
//             assert_eq!(Ok(None), result);
//         }
//     }
//
//     #[test]
//     fn test_requests_controller_check_request_success_v2() {
//         let mock_response_handler = MockResponsesHandler::new();
//         let mock_responses_parser = new_check_needs_cache(false,
//                                                           Version::V2);
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser);
//
//         let mut count = 0_u32;
//         for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
//             tested.request_needs_cache_send = false;
//             let result = tested.check_request(data_instruction_code);
//             count += 1;
//             assert_eq!(Ok(Some(count)), result);
//
//             tested.request_needs_cache_send = true;
//             let result = tested.check_request(data_instruction_code);
//             count += 1;
//             assert_eq!(Ok(Some(count)), result);
//         }
//     }
//
//     #[test]
//     fn test_requests_controller_add_sent_request() {
//         let mock_response_handler = MockResponsesHandler::new();
//         let mock_responses_parser = new_check_needs_cache(false,
//                                                           Version::V1);
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser);
//
//         let mut rng = rand::thread_rng();
//         let mut requests = [
//             SentRequest::new (None, Operation::None, DataInstructionCodes::None,
//                               RelativeMillis::new(rng.gen_range(1..u32::MAX))
//             ),
//             SentRequest::new (None,  Operation::Set, DataInstructionCodes::None,
//                               RelativeMillis::new(rng.gen_range(1..u32::MAX))
//             ),
//             SentRequest::new (None,  Operation::Read, DataInstructionCodes::None,
//                               RelativeMillis::new(rng.gen_range(1..u32::MAX))
//             ),
//             SentRequest::new (None,  Operation::Read, DataInstructionCodes::None,
//                               RelativeMillis::new(rng.gen_range(1..u32::MAX))
//             ),
//         ];
//
//         let mut count = 0;
//         let mut id = 0;
//         tested.request_needs_cache_send = false;
//
//         tested.response_body_parser.request_needs_cache_result = false;
//         tested.add_sent_request(requests[0]);
//
//         count += 1;
//         id += 1;
//         assert_eq!(count, tested.requests_count);
//         assert_eq!(false, tested.request_needs_cache_send);
//         assert_eq!(Some(requests[0]), tested.sent_requests[tested.requests_count - 1]);
//
//
//         tested.request_needs_cache_send = false;
//
//         tested.response_body_parser.request_needs_cache_result = true;
//         tested.add_sent_request(requests[1]);
//
//         count += 1;
//         id += 1;
//         assert_eq!(count, tested.requests_count);
//         assert_eq!(true, tested.request_needs_cache_send);
//         assert_eq!(Some(requests[1]), tested.sent_requests[tested.requests_count - 1]);
//
//         tested.response_body_parser.request_needs_cache_result = false;
//         tested.add_sent_request(requests[2]);
//
//         count += 1;
//         id += 1;
//         assert_eq!(count, tested.requests_count);
//         assert_eq!(true, tested.request_needs_cache_send);
//         assert_eq!(Some(requests[2]), tested.sent_requests[tested.requests_count - 1]);
//
//
//         tested.add_sent_request(requests[3]);
//     }
//
//     #[test]
//     fn test_requests_controller_check_request_should_return_error_on_cache_overflow_outer() {
//         let mock_response_handler = MockResponsesHandler::new();
//         let mock_responses_parser = new_check_needs_cache(false,
//                                                           Version::V1);
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser);
//         let mut rng = rand::thread_rng();
//         let request = SentRequest::new (
//             None, Operation::None, DataInstructionCodes::None,
//             RelativeMillis::new(rng.gen_range(1..u32::MAX)));
//         for _ in 0..MAX_REQUESTS_COUNT {
//             tested.add_sent_request(request);
//         }
//
//         for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
//             let result = tested.check_request(data_instruction_code);
//
//             assert_eq!(Err(Errors::RequestsLimitReached), result);
//         }
//     }
//
//     #[test]
//     fn test_requests_controller_check_request_should_return_error_on_needed_cache_request_send_duplication_outer() {
//         let mut rng = rand::thread_rng();
//         let mock_response_handler = MockResponsesHandler::new();
//         let operation = Operation::Set;
//         let parsed_data = [rng.gen(), rng.gen()].to_vec();
//         let mock_response_body_parser: MockResponseBodyParserLight = MockResponseBodyParserLight::new(
//             || Err(Errors::NoRequestsFound), false,
//             Ok(Some(0)), Version::V1
//         );
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_response_body_parser);
//
//         let request = SentRequest::new (
//             None, operation, DataInstructionCodes::None,
//             RelativeMillis::new(rng.gen_range(1..u32::MAX)));
//
//
//         tested.response_body_parser.request_needs_cache_result = false;
//         tested.add_sent_request(request);
//
//         for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
//             let result = tested.check_request(data_instruction_code);
//
//             assert_eq!(Ok(None), result);
//         }
//
//         let mock_response_parser = MockResponseParserLight::new(
//             Ok(MockResponsePostParser::new(
//                 operation, DataInstructionCodes::Id, None, false,
//                 ErrorCode::OK, parsed_data.to_vec())),
//             parsed_data.to_vec() );
//
//         tested.process_response(mock_response_parser);
//         tested.response_body_parser.request_needs_cache_result = false;
//         tested.add_sent_request(request);
//         tested.response_body_parser.request_needs_cache_result = true;
//
//         for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
//             let result = tested.check_request(data_instruction_code);
//
//             assert_eq!(Ok(None), result);
//         }
//
//         let mock_response_parser = MockResponseParserLight::new(
//             Ok(MockResponsePostParser::new(
//                 operation, DataInstructionCodes::Id, None, false,
//                  ErrorCode::OK, parsed_data.to_vec())),
//                parsed_data.to_vec() );
//         tested.process_response(mock_response_parser);
//         tested.response_body_parser.request_needs_cache_result = true;
//         tested.add_sent_request(request);
//         tested.response_body_parser.request_needs_cache_result = false;
//
//         for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
//             let result = tested.check_request(data_instruction_code);
//
//             assert_eq!(Ok(None), result);
//         }
//
//
//         let mock_response_parser = MockResponseParserLight::new(
//             Ok(MockResponsePostParser::new(
//                 operation, DataInstructionCodes::Id, None, false,
//                 ErrorCode::OK, parsed_data.to_vec())),
//             parsed_data.to_vec() );
//
//         tested.process_response(mock_response_parser);
//         tested.response_body_parser.request_needs_cache_result = true;
//         tested.add_sent_request(request);
//         tested.response_body_parser.request_needs_cache_result = true;
//
//         for data_instruction_code in ADD_DATA_INSTRUCTION_CODES {
//             let result = tested.check_request(data_instruction_code);
//
//             assert_eq!(Err(Errors::RequestsNeedsCacheAlreadySent), result);
//         }
//     }
//
//
//     #[test]
//     fn test_process_response_should_proxy_error_on_parse_error() {
//         for api_version in [Version::V1, Version::V2] {
//             let mut rng = rand::thread_rng();
//             let mut data: Vec<u8> = Vec::new();
//             for i in 1..8 {
//                 data.push(rng.gen_range(1..u8::MAX));
//             }
//
//             let mock_responses_parser = new_check_needs_cache(false,
//                 Version::V1);
//             let mock_response_handler = MockResponsesHandler::new();
//
//             let error = Errors::DataCorrupted;
//
//             let mut tested =
//                 RequestsController::new(mock_response_handler, mock_responses_parser);
//
//             let response_body_parser_id  = tested.response_body_parser.id;
//             let mock_response_parser = MockResponseParser::new(
//                 Err(error), data.clone(), |rbp| {
//                     assert_eq!(response_body_parser_id, rbp.id);
//                 });
//
//
//             tested.process_response(mock_response_parser);
//
//
//             assert_eq!(Some((None, error, data)), tested.response_handler.on_request_parse_error_params);
//             //nothing else should be called
//             assert!(tested.response_handler.on_request_search_error_params.is_none());
//             assert_eq!(None, tested.response_handler.on_request_response_params);
//             assert_eq!(None, tested.response_handler.on_request_success_params);
//             assert_eq!(None, tested.response_handler.on_request_error_params);
//             assert_eq!(None, *(tested.response_body_parser.request_needs_cache_param.borrow()));
//         }
//     }
// /*
//     #[test]
//     fn test_process_response_v1_should_proxy_error_on_empty_queue() {
//         let mut rng = rand::thread_rng();
//         let operation_code = rng.gen_range(1..u8::MAX);
//         let operation = Operation::Set;
//         let instruction_code = rng.gen_range(1..u8::MAX);
//         let data = [instruction_code, rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)];
//
//
//         let mock_responses_parser = MockResponseBodyParser::new(
//             || Err(Errors::NoRequestsFound), false,
//             Err(Errors::NotEnoughDataGot), Version::V1
//         );
//         let mock_response_handler = MockResponsesHandler::new();
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser);
//
//         let mock_response_parser = MockResponseParser::new(
//             MockResponsePreParseResult {
//                 operation: operation,
//                 instruction: DataInstructionCodes::Id(0),
//                 request_id: None,
//                 needs_cache: false,
//                 error_code: ErrorCode::Ok,
//                 data: parsed_data.to_vec(),
//             }, parsed_data.to_vec() );
//
//         tested.process_response(operation_code, &data);
//
//         assert_eq!(Some(operation_code), *tested.response_body_parser.parse_operation_param.borrow());
//         assert_eq!(Some((operation, instruction_code, None, Errors::SentRequestsQueueIsEmpty, data[1..].to_vec())),
//                    tested.response_handler.on_request_search_error_params);
//         //nothing else should be called
//         assert_eq!(None, *tested.response_body_parser.parse_id_param.borrow());
//         assert_eq!(None, tested.response_handler.on_request_process_error_params);
//         assert_eq!(None, tested.response_handler.on_request_response_params);
//         assert_eq!(None, tested.response_handler.on_request_success_params);
//         assert_eq!(None, tested.response_handler.on_request_error_params);
//         assert_eq!(None, *tested.response_body_parser.parse_response_params.borrow());
//         assert_eq!(None, *tested.response_body_parser.request_needs_cache_param.borrow());
//     }
//
//     #[test]
//     fn test_process_response_v2_should_proxy_error_on_empty_queue() {
//         let mut rng = rand::thread_rng();
//         let operation_code = rng.gen_range(1..u8::MAX);
//         let operation = Operation::Set;
//         let instruction_code = rng.gen_range(1..u8::MAX);
//         let mut data: Vec<u8> = Vec::new();
//         data.push(instruction_code);
//         for i in 1..8 {
//             data.push(rng.gen_range(1..u8::MAX));
//         }
//         let data = data.as_slice();
//         let id: u32 = rng.next_u32();
//
//         let parsed_data = [rng.gen(), rng.gen()];
//
//         let mock_responses_parser = MockResponseBodyParser::new(
//             || Err(Errors::NoRequestsFound), false, Ok(Some(id))
//         );
//         let mock_response_handler = MockResponsesHandler::new();
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser, Version::V2);
//
//         let mock_response_parser = MockResponseParser::new(
//             MockResponsePreParseResult {
//                 operation: operation,
//                 instruction: DataInstructionCodes::Id(0),
//                 request_id: None,
//                 needs_cache: false,
//                 error_code: ErrorCode::Ok,
//                 data: parsed_data.to_vec(),
//             }, parsed_data.to_vec() );
//
//         tested.process_response(operation_code, &data);
//
//         assert_eq!(Some(data[1..].to_vec()), *tested.response_body_parser.parse_id_param.borrow());
//         assert_eq!(Some(operation_code), *tested.response_body_parser.parse_operation_param.borrow());
//         assert_eq!(Some((operation, instruction_code, Some(id), Errors::SentRequestsQueueIsEmpty, data[5..].to_vec())),
//                    tested.response_handler.on_request_search_error_params);
//         //nothing else should be called
//         assert_eq!(None, tested.response_handler.on_request_process_error_params);
//         assert_eq!(None, tested.response_handler.on_request_response_params);
//         assert_eq!(None, tested.response_handler.on_request_success_params);
//         assert_eq!(None, tested.response_handler.on_request_error_params);
//         assert_eq!(None, *tested.response_body_parser.parse_response_params.borrow());
//         assert_eq!(None, *tested.response_body_parser.request_needs_cache_param.borrow());
//     }
//
//     #[test]
//     fn test_process_response_v1_should_proxy_error_on_correspondig_request_not_found() {
//         let mut rng = rand::thread_rng();
//         let operation_code = rng.gen_range(1..u8::MAX);
//         let operation = Operation::Set;
//         let instruction_code = rng.gen_range(1..u8::MAX);
//         let data = [instruction_code, rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)];
//
//
//         let mock_responses_parser = MockResponseBodyParser::new(
//             || Err(Errors::NotEnoughDataGot), false,
//             Err(Errors::NotEnoughDataGot), Version::V1
//         );
//         let mock_response_handler = MockResponsesHandler::new();
//
//         let mock_response_parser = MockResponseParser::new(
//             MockResponsePreParseResult {
//                 operation: operation,
//                 instruction: DataInstructionCodes::Id(0),
//                 request_id: None,
//                 needs_cache: false,
//                 error_code: ErrorCode::Ok,
//                 data: parsed_data.to_vec()
//             }, parsed_data.to_vec() );
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser);
//
//         tested.add_sent_request(SentRequest::new(
//             None, Operation::Error, DataInstructionCodes::None, RelativeMillis::new(rng.next_u32())));
//         *tested.response_body_parser.request_needs_cache_param.borrow_mut() = None;
//
//         tested.process_response(operation_code, &data);
//
//         assert_eq!(Some(operation_code), *tested.response_body_parser.parse_operation_param.borrow());
//         assert_eq!(Some((operation, instruction_code, None, Errors::NoRequestsFound, data[1..].to_vec())),
//                    tested.response_handler.on_request_search_error_params);
//         //nothing else should be called
//         assert_eq!(None, *tested.response_body_parser.parse_id_param.borrow());
//         assert_eq!(None, tested.response_handler.on_request_process_error_params);
//         assert_eq!(None, tested.response_handler.on_request_response_params);
//         assert_eq!(None, tested.response_handler.on_request_success_params);
//         assert_eq!(None, tested.response_handler.on_request_error_params);
//         assert_eq!(None, *tested.response_body_parser.parse_response_params.borrow());
//         assert_eq!(None, *tested.response_body_parser.request_needs_cache_param.borrow());
//     }
//
//     #[test]
//     fn test_process_response_v2_should_proxy_error_on_correspondig_request_not_found() {
//         let mut rng = rand::thread_rng();
//         let operation_code = rng.gen_range(1..u8::MAX);
//         let operation = Operation::Set;
//         let instruction_code = rng.gen_range(1..u8::MAX);
//         let mut data: Vec<u8> = Vec::new();
//         data.push(instruction_code);
//         for i in 1..8 {
//             data.push(rng.gen_range(1..u8::MAX));
//         }
//         let data = data.as_slice();
//         let id: u32 = rng.next_u32();
//
//         let parsed_data = [rng.gen(), rng.gen()];
//
//         let mock_responses_parser = MockResponseBodyParser::new(
//             || Err(Errors::NotEnoughDataGot), false, Ok(Some(id))
//         );
//         let mock_response_handler = MockResponsesHandler::new();
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser, Version::V2);
//
//         let mock_response_parser = MockResponseParser::new(
//             MockResponsePreParseResult {
//                 operation: operation,
//                 instruction: DataInstructionCodes::Id(0),
//                 request_id: None,
//                 needs_cache: false,
//                 error_code: ErrorCode::Ok,
//                 data: parsed_data.to_vec(),
//             }, parsed_data.to_vec() );
//
//         tested.add_sent_request(SentRequest::new(
//             Some(id), Operation::Error, DataInstructionCodes::None, RelativeMillis::new(rng.next_u32())));
//         *tested.response_body_parser.request_needs_cache_param.borrow_mut() = None;
//
//         tested.process_response(operation_code, &data);
//
//         assert_eq!(Some(data[1..].to_vec()), *tested.response_body_parser.parse_id_param.borrow());
//         assert_eq!(Some(operation_code), *tested.response_body_parser.parse_operation_param.borrow());
//         assert_eq!(Some((operation, instruction_code, Some(id), Errors::NoRequestsFound, data[5..].to_vec())),
//                    tested.response_handler.on_request_search_error_params);
//         //nothing else should be called
//         assert_eq!(None, tested.response_handler.on_request_process_error_params);
//         assert_eq!(None, tested.response_handler.on_request_response_params);
//         assert_eq!(None, tested.response_handler.on_request_success_params);
//         assert_eq!(None, tested.response_handler.on_request_error_params);
//         assert_eq!(None, *tested.response_body_parser.parse_response_params.borrow());
//         assert_eq!(None, *tested.response_body_parser.request_needs_cache_param.borrow());
//     }
//
//
//     #[test]
//     fn test_process_response_v1_should_inform_set_if_set_request_found() {
//         let mut rng = rand::thread_rng();
//         let operation_code = rng.gen_range(1..u8::MAX);
//         let operation = Operation::Set;
//         let instruction_code = ADD_DATA_INSTRUCTION_CODES[
//             rng.gen_range(0..ADD_DATA_INSTRUCTION_CODES.len() as usize)];
//         let data = [instruction_code as u8, rng.gen_range(1..u8::MAX),
//             rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)];
//
//
//         let mock_responses_parser = MockResponseBodyParser::new(
//             || Err(Errors::NotEnoughDataGot), false,
//             Err(Errors::NotEnoughDataGot), Version::V1
//         );
//         let mock_response_handler = MockResponsesHandler::new();
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser);
//
//         let request = SentRequest::new(
//             None, operation, instruction_code, RelativeMillis::new(rng.next_u32()));
//
//         let mock_response_parser = MockResponseParser::new(
//             MockResponsePreParseResult {
//                 operation: operation,
//                 instruction: DataInstructionCodes::Id(0),
//                 request_id: None,
//                 needs_cache: false,
//                 error_code: ErrorCode::Ok,
//                 data: parsed_data.to_vec(),
//             }, parsed_data.to_vec() );
//
//         tested.add_sent_request(request);
//         *tested.response_body_parser.request_needs_cache_param.borrow_mut() = None;
//
//         tested.process_response(operation_code, &data);
//
//         assert_eq!(Some(operation_code), *tested.response_body_parser.parse_operation_param.borrow());
//         assert_eq!(Some(request), tested.response_handler.on_request_success_params);
//         //nothing else should be called
//         assert_eq!(None, *tested.response_body_parser.parse_id_param.borrow());
//         assert_eq!(None, tested.response_handler.on_request_process_error_params);
//         assert_eq!(None, tested.response_handler.on_request_response_params);
//         assert_eq!(None, tested.response_handler.on_request_error_params);
//         assert_eq!(None, *tested.response_body_parser.parse_response_params.borrow());
//         assert_eq!(None, *tested.response_body_parser.request_needs_cache_param.borrow());
//         assert_eq!(None, tested.response_handler.on_request_search_error_params);
//     }
//
//     #[test]
//     fn test_process_response_v2_should_inform_set_if_set_request_found() {
//         let mut rng = rand::thread_rng();
//         let operation_code = rng.gen_range(1..u8::MAX);
//         let operation = Operation::Set;
//         let instruction_code = ADD_DATA_INSTRUCTION_CODES[
//             rng.gen_range(0..ADD_DATA_INSTRUCTION_CODES.len() as usize)];
//         let mut data: Vec<u8> = Vec::new();
//         data.push(instruction_code as u8);
//         for i in 1..8 {
//             data.push(rng.gen_range(1..u8::MAX));
//         }
//         let data = data.as_slice();
//         let id: u32 = rng.next_u32();
//
//         let parsed_data = [rng.gen(), rng.gen()];
//
//         let mock_responses_parser = MockResponseBodyParser::new(
//             || Err(Errors::NotEnoughDataGot), false,
//             Ok(Some(id))
//         );
//         let mock_response_handler = MockResponsesHandler::new();
//
//         let mock_response_parser = MockResponseParser::new(
//             MockResponsePreParseResult {
//                 operation: operation,
//                 instruction: DataInstructionCodes::Id(0),
//                 request_id: None,
//                 needs_cache: false,
//                 error_code: ErrorCode::Ok,
//                 data: parsed_data.to_vec(),
//             }, parsed_data.to_vec() );
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser, Version::V2);
//
//         let request = SentRequest::new(
//             Some(id), operation, instruction_code, RelativeMillis::new(rng.next_u32()));
//
//         tested.add_sent_request(request);
//         *tested.response_body_parser.request_needs_cache_param.borrow_mut() = None;
//
//         tested.process_response(operation_code, &data);
//
//         assert_eq!(Some(data[1..].to_vec()), *tested.response_body_parser.parse_id_param.borrow());
//         assert_eq!(Some(operation_code), *tested.response_body_parser.parse_operation_param.borrow());
//         assert_eq!(Some(request), tested.response_handler.on_request_success_params);
//         //nothing else should be called
//         assert_eq!(None, tested.response_handler.on_request_process_error_params);
//         assert_eq!(None, tested.response_handler.on_request_response_params);
//         assert_eq!(None, tested.response_handler.on_request_error_params);
//         assert_eq!(None, *tested.response_body_parser.parse_response_params.borrow());
//         assert_eq!(None, *tested.response_body_parser.request_needs_cache_param.borrow());
//         assert_eq!(None, tested.response_handler.on_request_search_error_params);
//     }
//
//     #[test]
//     fn test_process_response_v1_should_proxy_response_if_read_request_found() {
//         let mut rng = rand::thread_rng();
//         let operation_code = rng.gen_range(1..u8::MAX);
//         let operation = Operation::Read;
//         let instruction_code = ADD_DATA_INSTRUCTION_CODES[
//             rng.gen_range(0..ADD_DATA_INSTRUCTION_CODES.len() as usize)];
//         let data = [instruction_code as u8, rng.gen_range(1..u8::MAX),
//             rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)];
//
//
//         let parse_response_result_producer = || Ok(DataInstructions::Id(Conversation::Data(123)));
//
//         let mock_responses_parser = MockResponseBodyParser::new(
//             parse_response_result_producer, false, Err(Errors::NotEnoughDataGot),
//             Version::V1
//         );
//         let mock_response_handler = MockResponsesHandler::new();
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser);
//
//         let request = SentRequest::new(
//             None, operation, instruction_code, RelativeMillis::new(rng.next_u32()));
//
//         let mock_response_parser = MockResponseParser::new(
//             MockResponsePreParseResult {
//                 operation: operation,
//                 instruction: DataInstructionCodes::Id(0),
//                 request_id: None,
//                 needs_cache: false,
//                 error_code: ErrorCode::Ok,
//                 data: parsed_data.to_vec(),
//             }, parsed_data.to_vec() );
//
//         tested.add_sent_request(request);
//         *tested.response_body_parser.request_needs_cache_param.borrow_mut() = None;
//
//         tested.process_response(operation_code, &data);
//
//         assert_eq!(Some(operation_code), *tested.response_body_parser.parse_operation_param.borrow());
//         assert_eq!(Some(instruction_code), *tested.response_body_parser.request_needs_cache_param.borrow());
//         assert_eq!(Some((instruction_code as u8, data[1..].to_vec())), *tested.response_body_parser.parse_response_params.borrow());
//         let response = parse_response_result_producer().unwrap();
//         assert_eq!(Some((request, response)), tested.response_handler.on_request_response_params);
//         //nothing else should be called
//         assert_eq!(None, *tested.response_body_parser.parse_id_param.borrow());
//         assert_eq!(None, tested.response_handler.on_request_process_error_params);
//         assert_eq!(None, tested.response_handler.on_request_success_params);
//         assert_eq!(None, tested.response_handler.on_request_error_params);
//         assert_eq!(None, tested.response_handler.on_request_search_error_params);
//     }
//
//     #[test]
//     fn test_process_response_v2_should_proxy_response_if_read_request_found() {
//         let mut rng = rand::thread_rng();
//         let operation_code = rng.gen_range(1..u8::MAX);
//         let operation = Operation::Read;
//         let instruction_code = ADD_DATA_INSTRUCTION_CODES[
//             rng.gen_range(0..ADD_DATA_INSTRUCTION_CODES.len() as usize)];
//         let mut data: Vec<u8> = Vec::new();
//         data.push(instruction_code as u8);
//         for i in 1..8 {
//             data.push(rng.gen_range(1..u8::MAX));
//         }
//         let data = data.as_slice();
//         let id: u32 = rng.next_u32();
//
//         let parsed_data = [rng.gen(), rng.gen()];
//
//         let parse_response_result_producer = || Ok(DataInstructions::Id(Conversation::Data(123)));
//
//         let mock_responses_parser = MockResponseBodyParser::new(
//             parse_response_result_producer, false, Ok(Some(id)) );
//         let mock_response_handler = MockResponsesHandler::new();
//
//         let mock_response_parser = MockResponseParser::new(
//             MockResponsePreParseResult {
//                 operation: operation,
//                 instruction: DataInstructionCodes::Id(0),
//                 request_id: None,
//                 needs_cache: false,
//                 error_code: ErrorCode::Ok,
//                 data: parsed_data.to_vec(),
//             }, parsed_data.to_vec() );
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser, Version::V2);
//
//         let request = SentRequest::new(
//             Some(id), operation, instruction_code, RelativeMillis::new(rng.next_u32()));
//
//         tested.add_sent_request(request);
//         *tested.response_body_parser.request_needs_cache_param.borrow_mut() = None;
//
//         tested.process_response(operation_code, &data);
//
//         assert_eq!(Some(data[1..].to_vec()), *tested.response_body_parser.parse_id_param.borrow());
//         assert_eq!(Some(operation_code), *tested.response_body_parser.parse_operation_param.borrow());
//         assert_eq!(Some(instruction_code), *tested.response_body_parser.request_needs_cache_param.borrow());
//         assert_eq!(Some((instruction_code as u8, data[5..].to_vec())), *tested.response_body_parser.parse_response_params.borrow());
//         let response = parse_response_result_producer().unwrap();
//         assert_eq!(Some((request, response)), tested.response_handler.on_request_response_params);
//         //nothing else should be called
//         assert_eq!(None, tested.response_handler.on_request_process_error_params);
//         assert_eq!(None, tested.response_handler.on_request_success_params);
//         assert_eq!(None, tested.response_handler.on_request_error_params);
//         assert_eq!(None, tested.response_handler.on_request_search_error_params);
//     }
//
//     #[test]
//     fn test_process_response_v1_should_proxy_response_parse_error_if_read_request_found() {
//         let mut rng = rand::thread_rng();
//         let operation_code = rng.gen_range(1..u8::MAX);
//         let operation = Operation::Read;
//         let instruction_code = ADD_DATA_INSTRUCTION_CODES[
//             rng.gen_range(0..ADD_DATA_INSTRUCTION_CODES.len() as usize)];
//         let data = [instruction_code as u8, rng.gen_range(1..u8::MAX),
//             rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)];
//
//
//         let parse_response_result_producer = || Err(Errors::NotEnoughDataGot);
//
//         let mock_responses_parser = MockResponseBodyParser::new(
//             parse_response_result_producer, false,
//             Err(Errors::NotEnoughDataGot), Version::V1
//         );
//         let mock_response_handler = MockResponsesHandler::new();
//
//         let mock_response_parser = MockResponseParser::new(
//             MockResponsePreParseResult {
//                 operation: operation,
//                 instruction: DataInstructionCodes::Id(0),
//                 request_id: None,
//                 needs_cache: false,
//                 error_code: ErrorCode::Ok,
//                 data: parsed_data.to_vec(),
//             }, parsed_data.to_vec() );
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser);
//
//         let request = SentRequest::new(
//             None, operation, instruction_code, RelativeMillis::new(rng.next_u32()));
//
//         tested.add_sent_request(request);
//         *tested.response_body_parser.request_needs_cache_param.borrow_mut() = None;
//
//         tested.process_response(operation_code, &data);
//
//         assert_eq!(Some(operation_code), *tested.response_body_parser.parse_operation_param.borrow());
//         assert_eq!(Some(instruction_code), *tested.response_body_parser.request_needs_cache_param.borrow());
//         assert_eq!(Some((instruction_code as u8, data[1..].to_vec())), *tested.response_body_parser.parse_response_params.borrow());
//         let response = parse_response_result_producer().unwrap_err();
//         assert_eq!(Some((request, response, data[1..].to_vec())), tested.response_handler.on_request_process_error_params);
//         //nothing else should be called
//         assert_eq!(None, *tested.response_body_parser.parse_id_param.borrow());
//         assert_eq!(None, tested.response_handler.on_request_success_params);
//         assert_eq!(None, tested.response_handler.on_request_response_params);
//         assert_eq!(None, tested.response_handler.on_request_error_params);
//         assert_eq!(None, tested.response_handler.on_request_search_error_params);
//     }
//
//     #[test]
//     fn test_process_response_v2_should_proxy_response_parse_error_if_read_request_found() {
//         let mut rng = rand::thread_rng();
//         let operation_code = rng.gen_range(1..u8::MAX);
//         let operation = Operation::Read;
//         let instruction_code = ADD_DATA_INSTRUCTION_CODES[
//             rng.gen_range(0..ADD_DATA_INSTRUCTION_CODES.len() as usize)];
//         let mut data: Vec<u8> = Vec::new();
//         data.push(instruction_code as u8);
//         for i in 1..8 {
//             data.push(rng.gen_range(1..u8::MAX));
//         }
//         let data = data.as_slice();
//         let id: u32 = rng.next_u32();
//
//         let parse_response_result_producer = || Err(Errors::NotEnoughDataGot);
//
//         let parsed_data = [rng.gen(), rng.gen()];
//
//         let mock_responses_parser = MockResponseBodyParser::new(
//             parse_response_result_producer, false, Ok(Some(id)) );
//         let mock_response_handler = MockResponsesHandler::new();
//
//         let mock_response_parser = MockResponseParser::new(
//             MockResponsePreParseResult {
//                 operation: operation,
//                 instruction: DataInstructionCodes::Id(0),
//                 request_id: None,
//                 needs_cache: false,
//                 error_code: ErrorCode::Ok,
//                 data: parsed_data.to_vec(),
//             }, parsed_data.to_vec() );
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser, Version::V2);
//
//         let request = SentRequest::new(
//             Some(id), operation, instruction_code, RelativeMillis::new(rng.next_u32()));
//
//         tested.add_sent_request(request);
//         *tested.response_body_parser.request_needs_cache_param.borrow_mut() = None;
//
//         tested.process_response(operation_code, &data);
//
//         assert_eq!(Some(data[1..].to_vec()), *tested.response_body_parser.parse_id_param.borrow());
//         assert_eq!(Some(operation_code), *tested.response_body_parser.parse_operation_param.borrow());
//         assert_eq!(Some(instruction_code), *tested.response_body_parser.request_needs_cache_param.borrow());
//         assert_eq!(Some((instruction_code as u8, data[5..].to_vec())), *tested.response_body_parser.parse_response_params.borrow());
//         let response = parse_response_result_producer().unwrap_err();
//         assert_eq!(Some((request, response, data[5..].to_vec())), tested.response_handler.on_request_process_error_params);
//         //nothing else should be called
//         assert_eq!(None, tested.response_handler.on_request_success_params);
//         assert_eq!(None, tested.response_handler.on_request_response_params);
//         assert_eq!(None, tested.response_handler.on_request_error_params);
//         assert_eq!(None, tested.response_handler.on_request_search_error_params);
//     }
//
//
//     #[test]
//     fn test_process_response_v1_should_proxy_response_error() {
//         let mut rng = rand::thread_rng();
//         let operation_code = rng.gen_range(1..u8::MAX);
//         let operation = Operation::Error;
//         let instruction_code = ADD_DATA_INSTRUCTION_CODES[
//             rng.gen_range(0..ADD_DATA_INSTRUCTION_CODES.len() as usize)];
//         let error_code = ALL_ERROR_CODES[rng.gen_range(1..ALL_ERROR_CODES.len() as usize)];
//         let data = [instruction_code as u8, rng.gen_range(1..u8::MAX),
//             rng.gen_range(1..u8::MAX), rng.gen_range(1..u8::MAX)];
//
//         let parse_response_result_producer = || Err(Errors::NotEnoughDataGot);
//
//         let mock_responses_parser = MockResponseBodyParser::new(
//             parse_response_result_producer, false,
//             Err(Errors::NotEnoughDataGot), Version::V1
//         );
//         let mock_response_handler = MockResponsesHandler::new();
//
//         let mock_response_parser = MockResponseParser::new(
//             MockResponsePreParseResult {
//                 operation: operation,
//                 instruction: DataInstructionCodes::Id(0),
//                 request_id: None,
//                 needs_cache: false,
//                 error_code: ErrorCode::Ok,
//                 data: parsed_data.to_vec(),
//             }, parsed_data.to_vec() );
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser);
//
//         let request = SentRequest::new(
//             None, operation, instruction_code, RelativeMillis::new(rng.next_u32()));
//
//         tested.add_sent_request(request);
//         *tested.response_body_parser.request_needs_cache_param.borrow_mut() = None;
//
//         tested.process_response(operation_code, &data);
//
//         assert_eq!(Some(operation_code), *tested.response_body_parser.parse_operation_param.borrow());
//         assert_eq!(Some(instruction_code), *tested.response_body_parser.request_needs_cache_param.borrow());
//         assert_eq!(Some((request, ErrorCode::for_code(error_code.discriminant()))),
//                tested.response_handler.on_request_error_params);
//         //nothing else should be called
//         assert_eq!(None, *tested.response_body_parser.parse_id_param.borrow());
//         assert_eq!(None, tested.response_handler.on_request_success_params);
//         assert_eq!(None, tested.response_handler.on_request_response_params);
//         assert_eq!(None, *tested.response_body_parser.parse_response_params.borrow());
//         assert_eq!(None, tested.response_handler.on_request_process_error_params);
//         assert_eq!(None, tested.response_handler.on_request_search_error_params);
//     }
//
//     #[test]
//     fn test_process_response_v2_should_proxy_response_error() {
//         let mut rng = rand::thread_rng();
//         let operation_code = rng.gen_range(1..u8::MAX);
//         let operation = Operation::Error;
//         let instruction_code = ADD_DATA_INSTRUCTION_CODES[
//             rng.gen_range(0..ADD_DATA_INSTRUCTION_CODES.len() as usize)];
//         let mut data: Vec<u8> = Vec::new();
//         data.push(instruction_code as u8);
//         for i in 1..8 {
//             data.push(rng.gen_range(1..u8::MAX));
//         }
//         let data = data.as_slice();
//         let id: u32 = rng.next_u32();
//
//         let parse_response_result_producer = || Err(Errors::NotEnoughDataGot);
//
//         let parsed_data = [1_u8];
//
//         let mock_responses_parser = MockResponseBodyParser::new(
//             parse_response_result_producer, false, Ok(Some(id))
//         );
//         let mock_response_handler = MockResponsesHandler::new();
//
//         let mock_response_parser = MockResponseParser::new(
//             MockResponsePreParseResult {
//                 operation: operation,
//                 instruction: DataInstructionCodes::Id(0),
//                 request_id: None,
//                 needs_cache: false,
//                 error_code: ErrorCode::Ok,
//                 data: parsed_data.to_vec(),
//             }, parsed_data.to_vec() );
//
//         let mut tested =
//             RequestsController::new(mock_response_handler, mock_responses_parser, Version::V2);
//
//         let request = SentRequest::new(
//             Some(id), operation, instruction_code, RelativeMillis::new(rng.next_u32()));
//
//         tested.add_sent_request(request);
//         *tested.response_body_parser.request_needs_cache_param.borrow_mut() = None;
//
//         tested.process_response(operation_code, &data);
//
//         assert_eq!(Some(data[1..].to_vec()), *tested.response_body_parser.parse_id_param.borrow());
//         assert_eq!(Some(operation_code), *tested.response_body_parser.parse_operation_param.borrow());
//         assert_eq!(Some(instruction_code), *tested.response_body_parser.request_needs_cache_param.borrow());
//         assert_eq!(Some((instruction_code as u8, data[5..].to_vec())), *tested.response_body_parser.parse_response_params.borrow());
//         let response = parse_response_result_producer().unwrap_err();
//         assert_eq!(Some((request, response, data[5..].to_vec())), tested.response_handler.on_request_process_error_params);
//         //nothing else should be called
//         assert_eq!(None, tested.response_handler.on_request_success_params);
//         assert_eq!(None, tested.response_handler.on_request_response_params);
//         assert_eq!(None, tested.response_handler.on_request_error_params);
//         assert_eq!(None, tested.response_handler.on_request_search_error_params);
//     }
// */
//
//
//     const ALL_ERROR_CODES: [ErrorCode; 16] = [
//         ErrorCode::OK,
//         ErrorCode::ERequestDataNoValue,
//         ErrorCode::EInstructionUnrecognized,
//         ErrorCode::ECommandEmpty,
//         ErrorCode::ECommandSizeOverflow,
//         ErrorCode::EInstructionWrongStart,
//         ErrorCode::EWriteMaxAttemptsExceeded,
//         ErrorCode::EUndefinedOperation,
//         ErrorCode::ERelayCountOverflow,
//         ErrorCode::ERelayCountAndDataMismatch,
//         ErrorCode::ERelayIndexOutOfRange,
//         ErrorCode::ESwitchCountMaxValueOverflow,
//         ErrorCode::EControlInterruptedPinNotAllowedValue,
//         ErrorCode::EInternalError,
//         ErrorCode::ERelayNotAllowedPinUsed,
//         ErrorCode::EUndefinedCode(128),
//     ];
//
//     const ADD_DATA_INSTRUCTION_CODES: [DataInstructionCodes; 22] = [
//         DataInstructionCodes::None,
//         DataInstructionCodes::Settings,
//         DataInstructionCodes::State,
//         DataInstructionCodes::Id,
//         DataInstructionCodes::InterruptPin,
//         DataInstructionCodes::RemoteTimestamp,
//         DataInstructionCodes::StateFixSettings,
//         DataInstructionCodes::RelayState,
//         DataInstructionCodes::Version,
//         DataInstructionCodes::CurrentTime,
//         DataInstructionCodes::ContactWaitData,
//         DataInstructionCodes::FixData,
//         DataInstructionCodes::SwitchData,
//         DataInstructionCodes::CyclesStatistics,
//         DataInstructionCodes::SwitchCountingSettings,
//         DataInstructionCodes::RelayDisabledTemp,
//         DataInstructionCodes::RelaySwitchedOn,
//         DataInstructionCodes::RelayMonitorOn,
//         DataInstructionCodes::RelayControlOn,
//         DataInstructionCodes::All,
//         DataInstructionCodes::Last,
//         DataInstructionCodes::Unknown, ];
//
//
//     const ALL_ERRORS: [Errors; 25] = [
//         Errors::NoBufferAvailable,
//         Errors::TransferInProgress,
//         Errors::DmaBufferOverflow,
//         Errors::CommandDataCorrupted,
//         Errors::NotEnoughDataGot,
//         Errors::OperationNotRecognized(128),
//         Errors::InstructionNotRecognized(129),
//         Errors::DataCorrupted,
//         Errors::DmaError(DMAError::NotReady(())),
//         Errors::RequestsLimitReached,
//         Errors::RequestsNeedsCacheAlreadySent,
//         Errors::NoRequestsFound,
//         Errors::UndefinedOperation,
//         Errors::SentRequestsQueueIsEmpty,
//         Errors::RelayIndexOutOfRange,
//         Errors::RelayCountOverflow,
//         Errors::SlaveControllersInstancesMaxCountReached,
//         Errors::FromAfterTo,
//         Errors::OutOfRange,
//         Errors::SwitchesDataCountOverflow,
//         Errors::InvalidDataSize,
//         Errors::InstructionNotSerializable,
//         Errors::WrongStateNotParsed,
//         Errors::WrongStateIncompatibleOperation(Operation::Command),
//         Errors::WrongIncomingOperation(Operation::Command),
//     ];
//
//     pub struct ResponsePayloadParsedTestData {
//         operation: Operation,
//         instruction: DataInstructionCodes,
//         request_id: Option<u32>,
//         needs_cache: bool,
//         error_code: ErrorCode,
//         body_parser_id: Option<u32>,
//     }
//
//     struct MockResponsesHandler {
//         on_request_success_params: Option<SentRequest>,
//         on_request_error_params: Option<(SentRequest, ErrorCode)>,
//         on_request_parse_error_params: Option<(Option<SentRequest>, Errors, Vec<u8>)>,
//         on_request_response_params: Option<(SentRequest, DataInstructions)>,
//         on_request_search_error_params: Option<(ResponsePayloadParsedTestData, Vec<u8>, Errors)>,
//     }
//
//     impl MockResponsesHandler {
//         pub fn new() -> Self {
//             Self {
//                 on_request_success_params: None,
//                 on_request_error_params: None,
//                 on_request_parse_error_params: None,
//                 on_request_response_params: None,
//                 on_request_search_error_params: None,
//             }
//         }
//     }
//
//     trait IdContainer {
//         fn id(&self) -> u32;
//     }
//
//     impl ResponseHandler<MockResponsePostParser> for MockResponsesHandler {
//         fn on_request_success(&mut self, request: SentRequest) {
//             self.on_request_success_params = Some(request);
//         }
//
//         fn on_request_response(&mut self, request: SentRequest, response: DataInstructions) {
//             self.on_request_response_params = Some((request, response));
//         }
//
//         fn on_request_error(&mut self, request: SentRequest, error_code: ErrorCode) {
//             self.on_request_error_params = Some((request, error_code));
//         }
//
//         fn on_request_parse_error(&mut self, request: Option<SentRequest>, error: Errors, data: &[u8]) {
//             self.on_request_parse_error_params = Some((request, error, data.to_vec()));
//         }
//
//         fn on_request_search_error(&mut self, payload: MockResponsePostParser, error: Errors) {
//             let parsed_data = ResponsePayloadParsedTestData {
//                 operation: payload.operation(),
//                 instruction: payload.instruction(),
//                 request_id: payload.request_id(),
//                 needs_cache: payload.needs_cache(),
//                 error_code: payload.error_code(),
//                 body_parser_id: payload.body_parser_id(),
//             };
//             self.on_request_search_error_params = Some((parsed_data, payload.data().to_vec(), error));
//         }
//     }
//
//     type MyRbpCb = fn() -> Result<DataInstructions, Errors>;
//
//     fn new_check_needs_cache(request_needs_cache_result: bool, ctrl_version: Version) -> MockResponseBodyParser<MyRbpCb> {
//         let cb = || Err(Errors::NoRequestsFound);
//         MockResponseBodyParser::new(cb,
//             request_needs_cache_result, Ok(Some(0)), ctrl_version)
//     }
//
//     fn default_mock_parser(slave_controller_version_result: Version) -> MockResponseBodyParser<MyRbpCb> {
//
//         let mut rng = rand::thread_rng();
//         let version = if rng.gen_range(0..2) == 0 { Version::V1 } else { Version::V2 };
//         let error = ALL_ERRORS[rng.gen_range(0..ALL_ERRORS.len() as usize)];
//
//         let cb = || Err(Errors::NoRequestsFound);
//         MockResponseBodyParser::new(cb,
//             false, Ok(Some(0)), slave_controller_version_result)
//     }
//
//     struct MockResponseBodyParser<CB: Fn() -> Result<DataInstructions, Errors>> {
//         parse_response_params: RefCell<Option<(u8, Vec<u8>)>>,
//         parse_response_result_producer: CB,
//         request_needs_cache_param: RefCell<Option<DataInstructionCodes>>,
//         request_needs_cache_result: bool,
//         parse_id_param: RefCell<Option<Vec<u8>>>,
//         parse_id_result: Result<Option<u32>, Errors>,
//         slave_controller_version_result: Version,
//         id: u32,
//     }
//
//     impl <CB: Fn() -> Result<DataInstructions, Errors>> MockResponseBodyParser<CB> {
//         pub fn new(
//             parse_response_result_producer: CB, request_needs_cache_result: bool,
//             parse_id_result: Result<Option<u32>, Errors>, slave_controller_version_result: Version,
//         ) -> Self {
//             let mut rng = rand::thread_rng();
//             Self {
//                 parse_response_params: RefCell::new(None),
//                 parse_response_result_producer,
//                 request_needs_cache_param: RefCell::new(None),
//                 request_needs_cache_result,
//                 parse_id_param: RefCell::new(None),
//                 parse_id_result,
//                 slave_controller_version_result,
//                 id: rng.gen_range(1..u32::MAX),
//             }
//         }
//     }
//
//     impl <CB: Fn() -> Result<DataInstructions, Errors>> ResponseBodyParser for MockResponseBodyParser<CB> {
//
//         fn request_needs_cache(&self, instruction: DataInstructionCodes) -> bool {
//             *self.request_needs_cache_param.borrow_mut() = Some(instruction);
//             self.request_needs_cache_result
//         }
//
//         fn parse_id<'a>(&self, data: &'a[u8]) -> Result<(Option<u32>, &'a[u8]), Errors> {
//             *self.parse_id_param.borrow_mut() = Some(data.to_vec());
//             self.parse_id_result.map(|id| (id, data))
//         }
//
//         fn parse(&self, instruction: DataInstructionCodes, data: &[u8]) -> Result<DataInstructions, Errors> {
//             *self.parse_response_params.borrow_mut() = Some((instruction as u8, data.to_vec()));
//             (self.parse_response_result_producer)()
//         }
//
//         fn slave_controller_version(&self) -> Version {
//             self.slave_controller_version_result
//         }
//
//     }
//
//     impl <CB: Fn() -> Result<DataInstructions, Errors>> IdContainer for MockResponseBodyParser<CB> {
//         fn id(&self) -> u32 {
//             self.id
//         }
//     }
//
//     #[derive(Clone)]
//     struct MockResponsePostParser {
//         operation: Operation,
//         instruction: DataInstructionCodes,
//         request_id: Option<u32>,
//         needs_cache: bool,
//         error_code: ErrorCode,
//         data: Vec<u8>,
//         body_parser_id: Option<u32>,
//     }
//
//     impl MockResponsePostParser {
//
//         fn new(operation: Operation, instruction: DataInstructionCodes, request_id: Option<u32>, needs_cache: bool,
//                error_code: ErrorCode, data: Vec<u8>) -> Self {
//             Self {operation, instruction, request_id, needs_cache, error_code, data, body_parser_id: None}
//         }
//
//         fn clone_partial(other: &Self, body_parser_id: Option<u32>) -> Self {
//             Self { body_parser_id, ..other.clone() }
//         }
//
//         fn body_parser_id(&self) -> Option<u32> {
//             self.body_parser_id
//         }
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
//     impl <CB: Fn() -> Result<DataInstructions, Errors>> ResponseDataParser<MockResponseBodyParser<CB>> for MockResponsePostParser {
//         fn parse(&self, response_body_parser: &MockResponseBodyParser<CB>) -> Result<DataInstructions, Errors> {
//             unimplemented!()
//         }
//     }
//
//     struct MockResponseParserLight{
//         parse_result: Result<MockResponsePostParser, Errors>,
//         data_result: Vec<u8>,
//         id: u32,
//     }
//
//     impl MockResponseParserLight{
//         fn new(parse_result: Result<MockResponsePostParser, Errors>, data_result: Vec<u8>) -> Self {
//             let mut rng = rand::thread_rng();
//             Self {
//                 parse_result,
//                 data_result,
//                 id: rng.gen_range(1..u32::MAX),
//             }
//         }
//     }
//
//     type MockResponseBodyParserLight = MockResponseBodyParser<fn() -> Result<DataInstructions, Errors>>;
//
//     impl <'a> ResponseParser<'a, MockResponseBodyParserLight, MockResponsePostParser> for MockResponseParserLight {
//
//         fn parse(&'a self, body_parser: &MockResponseBodyParserLight) -> Result<MockResponsePostParser, Errors> {
//             let data: &Vec<u8> = self.data_result.as_ref();
//             self.parse_result.as_ref()
//                 .map(|res| {
//                     MockResponsePostParser::clone_partial(res, Some(body_parser.id()))
//                 })
//                 .map_err(|err| err.clone())
//         }
//
//         fn data(&self) -> &[u8] {
//             self.data_result.as_slice()
//         }
//     }
//
//     struct MockResponseParser<CB, CB2>
//         where
//             CB: Fn(&MockResponseBodyParser<CB2>),
//             CB2: Fn() -> Result<DataInstructions, Errors>
//     {
//         parse_result: Result<MockResponsePostParser, Errors>,
//         check_parse_callback: CB,
//         data_result: Vec<u8>,
//         id: u32,
//         _rbp: PhantomData<CB2>,
//     }
//
//     impl <CB, CB2> MockResponseParser<CB, CB2>
//         where
//             CB: Fn(&MockResponseBodyParser<CB2>),
//             CB2: Fn() -> Result<DataInstructions, Errors>
//     {
//         fn new(parse_result: Result<MockResponsePostParser, Errors>, data_result: Vec<u8>,
//                check_parse_param: CB) -> Self {
//             let mut rng = rand::thread_rng();
//             Self {
//                 parse_result,
//                 check_parse_callback: check_parse_param,
//                 data_result,
//                 id: rng.gen_range(1..u32::MAX),
//                 _rbp: PhantomData,
//             }
//         }
//     }
//
//     impl <CB, CB2> IdContainer for MockResponseParser<CB, CB2>
//         where
//             CB: Fn(&MockResponseBodyParser<CB2>),
//             CB2: Fn() -> Result<DataInstructions, Errors>
//     {
//         fn id(&self) -> u32 {
//             self.id
//         }
//     }
//
//     impl <'a, CB, CB2>  ResponseParser<'a, MockResponseBodyParser<CB2>, MockResponsePostParser> for MockResponseParser<CB, CB2>
//         where
//             CB: Fn(&MockResponseBodyParser<CB2>),
//             CB2: Fn() -> Result<DataInstructions, Errors>
//     {
//         fn parse(&'a self, body_parser: &MockResponseBodyParser<CB2>) -> Result<MockResponsePostParser, Errors> {
//             (self.check_parse_callback)(body_parser);
//             let data: &Vec<u8> = self.data_result.as_ref();
//             self.parse_result.as_ref()
//                 .map(|res| {
//                     MockResponsePostParser::clone_partial(res, Some(body_parser.id()))
//                 })
//                 .map_err(|err| err.clone())
//         }
//
//         fn data(&self) -> &[u8] {
//             self.data_result.as_slice()
//         }
//     }
// }