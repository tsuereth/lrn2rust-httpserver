use http;
use log;
use structured_logger;

struct ServerControl {
    should_stop: bool,
}

fn handle_request_stream(request_stream: &mut std::net::TcpStream) -> ServerControl {
    let mut control_result = ServerControl {
        should_stop: false,
    };

    let mut request_http_version: http::Version = http::Version::HTTP_10;
    let response: http::Response<String>;
    match lrn2rust_httpserver::read_http_request(request_stream) {
        Ok(request) => {
            log::info!("Read request: {} {}", request.method(), request.uri());

            request_http_version = request.version();

            let response_status: http::StatusCode;
            let response_body: String;
            match request.uri().path() {
                "/" => {
                    response_status = http::StatusCode::OK;
                    response_body = "Hello!".parse().unwrap();
                },
                "/stop" => {
                    control_result.should_stop = true;

                    response_status = http::StatusCode::OK;
                    response_body = "Goodbye.".parse().unwrap();
                },
                _ => {
                    response_status = http::StatusCode::NOT_FOUND;
                    response_body = format!("Unrecognized path {}", request.uri().path());
                }
            }

            response = lrn2rust_httpserver::create_text_response(response_status, response_body.as_str());
        },
        Err(read_error) => {
            log::error!("Request read error: {}", read_error);

            let response_body = &read_error.to_string();

            response = lrn2rust_httpserver::create_text_response(http::StatusCode::BAD_REQUEST, response_body);
        }
    }

    let response_writestream = request_stream as &mut dyn std::io::Write;

    let http_version_string: &str;
    match request_http_version {
        http::Version::HTTP_09 => {
            http_version_string = "HTTP/0.9";
        },
        http::Version::HTTP_10 => {
            http_version_string = "HTTP/1.0";
        },
        http::Version::HTTP_11 => {
            http_version_string = "HTTP/1.1";
        },
        http::Version::HTTP_2 => {
            http_version_string = "HTTP/2.0";
        },
        http::Version::HTTP_3 => {
            http_version_string = "HTTP/3.0";
        },
        _ => {
            log::warn!("Unrecognized request protocol, falling back to HTTP/1.0");
            http_version_string = "HTTP/1.0";
        }
    }
    let write_result = write!(response_writestream, "{} {} {}\r\n", http_version_string, response.status().as_str(), response.status().canonical_reason().unwrap_or(""));
    if write_result.is_err() {
        log::error!("Response write error: {}", write_result.unwrap_err());
        return control_result;
    }

    for response_header in response.headers() {
        let write_result = write!(response_writestream, "{}: {}\r\n", response_header.0, response_header.1.to_str().unwrap_or(""));
        if write_result.is_err() {
            log::error!("Response write error: {}", write_result.unwrap_err());
            return control_result;
        }
    }
    let write_result = write!(response_writestream, "\r\n");
    if write_result.is_err() {
        log::error!("Response write error: {}", write_result.unwrap_err());
        return control_result;
    }

    let write_result = write!(response_writestream, "{}\r\n", response.body().to_string());
    if write_result.is_err() {
        log::error!("Response write error: {}", write_result.unwrap_err());
        return control_result;
    }

    return control_result;
}

fn main() {
    let log_writer = structured_logger::json::new_writer(std::io::stdout());
    structured_logger::Builder::new().with_default_writer(log_writer).init();

    log::info!("Starting TCP listener");

    let tcp_listener = std::net::TcpListener::bind("0.0.0.0:8080").unwrap();
    for listen_result in tcp_listener.incoming() {
        let (handler_txchan, handler_rxchan) = std::sync::mpsc::channel();

        match listen_result {
            Ok(mut stream) => {
                std::thread::spawn(move || {
                    let handler_result = handle_request_stream(&mut stream);
                    handler_txchan.send(handler_result).unwrap();
                });
            }
            Err(error) => {
                log::error!("TCP listener error: {}", error);
            }
        }

        let control_result = handler_rxchan.recv().unwrap();
        if control_result.should_stop {
            break;
        }
    }

    log::info!("Shutting down");
}
