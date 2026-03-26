use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const CHUNK_SIZE: usize = 1024;

// ─── Отправка: размер (4 байта big-endian) + данные чанками ──────────────────
async fn send_all(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let data_length = data.len();

    // 1. Отправляем размер (htonl → big-endian)
    let network_length = (data_length as u32).to_be_bytes();
    stream.write_all(&network_length).await?;
    println!("  [client] отправляю размер: {} байт", data_length);

    // 2. Отправляем данные чанками
    let mut bytes_sent = 0usize;
    let mut chunk_index = 0usize;

    while bytes_sent < data_length {
        let remaining = data_length - bytes_sent;
        let current_chunk = remaining.min(CHUNK_SIZE);

        stream.write_all(&data[bytes_sent..bytes_sent + current_chunk]).await?;
        stream.flush().await?;

        bytes_sent += current_chunk;
        chunk_index += 1;
        println!(
            "  [client] чанк #{}: отправлено {} байт ({}/{})",
            chunk_index, current_chunk, bytes_sent, data_length
        );
    }

    Ok(())
}

// ─── Чтение: сначала 4 байта длины, затем payload чанками ────────────────────
async fn read_all(stream: &mut TcpStream) -> io::Result<String> {
    // 1. Читаем размер (ntohl → from big-endian)
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let data_length = u32::from_be_bytes(len_buf) as usize;
    println!("  [client] ожидаю {} байт от сервера", data_length);

    // 2. Читаем данные чанками
    let mut buffer = vec![0u8; data_length];
    let mut bytes_read = 0usize;
    let mut chunk_index = 0usize;

    while bytes_read < data_length {
        let remaining = data_length - bytes_read;
        let current_chunk = remaining.min(CHUNK_SIZE);

        stream.read_exact(&mut buffer[bytes_read..bytes_read + current_chunk]).await?;

        bytes_read += current_chunk;
        chunk_index += 1;
        println!(
            "  [client] чанк #{}: прочитано {} байт ({}/{})",
            chunk_index, current_chunk, bytes_read, data_length
        );
    }

    Ok(String::from_utf8_lossy(&buffer).to_string())
}

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:7878";

    println!("Rust TCP-клиент (tokio)");
    println!("Подключаюсь к {}", addr);

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    loop {
        print!("\nВведите сообщение (или 'exit'): ");
       
        use std::io::Write;
        std::io::stdout().flush().unwrap();

        let line = match reader.next_line().await {
            Ok(Some(l)) => l,
            Ok(None) => break, // EOF
            Err(e) => { eprintln!("[error] {}", e); break; }
        };

        let message = line.trim().to_string();

        if message.eq_ignore_ascii_case("exit") {
            println!("Выход!!!");
            break;
        }
        if message.is_empty() { continue; }

        match TcpStream::connect(addr).await {
            Ok(mut stream) => {
                println!("\n[client] Отправка \"{}\"...", message);

                if let Err(e) = send_all(&mut stream, message.as_bytes()).await {
                    eprintln!("[error] {}", e);
                    continue;
                }

                match read_all(&mut stream).await {
                    Ok(ack) => println!("\n[client] Ответ сервера: \"{}\"", ack),
                    Err(e)  => eprintln!("[error] {}", e),
                }
            }
            Err(e) => eprintln!("[error] Не удалось подключиться: {}", e),
        }
    }
}
