use flate2::{write::GzEncoder, Compression};
use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
};

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();
    println!("Listening on 127.0.0.1:4221 ...");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("accepted new connection");
                thread::spawn(|| handle_connection(stream));
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream) {
    let mut buf = [0; 512];
    stream.read(&mut buf).unwrap();
    let request = String::from_utf8_lossy(&buf[..]);

    let request_lines: Vec<&str> = request.split("\r\n").collect();
    let request_line = request_lines[0];

    if request_lines[0].starts_with("POST ") {
        let response = handle_post(
            request_line,
            request_lines[2].to_string(),
            request_lines[5].to_string(),
        )
        .expect("Failed to handle POST request");

        stream
            .write(response.as_bytes())
            .expect("Failed to write response to stream");
        return;
    }

    if request_line == "GET / HTTP/1.1" {
        stream
            .write("HTTP/1.1 200 OK\r\n\r\n".as_bytes())
            .expect("GET / failed");
        return;
    }

    if request_line.starts_with("GET /echo") && request_line.ends_with(" HTTP/1.1") {
        let content_type: Vec<&str>;
        if request_lines.len() > 4 {
            content_type = request_lines[2].split(" ").collect();
        } else {
            content_type = vec!["", ""];
        }

        let (header, body) = get_url_path(request_line, content_type);
        stream
            .write(header.as_bytes())
            .expect("Failed to write header to stream");
        if !body.is_empty() {
            stream.write(&body).expect("Failed to write body to stream");
        }
        return;
    }

    if request_line == "GET /user-agent HTTP/1.1" {
        let mut user_agent = String::new();
        for line in request_lines {
            if line.starts_with("User-Agent:") {
                user_agent = line[12..].trim().to_string();
                break;
            }
            if line.is_empty() {
                break;
            }
        }
        stream
            .write(handle_user_agent(user_agent.as_str()).as_str().as_bytes())
            .expect("GET /user-agent failed");
        return;
    }

    if request_line.starts_with("GET /files/") && request_line.ends_with(" HTTP/1.1") {
        stream
            .write(handle_file_request(request_line).as_bytes())
            .expect("GET /files response failed");
        return;
    } else {
        stream
            .write("HTTP/1.1 404 Not Found\r\n\r\n".as_bytes())
            .expect("HTTP 404 path failed");
        return;
    }
}

fn get_url_path(request_line: &str, content_type: Vec<&str>) -> (String, Vec<u8>) {
    let split_http_request_whitespaces = request_line.split_whitespace().nth(1).unwrap();
    let url_path_wildcard: Vec<&str> = split_http_request_whitespaces.split("/echo/").collect();
    let response: String;
    let mut content_type_checker = false;

    for x in content_type {
        if x.starts_with("gzip") {
            content_type_checker = true
        }
    }

    if content_type_checker {
        let (compressed, len) = get_gzip(url_path_wildcard[1].to_string());
        response = format!(
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\n\r\n",
            len
        );
        return (response, compressed);
    } else {
        response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
            url_path_wildcard[1].len().to_string(),
            url_path_wildcard[1].to_string()
        );
        return (response, Vec::new());
    }
}
fn handle_user_agent(user_agent: &str) -> String {
    let trimmed_user_agent = user_agent.trim();
    let content_length = trimmed_user_agent.len();

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
        content_length, trimmed_user_agent
    );

    response
}

fn handle_file_request(request_line: &str) -> String {
    let response: String;
    let split_http_request_whitespaces = request_line.split_whitespace().nth(1).unwrap();
    let url_path_wildcard: Vec<&str> = split_http_request_whitespaces.split("/files/").collect();

    let env_args: Vec<String> = env::args().collect();
    let mut dir = env_args[2].clone();
    dir.push_str(url_path_wildcard[1]);

    let file = fs::read(dir);

    match file {
        Ok(f) => {
            response = format! {
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length:{}\r\n\r\n{}"
                ,f.len(),String::from_utf8(f).expect("Didn't read file contents")};
        }
        Err(..) => response = format! {"HTTP/1.1 404 Not Found\r\n\r\n"},
    }

    response
}

fn handle_post(
    request_line: &str,
    requset: String,
    contents: String,
) -> Result<String, std::io::Error> {
    let length_finder: Vec<&str> = requset.split(" ").collect();
    let my_int: i32 = length_finder[1].parse().unwrap();

    let split_http_request_whitespaces = request_line.split_whitespace().nth(1).unwrap();
    let url_path_wildcard: Vec<&str> = split_http_request_whitespaces.split("/files/").collect();

    if url_path_wildcard.len() < 2 {
        return Ok("HTTP/1.1 400 Bad Request\r\n\r\n".to_string());
    }

    let env_args: Vec<String> = env::args().collect();
    if env_args.len() < 3 {
        return Ok("HTTP/1.1 500 Internal Server Error\r\n\r\n".to_string());
    }

    let mut dir = env_args[2].clone();
    dir.push_str("/");
    dir.push_str(url_path_wildcard[1]);

    let file = File::create(&dir);

    let response = match file {
        Ok(mut f) => {
            f.write_all(contents[0..my_int as usize].as_bytes())?;
            format!("HTTP/1.1 201 Created\r\n\r\n")
        }
        Err(e) => {
            println!("Failed to create file: {}", e);
            format!("HTTP/1.1 404 Not Found\r\n\r\n")
        }
    };

    Ok(response)
}

fn get_gzip(data: String) -> (Vec<u8>, usize) {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data.as_bytes()).unwrap();
    let compressed = encoder.finish().unwrap();
    let len = compressed.len();
    (compressed, len)
}
