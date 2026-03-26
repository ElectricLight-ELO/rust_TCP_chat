use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const CHUNK_SIZE: usize = 1024;

/// Читает одно length-prefixed сообщение.
/// Возвращает None если клиент закрыл соединение (EOF при чтении длины).
async fn read_message(stream: &mut TcpStream) -> io::Result<Option<String>> {
    // 1. Читаем 4 байта длины (big-endian)
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let data_length = u32::from_be_bytes(len_buf) as usize;
    println!("  [server] ожидаю {} байт", data_length);

    // 2. Читаем payload чанками
    let mut buffer = vec![0u8; data_length];
    let mut bytes_read = 0usize;
    let mut chunk_index = 0usize;

    while bytes_read < data_length {
        let current_chunk = (data_length - bytes_read).min(CHUNK_SIZE);
        stream.read_exact(&mut buffer[bytes_read..bytes_read + current_chunk]).await?;
        bytes_read += current_chunk;
        chunk_index += 1;
        println!(
            "  [server] чанк #{}: {} байт ({}/{})",
            chunk_index, current_chunk, bytes_read, data_length
        );
    }

    Ok(Some(String::from_utf8_lossy(&buffer).to_string()))
}

/// Отправляет одно length-prefixed сообщение.
async fn send_message(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let data_length = data.len();

    // 1. Отправляем длину (4 байта, big-endian)
    stream.write_all(&(data_length as u32).to_be_bytes()).await?;

    // 2. Отправляем payload чанками
    let mut bytes_sent = 0usize;
    let mut chunk_index = 0usize;

    while bytes_sent < data_length {
        let current_chunk = (data_length - bytes_sent).min(CHUNK_SIZE);
        stream.write_all(&data[bytes_sent..bytes_sent + current_chunk]).await?;
        stream.flush().await?;
        bytes_sent += current_chunk;
        chunk_index += 1;
        println!(
            "  [server] ACK чанк #{}: {} байт ({}/{})",
            chunk_index, current_chunk, bytes_sent, data_length
        );
    }

    Ok(())
}

/// Обрабатывает клиента: читает сообщения в цикле, на каждое отвечает ACK.
/// Цикл завершается когда клиент закрывает соединение.
async fn handle_client(mut stream: TcpStream) {
    let peer = stream.peer_addr().unwrap();
    println!("\n[+] Подключился: {}", peer);

    loop {
        match read_message(&mut stream).await {
            Ok(Some(msg)) => {
                println!("  [{}] > {}", peer, msg);

                let ack = format!("OK: получено {} байт", msg.len());
                if let Err(e) = send_message(&mut stream, ack.as_bytes()).await {
                    eprintln!("[error] Не удалось отправить ACK клиенту {}: {}", peer, e);
                    break;
                }
            }
            Ok(None) => {
                // Клиент закрыл соединение
                println!("[-] Клиент {} закрыл соединение", peer);
                break;
            }
            Err(e) => {
                eprintln!("[error] Ошибка чтения от {}: {}", peer, e);
                break;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:7878";
    let listener = TcpListener::bind(addr).await.expect("Не удалось занять порт 7878");

    println!("TCP-сервер запущен (tokio)");
    println!("Слушаю на {}\n", addr);

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle_client(stream));
            }
            Err(e) => eprintln!("[error] accept: {}", e),
        }
    }
}
