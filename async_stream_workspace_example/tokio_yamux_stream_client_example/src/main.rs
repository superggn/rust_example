use anyhow::Result;
use futures::prelude::*;

use tokio::net::TcpStream;
use tokio_util::{
  codec::{Framed, LinesCodec},
  compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt},
};
use tracing::info;
use yamux::{Config, Connection, Mode};

#[tokio::main]
async fn main() -> Result<()> {
  tracing_subscriber::fmt::init();
  let tcp_stream = TcpStream::connect("127.0.01:8080").await?;
  info!("Connected to server");
  let mut config = Config::default();
  config.set_split_send_size(4 * 1024);
  let mut yamux_conn = Connection::new(tcp_stream.compat(), config, Mode::Client);

  // poll 所有 stream 下的数据
  // 这里用的是 future::poll_fn, 正常 poll_new_outbound(cx)
  // 所以我那边的问题不是拿不到 yamuxstream, 而是拿到的 stream 不对
  // client 这边新开一个 substream， 后面处理下， 继续弄别的
  let substream = future::poll_fn(|cx| yamux_conn.poll_new_outbound(cx))
    .await
    .unwrap();

  // 可能会有 new incoming yamux substream， 啥都不干
  // for new substreams opened by server side => do nothing
  let incoming_substreams = futures::stream::poll_fn(move |cx| yamux_conn.poll_next_inbound(cx));
  tokio::spawn(noop_server(incoming_substreams));

  let compat = substream.compat();
  info!("Started a new stream");
  let mut framed = Framed::new(compat, LinesCodec::new());
  loop {
    tokio::select! {
      result = framed.send("Hello, this is Tyr!".to_string()).fuse() => {
        // Handle the result of the send operation
        if let Err(err) = result {
          eprintln!("Error sending message: {:?}", err);
          break
          // Optionally: return Err(err) or take other actions
        }
      },
    }

    tokio::select! {
      response = framed.next().fuse() => {
        // Handle the received response
        if let Some(Ok(line)) = response {
          println!("Got: {}", line);
        }
        // Optionally: Handle other cases if needed
      },
    }
  }
  Ok(())
}

/// For each incoming stream, do nothing.
/// yamux stream => 进来一个就 drop 一个
pub async fn noop_server(c: impl Stream<Item = Result<yamux::Stream, yamux::ConnectionError>>) {
  c.for_each(|maybe_stream| {
    drop(maybe_stream);
    future::ready(())
  })
  .await;
}

// copy from https://github.com/tyrchen/geektime-rust
// modified with chatpgpt
