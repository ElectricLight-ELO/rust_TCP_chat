use std::io::Write;
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{tcp::OwnedWriteHalf, TcpStream};

const CHUNK_SIZE: usize = 1024;

/// Отправляет одно length-prefixed сообщение.
async fn send_message(writer: &mut OwnedWriteHalf, data: &[u8]) -> io::Result<()> {
    let data_length = data.len();

    // Длина (4 байта, big-endian)
    writer.write_all(&(data_length as u32).to_be_bytes()).await?;

    // Payload чанками
    let mut bytes_sent = 0usize;
    let mut chunk_index = 0usize;

    while bytes_sent < data_length {
        let current_chunk = (data_length - bytes_sent).min(CHUNK_SIZE);
        writer.write_all(&data[bytes_sent..bytes_sent + current_chunk]).await?;
        writer.flush().await?;
        bytes_sent += current_chunk;
        chunk_index += 1;
     //   println!("  [client] чанк #{}: {} байт ({}/{})", chunk_index, current_chunk, bytes_sent, data_length);
    }

    Ok(())
}

/// Читает одно length-prefixed сообщение.
/// Возвращает None если сервер закрыл соединение.
async fn read_message<R: AsyncReadExt + Unpin>(reader: &mut R) -> io::Result<Option<String>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }

    let data_length = u32::from_be_bytes(len_buf) as usize;

    let mut buffer = vec![0u8; data_length];
    let mut bytes_read = 0usize;
    let mut chunk_index = 0usize;

    while bytes_read < data_length {
        let current_chunk = (data_length - bytes_read).min(CHUNK_SIZE);
        reader.read_exact(&mut buffer[bytes_read..bytes_read + current_chunk]).await?;
        bytes_read += current_chunk;
        chunk_index += 1;
      //  println!("  [client] чанк #{}: {} байт ({}/{})", chunk_index, current_chunk, bytes_read, data_length );
    }

    Ok(Some(String::from_utf8_lossy(&buffer).to_string()))
}

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:7878";

    println!("Rust TCP-клиент (tokio)");

    let stdin = tokio::io::stdin();
    let mut stdin_lines = BufReader::new(stdin).lines();

    // Запрашиваем nickname до подключения
    let nickname = loop {
        print!("Введите nickname: ");
        std::io::stdout().flush().unwrap();

        let line = match stdin_lines.next_line().await {
            Ok(Some(l)) => l,
            Ok(None) => { println!("EOF — выход."); return; }
            Err(e) => { eprintln!("[ошибка] {}", e); return; }
        };

        let nickname = line.trim().to_string();
        if nickname.is_empty() {
            println!("Nickname не может быть пустым.");
            continue;
        }

        break nickname;
    };

    // Устанавливаем соединение только после ввода nickname
    let stream = match TcpStream::connect(addr).await {
        Ok(s) => {
            println!("Подключён к {} как {}\n", addr, nickname);
            s
        }
        Err(e) => {
            eprintln!("[ошибка] Не удалось подключиться к {}: {}", addr, e);
            return;
        }
    };

    // Разделяем стрим на чтение и запись
    let (mut reader, mut writer) = stream.into_split();

    // Отправляем nickname серверу
    if let Err(e) = send_message(&mut writer, nickname.as_bytes()).await {
        eprintln!("[ошибка] Сервер недоступен при отправке nickname: {}", e);
        return;
    }

    // Читаем ответ на авторизацию (приветствие или ошибка занятого ника)
    match read_message(&mut reader).await {
        Ok(Some(ack)) if ack.starts_with("Ошибка") => {
            eprintln!("[сервер] {}", ack);
            return;
        }
        Ok(Some(ack)) => println!("[сервер] {}\n", ack),
        Ok(None) | Err(_) => {
            eprintln!("[ошибка] Сервер недоступен или закрыл соединение.");
            return;
        }
    }

    // Два параллельных таска:
    // 1) читаем ввод пользователя и отправляем серверу
    // 2) принимаем входящие сообщения от сервера
    loop {

        println!("Введите сообщение (или 'exit' для выхода): ");
        tokio::select! {
            // Ввод пользователя
            
            line = stdin_lines.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        let message = l.trim().to_string();

                        if message.eq_ignore_ascii_case("exit") {
                            println!("Выход.");
                            break;
                        }
                        if message.is_empty() {
                            continue;
                        }

                        if let Err(_) = send_message(&mut writer, message.as_bytes()).await {
                            eprintln!("[ошибка] Сервер недоступен, соединение разорвано.");
                            break;
                        }
                    }
                    Ok(None) => {
                        println!("EOF — выход.");
                        break;
                    }
                    Err(e) => {
                        eprintln!("[ошибка] {}", e);
                        break;
                    }
                }
            }

            // Входящие сообщения от сервера
            msg = read_message(&mut reader) => {
                match msg {
                    Ok(Some(text)) => {
                        println!("Сообщение > {}", text);
                    }
                    Ok(None) => {
                        eprintln!("[ошибка] Сервер закрыл соединение.");
                        
                        break;
                    }
                    Err(_) => {
                        eprintln!("[ошибка] Сервер недоступен, соединение разорвано.");
                        break;
                    }
                }
            }
        }
    }
}
