use std::io::{self, BufRead, Read, Write};
use std::net::TcpStream;

const CHUNK_SIZE: usize = 1024;

// ─── Аналог send_all из C++ ───────────────────────────────────────────────────
fn send_all(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let data_length = data.len();

    // 1. Отправляем размер (htonl → big-endian)
    let network_length = (data_length as u32).to_be_bytes();
    stream.write_all(&network_length)?;
    println!("  [client] отправляю размер: {} байт", data_length);

    // 2. Отправляем данные чанками
    let mut bytes_sent = 0usize;
    let mut chunk_index = 0usize;

    while bytes_sent < data_length {
        let remaining = data_length - bytes_sent;
        let current_chunk = remaining.min(CHUNK_SIZE);

        stream.write_all(&data[bytes_sent..bytes_sent + current_chunk])?;
        stream.flush()?;

        bytes_sent += current_chunk;
        chunk_index += 1;
        println!(
            "  [client] чанк #{}: отправлено {} байт ({}/{})",
            chunk_index, current_chunk, bytes_sent, data_length
        );
    }

    Ok(())
}

// ─── Аналог read_all из C++ ───────────────────────────────────────────────────
fn read_all(stream: &mut TcpStream) -> io::Result<String> {
    // 1. Читаем размер (ntohl → from big-endian)
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let data_length = u32::from_be_bytes(len_buf) as usize;
    println!("  [client] ожидаю {} байт от сервера", data_length);

    // 2. Читаем данные чанками
    let mut buffer = vec![0u8; data_length];
    let mut bytes_read = 0usize;
    let mut chunk_index = 0usize;

    while bytes_read < data_length {
        let remaining = data_length - bytes_read;
        let current_chunk = remaining.min(CHUNK_SIZE);

        stream.read_exact(&mut buffer[bytes_read..bytes_read + current_chunk])?;

        bytes_read += current_chunk;
        chunk_index += 1;
        println!(
            "  [client] чанк #{}: прочитано {} байт ({}/{})",
            chunk_index, current_chunk, bytes_read, data_length
        );
    }

    Ok(String::from_utf8_lossy(&buffer).to_string())
}

fn main() {
    let addr = "127.0.0.1:7878";

    println!("Rust TCP-клиент");
    println!("Подключаюсь к {}", addr);

    let stdin = io::stdin();

    loop {
        print!("\nВведите сообщение (или 'exit'): ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        stdin.lock().read_line(&mut line).unwrap();
        let message = line.trim();

        if message.eq_ignore_ascii_case("exit") {
            println!("Выход.");
            break;
        }
        if message.is_empty() { continue; }

        match TcpStream::connect(addr) {
            Ok(mut stream) => {
                println!("\n[client] Отправка \"{}\"...", message);

                if let Err(e) = send_all(&mut stream, message.as_bytes()) {
                    eprintln!("[error] {}", e);
                    continue;
                }

                match read_all(&mut stream) {
                    Ok(ack) => println!("\n[client] Ответ сервера: \"{}\"", ack),
                    Err(e)  => eprintln!("[error] {}", e),
                }
            }
            Err(e) => eprintln!("[error] Не удалось подключиться: {}", e),
        }
    }
}