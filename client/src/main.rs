use std::io::Write;
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const CHUNK_SIZE: usize = 1024;


/// Отправляет одно length-prefixed сообщение.
async fn send_message(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let data_length = data.len();

    // 1. Длина (4 байта, big-endian)
    stream.write_all(&(data_length as u32).to_be_bytes()).await?;

    // 2. Payload чанками
    let mut bytes_sent = 0usize;
    let mut chunk_index = 0usize;

    while bytes_sent < data_length {
        let current_chunk = (data_length - bytes_sent).min(CHUNK_SIZE);
        stream.write_all(&data[bytes_sent..bytes_sent + current_chunk]).await?;
        stream.flush().await?;
        bytes_sent += current_chunk;
        chunk_index += 1;
        println!(
            "  [client] чанк #{}: {} байт ({}/{})",
            chunk_index, current_chunk, bytes_sent, data_length
        );
    }

    Ok(())
}

/// Читает одно length-prefixed сообщение (ACK от сервера).
async fn read_message(stream: &mut TcpStream) -> io::Result<String> {
    // 1. Читаем длину
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let data_length = u32::from_be_bytes(len_buf) as usize;

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
            "  [client] ACK чанк #{}: {} байт ({}/{})",
            chunk_index, current_chunk, bytes_read, data_length
        );
    }

    Ok(String::from_utf8_lossy(&buffer).to_string())
}

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:7878";

    println!("Rust TCP-клиент (tokio)");

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    let nickname = loop {
        print!("Введите nickname: ");
        std::io::stdout().flush().unwrap();

        let line = match reader.next_line().await {
            Ok(Some(l)) => l,
            Ok(None) => {
                println!("EOF — выход.");
                return;
            }
            Err(e) => {
                eprintln!("[error] {}", e);
                return;
            }
        };

        let nickname = line.trim().to_string();
        if nickname.is_empty() {
            println!("Nickname не может быть пустым.");
            continue;
        }

        break nickname;
    };

    // Устанавливаем соединение только после ввода nickname
    let mut stream = match TcpStream::connect(addr).await {
        Ok(s) => {
            println!("Подключён к {} как {}\n", addr, nickname);
            s
        }
        Err(e) => {
            eprintln!("[error] Не удалось подключиться к {}: {}", addr, e);
            return;
        }
    };

    // Сразу после подключения отправляем nickname серверу
    if let Err(e) = send_message(&mut stream, nickname.as_bytes()).await {
        eprintln!("[error] Не удалось отправить nickname: {}", e);
        return;
    }

    // Цикл отправки сообщений по одному соединению
    loop {
        print!("[{}] Введите сообщение (или 'exit'): ", nickname);
        std::io::stdout().flush().unwrap();

        let line = match reader.next_line().await {
            Ok(Some(l)) => l,
            Ok(None) => {
                println!("EOF — выход.");
                break;
            }
            Err(e) => {
                eprintln!("[error] {}", e);
                break;
            }
        };

        let message = line.trim().to_string();

        if message.eq_ignore_ascii_case("exit") {
            println!("Выход.");
            break;
        }
        if message.is_empty() {
            continue;
        }

        // Отправляем сообщение
        if let Err(e) = send_message(&mut stream, message.as_bytes()).await {
            eprintln!("[error] Ошибка отправки: {}", e);
            break;
        }

        // Ждём ACK от сервера
        match read_message(&mut stream).await {
            Ok(ack) => println!("[client] Сервер: \"{}\"\n", ack),
            Err(e) => {
                eprintln!("[error] Ошибка получения ACK: {}", e);
                break;
            }
        }
    }
}
