use http;

enum RequestReadPart {
    StartLine,
    Headers,
    Body,
}

enum RequestReadStartLinePart {
    Method,
    Path,
    Protocol,
}

enum RequestReadHeaderPart {
    Key,
    Value,
}

pub fn read_http_request(request_stream: &mut std::net::TcpStream) -> Result<http::Request<String>, std::io::Error> {
    let mut request: http::Request<String> = http::Request::default();
    let mut request_read_error: Option<std::io::Error> = None;

    // NOTE: This sets a short read-timeout on the stream to ensure an empty stream can return 0,
    // i.e. so that the "end" of an incoming request can be detected with a non-blocking 0 result.
    let timeout_result = request_stream.set_read_timeout(Some(std::time::Duration::from_millis(100)));
    if timeout_result.is_err() {
        return Err(timeout_result.unwrap_err());
    }

    let request_readstream = request_stream as &mut dyn std::io::Read;

    let mut request_buffer = [0u8; 4 * 1024];
    let mut request_read_part = RequestReadPart::StartLine;
    let mut request_read_start_line_part = RequestReadStartLinePart::Method;
    let mut request_read_header_part = RequestReadHeaderPart::Key;
    let mut request_read_current_header_name: Option<http::HeaderName> = None;
    loop {
        let read_result = request_readstream.read(&mut request_buffer);
        match read_result {
            Ok(read_len) => {
                if read_len == 0 {
                    break;
                }

                let mut read_pos = 0;
                while read_pos < read_len {
                    match request_read_part {
                        RequestReadPart::StartLine => {
                            // A start line looks like: "GET /path HTTP/1.1"
                            // - The request method (verb) followed by spaces,
                            // - Then a request path followed by spaces,
                            // - Then a protocol name and version specifier followed by a line break.
                            match request_read_start_line_part {
                                RequestReadStartLinePart::Method => {
                                    // Find the next ' ' to parse out the method string.
                                    let method_end = request_buffer[read_pos..].iter().position(|&b| b == b' ');
                                    match method_end {
                                        Some(method_len) => {
                                            // Parse the method string into an http::Method value.
                                            let method_bytes = &request_buffer[read_pos..read_pos+method_len];
                                            log::debug!("request method {}", std::str::from_utf8(method_bytes).unwrap());

                                            let method = http::Method::from_bytes(method_bytes).unwrap();
                                            *request.method_mut() = method;

                                            // Continue parsing the next part of the start line (the request path).
                                            read_pos += method_len + 1;
                                            while request_buffer[read_pos] == b' ' {
                                                read_pos += 1;
                                            }
                                            request_read_start_line_part = RequestReadStartLinePart::Path;
                                            continue;
                                        },
                                        None => {
                                            request_read_error = Some(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Failed to parse HTTP request method"));
                                            break;
                                        }
                                    }
                                },
                                RequestReadStartLinePart::Path => {
                                    // Find the next ' ' to parse out the path string.
                                    let path_end = request_buffer[read_pos..].iter().position(|&b| b == b' ');
                                    match path_end {
                                        Some(path_len) => {
                                            // Parse the request path.
                                            let path_bytes = &request_buffer[read_pos..read_pos+path_len];
                                            log::debug!("request path {}", std::str::from_utf8(path_bytes).unwrap());

                                            let uri_path = http::uri::PathAndQuery::try_from(path_bytes).unwrap();
                                            let mut uri_parts = http::uri::Parts::default();
                                            uri_parts.path_and_query = Some(uri_path);
                                            *request.uri_mut() = http::uri::Uri::from_parts(uri_parts).unwrap();

                                            // Continue parsing the next part of the start line (the request protocol).
                                            read_pos += path_len + 1;
                                            while request_buffer[read_pos] == b' ' {
                                                read_pos += 1;
                                            }
                                            request_read_start_line_part = RequestReadStartLinePart::Protocol;
                                            continue;
                                        },
                                        None => {
                                            request_read_error = Some(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Failed to parse HTTP request path"));
                                            break;
                                        }
                                    }
                                },
                                RequestReadStartLinePart::Protocol => {
                                    // Find the next line-break to parse out the protocol string.
                                    let protocol_end = request_buffer[read_pos..].iter().position(|&b| b == b'\r' || b == b'\n');
                                    match protocol_end {
                                        Some(protocol_len) => {
                                            // Parse the protocol string into an http::Version value.
                                            let protocol_bytes = &request_buffer[read_pos..read_pos+protocol_len];
                                            log::debug!("request protocol {}", std::str::from_utf8(protocol_bytes).unwrap());
                                            
                                            match std::str::from_utf8(protocol_bytes).unwrap() {
                                                "HTTP/0.9" => {
                                                    *request.version_mut() = http::Version::HTTP_09;
                                                },
                                                "HTTP/1.0" => {
                                                    *request.version_mut() = http::Version::HTTP_10;
                                                },
                                                "HTTP/1.1" => {
                                                    *request.version_mut() = http::Version::HTTP_11;
                                                },
                                                "HTTP/2.0" => {
                                                    *request.version_mut() = http::Version::HTTP_2;
                                                },
                                                "HTTP/3.0" => {
                                                    *request.version_mut() = http::Version::HTTP_3;
                                                },
                                                _ => {
                                                    log::warn!("Unrecognized request protocol {} falling back to HTTP/1.0", std::str::from_utf8(protocol_bytes).unwrap());
                                                    *request.version_mut() = http::Version::HTTP_10;
                                                }
                                            }

                                            // Continue parsing the next part of the request (headers).
                                            // This SHOULD put read_pos on a '\r' immediately preceding a '\n' ...
                                            read_pos += protocol_len + 1;
                                            // ... then set read_pos to the start of the next line.
                                            if request_buffer[read_pos] == b'\n' {
                                                read_pos += 1;
                                            }
                                            request_read_part = RequestReadPart::Headers;
                                            continue;
                                        },
                                        None => {
                                            request_read_error = Some(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Failed to parse HTTP request protocol"));
                                            break;
                                        }
                                    }
                                }
                            }
                        },
                        RequestReadPart::Headers => {
                            // A header line looks like: "content-type: text/something;extrabits"
                            // - The header key (name) followed by a colon and spaces,
                            // - Then the header value followed by a line break.
                            // Header lines continue until a double line break, indicating the start of the request body.
                            match request_read_header_part {
                                RequestReadHeaderPart::Key => {
                                    // If the line ends immediately, then we're done reading headers, and are on to the request body.
                                    if request_buffer[read_pos] == b'\r' || request_buffer[read_pos] == b'\n' {
                                        read_pos += 1;
                                        if request_buffer[read_pos] == b'\n' {
                                            read_pos += 1;
                                        }
                                        request_read_part = RequestReadPart::Body;
                                        continue;
                                    }

                                    // Find the next ':' to parse out the key string.
                                    let key_end = request_buffer[read_pos..].iter().position(|&b| b == b':');
                                    match key_end {
                                        Some(key_len) => {
                                            // Parse the header key.
                                            let key_bytes = &request_buffer[read_pos..read_pos+key_len];
                                            log::debug!("request header key {}", std::str::from_utf8(key_bytes).unwrap());

                                            request_read_current_header_name = Some(http::HeaderName::try_from(key_bytes).unwrap());

                                            // Continue parsing the next part of the header line (the value).
                                            read_pos += key_len + 1;
                                            while request_buffer[read_pos] == b' ' {
                                                read_pos += 1;
                                            }
                                            request_read_header_part = RequestReadHeaderPart::Value;
                                            continue;
                                        },
                                        None => {
                                            request_read_error = Some(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Failed to parse HTTP request header key"));
                                            break;
                                        }
                                    }
                                },
                                RequestReadHeaderPart::Value => {
                                    // Find the next line-break to parse out the value string.
                                    let value_end = request_buffer[read_pos..].iter().position(|&b| b == b'\r' || b == b'\n');
                                    match value_end {
                                        Some(value_len) => {
                                            // Parse the header value.
                                            let value_bytes = &request_buffer[read_pos..read_pos+value_len];
                                            log::debug!("request header value {}", std::str::from_utf8(value_bytes).unwrap());

                                            let header_value = http::HeaderValue::try_from(value_bytes).unwrap();
                                            request.headers_mut().append(request_read_current_header_name.clone().unwrap(), header_value);

                                            // Continue parsing the next part of the request (another header, or the body).
                                            // This SHOULD put read_pos on a '\r' immediately preceding a '\n' ...
                                            read_pos += value_len + 1;
                                            // ... then set read_pos to the start of the next line.
                                            if request_buffer[read_pos] == b'\n' {
                                                read_pos += 1;
                                            }
                                            request_read_header_part = RequestReadHeaderPart::Key;
                                            continue;
                                        },
                                        None => {
                                            request_read_error = Some(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Failed to parse HTTP request protocol"));
                                            break;
                                        }
                                    }
                                }
                            }
                        },
                        RequestReadPart::Body => {
                            // The request body continues until the end of the request stream.
                            let body_bytes = &request_buffer[read_pos..read_len];
                            let body_string = std::str::from_utf8(body_bytes).unwrap();
                            request.body_mut().push_str(&body_string);

                            // Continue parsing the next chunk of bytes.
                            read_pos = read_len;
                        }
                    }
                }
                if request_read_error.is_some() {
                    break;
                }
            },
            Err(readstream_error) => {
                if readstream_error.kind() == std::io::ErrorKind::WouldBlock {
                    // No new data, but no real error, either.
                    break;
                }

                request_read_error = Some(readstream_error);
                break;
            }
        }
    }

    if request_read_error.is_some() {
        return Err(request_read_error.unwrap());
    }
    
    return Ok(request);
}

pub fn create_text_response(status: http::StatusCode, text: &str) -> http::Response<String> {
    let response_length = text.len() + 2; // add 2 bytes for the trailing line break

    let mut response: http::Response<String> = http::Response::default();

    *response.status_mut() = status;
    response.headers_mut().append(http::header::CONTENT_TYPE, "text/plain".parse().unwrap());
    response.headers_mut().append(http::header::CONTENT_LENGTH, response_length.into());
    response.body_mut().push_str(text);

    return response;
}
