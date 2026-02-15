use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use tetris_application::ports::NetClientPort;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetMessage {
    Start(u64),
    Garbage(u32),
    Over,
    Ready,
    Disconnect,
}

#[derive(Debug)]
pub struct NetLobby {
    listener: TcpListener,
    pub port: u16,
}

impl NetLobby {
    pub fn bind_any() -> io::Result<Self> {
        let listener = TcpListener::bind("0.0.0.0:0")?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        Ok(Self { listener, port })
    }

    pub fn try_accept(&self) -> io::Result<Option<TcpStream>> {
        match self.listener.accept() {
            Ok((stream, _)) => Ok(Some(stream)),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[derive(Debug)]
pub struct NetClient {
    stream: TcpStream,
    rx: Receiver<NetMessage>,
}

impl NetClient {
    pub fn connect(addr: &str) -> io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        Self::from_stream(stream)
    }

    pub fn from_stream(stream: TcpStream) -> io::Result<Self> {
        stream.set_nodelay(true)?;
        let read_stream = stream.try_clone()?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || read_loop(read_stream, tx));
        Ok(Self { stream, rx })
    }

    pub fn send_start(&mut self, seed: u64) -> io::Result<()> {
        self.send_line(&format!("START {}", seed))
    }

    pub fn send_garbage(&mut self, lines: u32) -> io::Result<()> {
        self.send_line(&format!("G {}", lines))
    }

    pub fn send_over(&mut self) -> io::Result<()> {
        self.send_line("OVER")
    }

    pub fn send_ready(&mut self) -> io::Result<()> {
        self.send_line("READY")
    }

    pub fn try_recv(&mut self) -> Option<NetMessage> {
        self.rx.try_recv().ok()
    }

    fn send_line(&mut self, line: &str) -> io::Result<()> {
        self.stream.write_all(line.as_bytes())?;
        self.stream.write_all(b"\n")?;
        self.stream.flush()
    }
}

fn read_loop(stream: TcpStream, tx: mpsc::Sender<NetMessage>) {
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                let _ = tx.send(NetMessage::Disconnect);
                break;
            }
            Ok(_) => {
                if let Some(msg) = parse_message(&line) {
                    let _ = tx.send(msg);
                }
            }
            Err(_) => {
                let _ = tx.send(NetMessage::Disconnect);
                break;
            }
        }
    }
}

fn parse_message(line: &str) -> Option<NetMessage> {
    let trimmed = line.trim();
    if trimmed.starts_with("START ") {
        let seed = trimmed[6..].trim().parse().ok()?;
        return Some(NetMessage::Start(seed));
    }
    if trimmed.starts_with("G ") {
        let lines = trimmed[2..].trim().parse().ok()?;
        return Some(NetMessage::Garbage(lines));
    }
    if trimmed == "READY" {
        return Some(NetMessage::Ready);
    }
    if trimmed == "OVER" {
        return Some(NetMessage::Over);
    }
    None
}

impl NetClientPort for NetClient {
    type Error = io::Error;
    type Message = NetMessage;

    fn send_ready(&mut self) -> Result<(), Self::Error> {
        NetClient::send_ready(self)
    }

    fn send_start(&mut self, seed: u64) -> Result<(), Self::Error> {
        NetClient::send_start(self, seed)
    }

    fn send_garbage(&mut self, lines: u32) -> Result<(), Self::Error> {
        NetClient::send_garbage(self, lines)
    }

    fn send_over(&mut self) -> Result<(), Self::Error> {
        NetClient::send_over(self)
    }

    fn try_recv(&mut self) -> Option<Self::Message> {
        NetClient::try_recv(self)
    }
}
