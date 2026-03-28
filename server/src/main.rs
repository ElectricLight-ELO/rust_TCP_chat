use std::{collections::HashMap, sync::OnceLock, time::SystemTime};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{tcp::OwnedWriteHalf, TcpListener, TcpStream};
use tokio::sync::Mutex;

const CHUNK_SIZE: usize = 1024;

struct User {
    nickname: String,
    last_connected_at: SystemTime,
    authorized: bool,
    writer: Option<OwnedWriteHalf>,
}

static USERS: OnceLock<Mutex<HashMap<String, User>>> = OnceLock::new();

fn users() -> &'static Mutex<HashMap<String, User>> {
    USERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Читает одно length-prefixed сообщение.
/// Возвращает None если клиент закрыл соединение (EOF при чтении длины).
async fn read_message<R: AsyncReadExt + Unpin>(stream: &mut R) -> io::Result<Option<String>> {
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }

    let data_length = u32::from_be_bytes(len_buf) as usize;
    println!("  [server] ожидаю {} байт", data_length);

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
async fn send_message<W: AsyncWriteExt + Unpin>(writer: &mut W, data: &[u8]) -> io::Result<()> {
    let data_length = data.len();

    writer.write_all(&(data_length as u32).to_be_bytes()).await?;

    let mut bytes_sent = 0usize;
    let mut chunk_index = 0usize;

    while bytes_sent < data_length {
        let current_chunk = (data_length - bytes_sent).min(CHUNK_SIZE);
        writer.write_all(&data[bytes_sent..bytes_sent + current_chunk]).await?;
        writer.flush().await?;
        bytes_sent += current_chunk;
        chunk_index += 1;
        println!(
            "  [server] ACK чанк #{}: {} байт ({}/{})",
            chunk_index, current_chunk, bytes_sent, data_length
        );
    }

    Ok(())
}

/// Рассылает сообщение всем авторизованным пользователям кроме отправителя.
async fn broadcast(sender_nickname: &str, data: &[u8]) {
    let mut users = users().lock().await;
    for (_, user) in users.iter_mut() {
        if user.authorized && user.nickname != sender_nickname {
            if let Some(writer) = user.writer.as_mut() {
                let _ = send_message(writer, data).await;
            }
        }
    }
}

/// Обрабатывает клиента: сначала читает nickname, затем сообщения в цикле.
/// Цикл завершается когда клиент закрывает соединение.
async fn handle_client(stream: TcpStream) {
    let peer = stream.peer_addr().unwrap();
    println!("\n[+] Подключился: {}", peer);

    // Разделяем стрим на чтение и запись
    let (mut reader, mut writer) = stream.into_split();

    let nickname = match read_message(&mut reader).await {
        Ok(Some(name)) => {
            let name = name.trim().to_string();
            if name.is_empty() {
                eprintln!("[ошибка] Клиент {} отправил пустой nickname", peer);
                return;
            }
            println!("[+] {} представился как {}", peer, name);
            name
        }
        Ok(None) => {
            println!("[-] Клиент {} отключился до отправки nickname", peer);
            return;
        }
        Err(e) => {
            eprintln!("[ошибка] Не удалось прочитать nickname от {}: {}", peer, e);
            return;
        }
    };

    {
        let mut users = users().lock().await;

        // Проверяем, что ник не занят активным пользователем
        if let Some(u) = users.get(&nickname) {
            if u.authorized {
                eprintln!("[server] ник \"{}\" уже занят, отклоняем {}", nickname, peer);
                let _ = send_message(&mut writer, "Ошибка: такой ник уже занят".as_bytes()).await;
                return;
            }
        }

        // Сохраняем пользователя как авторизованного со stream writer
        let user = User {
            nickname: nickname.clone(),
            last_connected_at: SystemTime::now(),
            authorized: true,
            writer: Some(writer),
        };
        users.insert(nickname.clone(), user);
        println!(
            "[server] пользователь сохранён: {} ({:?})",
            nickname, users[&nickname].last_connected_at
        );
    }

    // Отправляем клиенту подтверждение успешной авторизации
    {
        let mut users = users().lock().await;
        if let Some(user) = users.get_mut(&nickname) {
            if let Some(writer) = user.writer.as_mut() {
                if let Err(e) = send_message(writer, format!("Добро пожаловать, {}!", nickname).as_bytes()).await {
                    eprintln!("[ошибка] Не удалось отправить приветствие {}: {}", nickname, e);
                    return;
                }
            }
        }
    }

    loop {
        match read_message(&mut reader).await {
            Ok(Some(msg)) => {
                println!("  [{} / {}] > {}", peer, nickname, msg);

                // Рассылаем сообщение всем остальным пользователям
                let broadcast_msg = format!("[{}] {}", nickname, msg);
                broadcast(&nickname, broadcast_msg.as_bytes()).await;
            }
            Ok(None) => {
                println!("[-] Клиент {} ({}) закрыл соединение", peer, nickname);
                break;
            }
            Err(e) => {
                eprintln!("[ошибка] Ошибка чтения от {} ({}): {}", peer, nickname, e);
                break;
            }
        }
    }

    // При отключении снимаем флаг авторизации и обнуляем writer — ник снова становится свободным
    if let Some(u) = users().lock().await.get_mut(&nickname) {
        u.authorized = false;
        u.writer = None;
    }
    println!("[server] пользователь {} отключился, ник освобождён", nickname);
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
            Err(e) => eprintln!("[ошибка] accept: {}", e),
        }
    }
}
