use nezuko::sleep;
use nezuko::{Runtime, accept, print_all, spawn, write_all};
use std::io;
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

async fn one_response(mut socket: TcpStream, n: u64) -> io::Result<()> {
    let start_msg = format!("start {n}\n");
    write_all(start_msg.as_bytes(), &mut socket).await?;
    sleep(Duration::from_secs(1)).await;
    let end_msg = format!("end {n}\n");
    write_all(end_msg.as_bytes(), &mut socket).await?;
    Ok(())
}

async fn server_main(mut listener: TcpListener) -> io::Result<()> {
    let mut n = 1;
    loop {
        let (socket, _) = accept(&mut listener).await?;
        spawn(async move { one_response(socket, n).await.unwrap() });
        n += 1;
    }
}

async fn client_main() -> io::Result<()> {
    let mut socket = TcpStream::connect("localhost:8000")?;
    socket.set_nonblocking(true)?;
    print_all(&mut socket).await?;
    Ok(())
}

#[test]
fn tcp_server_ten_clients() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let listener = TcpListener::bind("0.0.0.0:8000").unwrap();
        listener.set_nonblocking(true).unwrap();

        spawn(async { server_main(listener).await.unwrap() });

        let mut handles = Vec::new();
        for _ in 1..=10 {
            handles.push(spawn(client_main()));
        }
        for handle in handles {
            handle.await.unwrap();
        }
    });
}
