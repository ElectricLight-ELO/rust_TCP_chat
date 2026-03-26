use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const CHUNK_SIZE: usize = 1024;

async fn read_all(stream: &mut TcpStream) -> io::Result<String> {
    let peer = stream.peer_addr()?;

    // 1. Читаем размер (4 байта, big-endian = network byte order)
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let data_length = u32::from_be_bytes(len_buf) as usize;
    println!("  [server] ожидаю {} байт от {}", data_length, peer);

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
            "  [server] чанк #{}: прочитано {} байт ({}/{})",
            chunk_index, current_chunk, bytes_read, data_length
        );
    }

    Ok(String::from_utf8_lossy(&buffer).to_string())
}

async fn send_all(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let data_length = data.len();

    // 1. Отправляем размер (4 байта, big-endian = htonl)
    let network_length = (data_length as u32).to_be_bytes();
    stream.write_all(&network_length).await?;
    println!("  [server] отправляю размер: {} байт", data_length);

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
            "  [server] чанк #{}: отправлено {} байт ({}/{})",
            chunk_index, current_chunk, bytes_sent, data_length
        );
    }

    Ok(())
}

async fn handle_client(mut stream: TcpStream) {
    let peer = stream.peer_addr().unwrap();
    println!("\n[+] Подключился: {} (задача {:?})", peer, tokio::task::id());

    match read_all(&mut stream).await {
        Ok(msg) => {
            println!("  Сообщение от {}:", peer);
            println!("  > {}", msg);

            let ack = format!("OK: получено {} байт", msg.len());
            if let Err(e) = send_all(&mut stream, ack.as_bytes()).await {
                eprintln!("[error] ACK не отправлен: {}", e);
            }
        }
        Err(e) => {
            eprintln!("[error] Ошибка чтения: {}", e);
        }
    }

    println!("[-] Отключился: {}", peer);
}

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:7878";
    let listener = TcpListener::bind(addr).await.expect("Не удалось занять порт 7878");

    println!("TCP-сервер запущен (tokio)");
    println!("Слушаю на {}", addr);

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle_client(stream));
            }
            Err(e) => eprintln!("[error] {}", e),
        }
    }
}
