//! 监听器辅助：优先 IPv6 双栈，回退 IPv4。

use std::net::SocketAddr;
use tokio::net::TcpListener;

pub async fn create_dual_stack_listener(host: &str, port: u16) -> std::io::Result<TcpListener> {
    let addr: SocketAddr = format!("[{}]:{}", host, port)
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    TcpListener::bind(addr).await
}

pub async fn create_ipv4_listener(host: &str, port: u16) -> std::io::Result<TcpListener> {
    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    TcpListener::bind(addr).await
}
