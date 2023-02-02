use async_std::{
    self,
    net::{TcpListener},
    task::{spawn},
};
use futures::stream::StreamExt;
use my_server::handle_connection;


#[async_std::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:7878").await.unwrap();
    listener
        .incoming()
        .for_each_concurrent(/* limit */ None, |tcpstream| async move {
            let tcpstream = tcpstream.unwrap();
            spawn(handle_connection(tcpstream));
        })
        .await;
}
